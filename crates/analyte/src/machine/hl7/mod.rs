//! Drives HL7 connections and chooses ACKs after durable interpretation.
use crate::machine::hooks::Pause;
pub(crate) mod location;
use super::connection_input::{self, Input};
use super::driver::{self, Context, Effects, Failures, Flow, Run};
use crate::intake;
mod error;
// A stored parking reason; `receive::bytes` pins its spelling.
pub(crate) const UNSOLICITED_REASON: &str = "HL7 unsolicited message or queue overflow";
pub(crate) mod orders;
pub(crate) mod settings;
use crate::machine::receive::custody::{self, WriteDeadline};
use analyte_dialog::hl7::outcome::Code as Hl7End;
use analyte_dialog::hl7::{Action, Dialog, Event, Expect};
use analyte_model::ReceiptId;
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::watch;
#[cfg(test)]
mod tests;

struct DisplayOutcome {
    receipt: Option<analyte_dialog::hl7::ExchangeId>,
    accepted: bool,
    code: &'static str,
    detail: Option<String>,
}

struct QueryRequest {
    token: analyte_dialog::hl7::ExchangeId,
    message: analyte_hl7::Message,
    started: Duration,
    edition_matches: bool,
}

/// Bounds receipts awaiting durable completion within a connection.
const MAX_PENDING_RECEIPTS: usize = 2;

pub(super) struct Host<'a, 'm> {
    machine: &'a intake::ConnectionEvidence<'m>,
    store: &'a Handle,
    dialog: Dialog,
    start: Instant,
    wake: Option<Duration>,
    pending: BTreeMap<analyte_dialog::ExchangeId, ReceiptId>,
    queries: Option<orders::Hl7OrderLink<'a>>,
    input_evidence: connection_input::Custody,
}

impl<'a, 'm> Host<'a, 'm> {
    pub(super) fn open(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a analyte_config::Hl7Stack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let (options, config) = settings::dialog(settings)?;
        let mut dialog = Dialog::new(
            config,
            analyte_dialog::Epoch(uuid::Uuid::now_v7().as_u128()),
        );
        let start = Instant::now();
        let wake = None;
        let pending = BTreeMap::new();
        let queries = match settings.orders() {
            Some(orders) if orders.query().is_some() || orders.send().is_some() => {
                dialog.enable_orders(settings.settings().syntax().edition().into())?;
                Some(orders::Hl7OrderLink::new(settings, orders, options)?)
            }
            Some(_) | None => None,
        };
        let input_evidence = connection_input::Custody::default();
        Ok(Self {
            machine,
            store,
            dialog,
            start,
            wake,
            pending,
            queries,
            input_evidence,
        })
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        self.input_evidence.begin(&bytes);
        let events = if bytes.is_empty() {
            vec![Event::Tick]
        } else {
            bytes.into_iter().map(Event::Byte).collect()
        };
        for event in events {
            if matches!(event, Event::Byte(_)) {
                self.input_evidence.fed();
            }
            if self.transition(event, conn, stop).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }
    async fn transition(
        &mut self,
        event: Event<'_>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let actions = self.dialog.step(self.start.elapsed(), event)?;
        driver::drain(self, actions, conn, stop).await
    }
}

impl driver::Drain for Host<'_, '_> {
    type Action = Action;
    async fn effect(
        &mut self,
        action: Action,
        run: &mut Run<'_, Action, Error>,
    ) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        match action {
            Action::ReceiptRetained(token) => {
                self.pending
                    .remove(&token)
                    .ok_or(Error::LateReceiptAbsent)?;
            }
            Action::Query {
                receipt: token,
                message,
                started,
                edition_matches,
            } => {
                return self
                    .answer_query(
                        QueryRequest {
                            token,
                            message,
                            started,
                            edition_matches,
                        },
                        machine,
                        store,
                        run.stop,
                        &mut run.actions,
                    )
                    .await;
            }
            Action::OrderResponse {
                receipt: token,
                message,
            } => {
                self.order_response(token, &message, store).await?;
            }
            Action::OrderResponseTimeout => {
                self.queries
                    .as_mut()
                    .ok_or(Error::OrderServicesAbsent)?
                    .order_response(store, None)
                    .await?;
            }
            Action::DisplayOutcome {
                receipt,
                accepted,
                code,
                detail,
            } => {
                self.display_outcome(
                    DisplayOutcome {
                        receipt,
                        accepted,
                        code,
                        detail,
                    },
                    machine,
                    store,
                )
                .await?;
            }
            Action::Store(token, raw) => {
                self.commit(token, raw, run).await?;
            }
            Action::Interpret(token, _payload) => {
                self.interpret(token, machine, store, &mut run.actions)
                    .await?;
            }
            Action::ResponseNeeded(token, _) => {
                self.respond(token, &mut run.actions)?;
            }
            Action::QueryWrite { bytes, until, part } => {
                let (actions, failures) = (&mut run.actions, &mut run.failures);
                self.write_query(&bytes, until, part, run.conn, actions, failures)
                    .await?;
            }
            Action::Write(write) => self.write(&write, run.conn).await?,
            Action::Deadline(next) => self.wake = next,
            Action::Rejected(error, bytes) => {
                intake::park_bytes(store, machine, bytes, &error.to_string()).await?;
            }
            Action::Parked(bytes, error) => {
                self.pending.clear();
                intake::park_bytes(store, machine, bytes, &error.to_string()).await?;
            }
            Action::QueueFull(bytes) | Action::Unmatched(bytes) => {
                return self.refuse_message(machine, store, bytes).await;
            }
            // Both end the session, but only after the replies the dialog already queued:
            // a timed-out exchange is answered with its refusal (AR or CE), and a late
            // store or interpretation outcome for it is refused as stale (ADR 0003).
            Action::Refused(error) => run.failures.keep(error.into()),
            Action::TimedOut(_) => run.failures.keep(Error::HostPhaseTimeout),
            Action::Sent(_, _) | Action::CommitAccepted(_) => {
                return Err(Error::UnexpectedOutgoingHl7Outcome);
            }
        }
        Ok(Flow::Continue)
    }
}

impl Host<'_, '_> {
    async fn answer_query(
        &mut self,
        request: QueryRequest,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
    ) -> Result<Flow, Error> {
        let QueryRequest {
            token,
            message,
            started,
            edition_matches,
        } = request;

        let id = self
            .pending
            .remove(&token)
            .ok_or(Error::QueryReceiptAbsent)?;
        if !edition_matches {
            intake::failed_reading(
                store,
                machine.machine(),
                id,
                "HL7 query edition differs from configured edition",
            )
            .await?;
        } else {
            let query = self.queries.as_mut().ok_or(Error::QueryServicesAbsent)?;
            let request = orders::Request {
                message: &message,
                started: self
                    .start
                    .checked_add(started)
                    .ok_or(Error::QueryClockOverflow)?,
                receipt: id,
            };
            let reply = tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(Flow::Stop),
                result = query.answer(store, machine.machine(), request, &self.dialog, self.start) => result?,
            };
            if let Some(reply) = reply {
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::QueryReply(&reply))?,
                );
            }
        }
        // The reply is prepared; record the query as handled before any reply byte.
        intake::mark_handled(store, machine.machine(), id).await?;
        actions.extend(self.dialog.step(self.start.elapsed(), Event::Tick)?);

        Ok(Flow::Continue)
    }
    async fn commit(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        raw: Vec<u8>,
        run: &mut Run<'_, Action, Error>,
    ) -> Result<(), Error> {
        let machine = self.machine;
        let store = self.store;
        let id = ReceiptId::new();
        match custody::store_before_ack(machine.machine(), intake::receive(store, machine, id, raw))
            .await
        {
            Ok(_) => {
                if self.pending.len() >= MAX_PENDING_RECEIPTS {
                    return Err(Error::PendingReceiptLimitExceeded);
                }
                self.pending.insert(token, id);
                run.actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::Stored(token))?,
                );
            }
            Err(error) => {
                run.failures.keep(error.into());
                run.actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::StoreFailed(token))?,
                );
            }
        }

        Ok(())
    }
    async fn interpret(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let id = self
            .pending
            .remove(&token)
            .ok_or_else(|| Error::ReceiptCorrelationMissing)?;
        let (outcome, location) =
            intake::interpret_with_location(store, machine.machine(), id).await?;
        let event = if matches!(outcome, intake::InterpretationOutcome::Accepted) {
            Event::Interpreted(token)
        } else {
            Event::NotInterpretable(
                token,
                analyte_hl7::location_error(
                    location.as_ref(),
                    analyte_hl7::APPLICATION_INTERNAL_ERROR,
                    b"operator-review",
                ),
                analyte_dialog::hl7::AckChoice::ContentError,
            )
        };
        actions.extend(self.dialog.step(self.start.elapsed(), event)?);

        Ok(())
    }
    async fn display_outcome(
        &mut self,
        outcome: DisplayOutcome,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
    ) -> Result<(), Error> {
        let DisplayOutcome {
            receipt,
            accepted,
            code,
            detail,
        } = outcome;

        if let Some(token) = receipt {
            let id = self
                .pending
                .remove(&token)
                .ok_or(Error::DisplayReceiptAbsent)?;
            if let Some(detail) = detail {
                intake::failed_reading(store, machine.machine(), id, &detail).await?;
            }
            intake::mark_handled(store, machine.machine(), id).await?;
        }
        self.queries
            .as_mut()
            .ok_or(Error::DisplayServicesAbsent)?
            .outcome(store, code, accepted)
            .await?;

        Ok(())
    }
    /// Issues dialog bytes by the dialog's absolute deadline, or the transport budget.
    async fn write(
        &self,
        write: &analyte_dialog::Write,
        conn: &mut analyte_transport::Connection,
    ) -> Result<(), Error> {
        custody::write(
            conn,
            &write.bytes,
            WriteDeadline::dialog(self.start, write.until)?,
        )
        .await?;
        Ok(())
    }
    async fn write_query(
        &mut self,
        bytes: &[u8],
        until: Duration,
        part: analyte_dialog::hl7::ReplyPart,
        conn: &mut analyte_transport::Connection,
        actions: &mut VecDeque<Action>,
        failures: &mut Failures<Error>,
    ) -> Result<(), Error> {
        let deadline = WriteDeadline::dialog(self.start, Some(until))?;
        match custody::write(conn, bytes, deadline).await {
            Ok(()) => actions.extend(
                self.dialog
                    .step(self.start.elapsed(), Event::QueryWritten(part))?,
            ),
            Err(error) => {
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::QueryWriteFailed(part))?,
                );
                failures.keep(error.into());
            }
        }

        Ok(())
    }
    /// Sends at most one reserved order through the dialog's `Send` event. Stopping
    /// during the write leaves its outcome to the exchange cleanup; it is never resent.
    async fn send_order(
        &mut self,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let Some(query) = &mut self.queries else {
            return Ok(Flow::Continue);
        };
        let next = tokio::select! {
            biased;
            _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(Flow::Stop),
            result = query.next(self.store, self.machine, &self.dialog) => result?,
        };
        let Some(order) = next else {
            return Ok(Flow::Continue);
        };
        let actions = self.dialog.step(
            self.start.elapsed(),
            Event::Send(order.message, Expect::OrderResponse(order.wait)),
        )?;
        let mut stopped = stop.clone();
        tokio::select! {
            biased;
            _ = async { let _ = stopped.wait_for(|&stopped| stopped).await; } => Ok(Flow::Stop),
            flow = driver::drain(self, actions, conn, stop) => flow,
        }
    }
    async fn order_response(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        message: &analyte_hl7::Message,
        store: &Handle,
    ) -> Result<(), Error> {
        let id = self
            .pending
            .remove(&token)
            .ok_or(Error::OrderResponseReceiptAbsent)?;
        self.queries
            .as_mut()
            .ok_or(Error::OrderServicesAbsent)?
            .order_response(store, Some(message))
            .await?;
        intake::mark_handled(store, self.machine.machine(), id).await?;

        Ok(())
    }
    fn respond(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let metadata = orders::response::response_metadata(orders::response::Profile::Hl7);
        actions.extend(self.dialog.step(
            self.start.elapsed(),
            Event::Respond(token, metadata.response()?),
        )?);

        Ok(())
    }
    async fn refuse_message(
        &self,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        bytes: Vec<u8>,
    ) -> Result<Flow, Error> {
        intake::park_bytes(store, machine, bytes, UNSOLICITED_REASON).await?;
        Err(Error::MessageRefused)
    }
}

impl Effects for Host<'_, '_> {
    type Error = Error;
    fn wait(&self) -> Result<Option<Duration>, Error> {
        Ok(Some(self.wake.map_or(
            crate::orders::sending::ORDER_POLL_INTERVAL,
            |deadline| deadline.saturating_sub(self.start.elapsed()),
        )))
    }
    async fn input(&mut self, host: &mut Context<'_>, input: Input) -> Result<Flow, Error> {
        let machine = self.machine;
        let conn = &mut *host.conn;
        let stop = &mut host.stop;
        let bytes = match input {
            Input::Opened { session } => {
                tracing::info!(
                    machine = machine.machine().id.as_str(),
                    session,
                    "HL7 connection opened"
                );
                Vec::new()
            }
            Input::Bytes { bytes } => bytes,
            Input::Closed(_) => return Ok(Flow::Stop),
            Input::Failed(_) | Input::Tick => Vec::new(),
        };
        let has_bytes = !bytes.is_empty();
        if self.receive_bytes(bytes, conn, stop).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
        self.input_evidence.complete();

        if has_bytes && machine.machine().hooks.enabled(Pause::InputProcessed) {
            machine
                .machine()
                .hooks
                .pause(Pause::InputProcessed, stop)
                .await
                .map_err(Error::from)?;
        }
        if self.send_order(conn, stop).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
        self.wake = self.dialog.next_deadline();
        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let machine = self.machine;
        let store = self.store;
        let cleanup = self.input_evidence.retain(store, machine).await;
        custody::retain_cleanup(result, cleanup.map_err(Into::into));
        // A cancelled or failed host operation must expose partial input as well.
        let actions = match self.dialog.step(self.start.elapsed(), Event::Disconnected) {
            Ok(actions) => actions,
            Err(error) => {
                custody::retain_cleanup(result, Err(error.into()));
                Vec::new()
            }
        };
        for action in actions {
            if let Action::Rejected(error, bytes) = action {
                let cleanup = intake::park_bytes(store, machine, bytes, &error.to_string()).await;
                custody::retain_cleanup(result, cleanup.map_err(Into::into));
            }
        }
        if let Some(query) = &mut self.queries {
            let cleanup = query
                .close(
                    store,
                    machine,
                    if result.is_err() {
                        Hl7End::DsrSessionError.as_str()
                    } else {
                        Hl7End::DsrDisconnect.as_str()
                    },
                )
                .await;
            custody::retain_cleanup(result, cleanup.map_err(Into::into));
        }
        Ok(())
    }
}
