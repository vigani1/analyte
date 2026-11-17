//! Drives HL7 connections and chooses ACKs after durable interpretation.
use crate::intake::ByteReason;
use crate::machine::hooks::Pause;
pub(crate) mod error_location;
use super::connection_input::{self, Input};
use super::driver::{Context, Effects, Flow};
use crate::intake;
mod error;
pub(crate) mod settings;
use crate::orders::outcome::Code;
use analyte_dialog::hl7::{Action, Dialog, Event};
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
    settings: &'a analyte_config::Hl7Stack,
    store: &'a Handle,
    dialog: Dialog,
    start: Instant,
    wake: Option<Duration>,
    pending: BTreeMap<analyte_dialog::ExchangeId, ReceiptId>,
    queries: Option<crate::orders::hl7::Hl7OrderLink>,
    input_evidence: connection_input::Custody,
    write_error: Option<analyte_transport::Error>,
    commit_error: Option<Error>,
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
        let enabled = settings
            .orders()
            .as_ref()
            .is_some_and(|o| o.query().is_some() || o.send().is_some());
        let queries = if enabled {
            Some(crate::orders::hl7::Hl7OrderLink::new(options)?)
        } else {
            None
        };
        if enabled {
            dialog.enable_orders(settings.settings().syntax().edition().into())?;
        }
        let input_evidence = connection_input::Custody::default();
        let write_error = None;
        Ok(Self {
            machine,
            settings,
            store,
            dialog,
            start,
            wake,
            pending,
            queries,
            input_evidence,
            write_error,
            commit_error: None,
        })
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<bool, Error> {
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
            if self.transition(event, conn, stop).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn transition(
        &mut self,
        event: Event<'_>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<bool, Error> {
        let actions = self.dialog.step(self.start.elapsed(), event)?;
        self.apply(actions, conn, stop).await
    }
    async fn apply(
        &mut self,
        actions: Vec<Action>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<bool, Error> {
        let mut actions: VecDeque<_> = actions.into();
        let result = async {
            while let Some(action) = actions.pop_front() {
                if self.effect(action, conn, stop, &mut actions).await? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        .await;
        if let Some(error) = self.commit_error.take() {
            return Err(match result {
                Ok(_) => error,
                Err(effect) => {
                    crate::machine::receive::custody::effect_failure(Some(error), effect)
                }
            });
        }
        if result? {
            return Ok(true);
        }
        if let Some(error) = self.write_error.take() {
            return Err(error.into());
        }
        Ok(false)
    }
    async fn effect(
        &mut self,
        action: Action,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
    ) -> Result<bool, Error> {
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
                        stop,
                        actions,
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
                self.commit(token, raw, machine, store, actions).await?;
            }
            Action::Interpret(token, _payload) => {
                self.interpret(token, machine, store, actions).await?;
            }
            Action::ResponseNeeded(token, _) => {
                self.respond(token, actions)?;
            }
            Action::QueryWrite { bytes, until, part } => {
                self.write_query(&bytes, until, part, conn, actions).await?;
            }
            Action::Write(write) => {
                conn.write_all(&write.bytes, None).await?;
            }
            Action::Deadline(next) => self.wake = next,
            Action::Rejected(error, bytes) => {
                intake::park_bytes(store, machine, bytes, ByteReason::Mllp(&error)).await?;
            }
            Action::Parked(bytes, error) => {
                self.pending.clear();
                intake::park_bytes(store, machine, bytes, ByteReason::Hl7Dialog(&error)).await?;
            }
            Action::QueueFull(bytes) | Action::Unmatched(bytes) => {
                return self.refuse_message(machine, store, bytes).await;
            }
            Action::Refused(error) => return Err(error.into()),
            Action::TimedOut(_) => {
                return Err(Error::HostPhaseTimeout);
            }
            Action::Sent(_, _) | Action::CommitAccepted(_) => {
                return Err(Error::UnexpectedOutgoingHl7Outcome);
            }
        }
        Ok(false)
    }
    async fn answer_query(
        &mut self,
        request: QueryRequest,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
    ) -> Result<bool, Error> {
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
            let request = crate::orders::hl7::Request {
                message: &message,
                started: self
                    .start
                    .checked_add(started)
                    .ok_or(Error::QueryClockOverflow)?,
                receipt: id,
            };
            let reply = tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(true),
                result = query.answer(store, machine.machine(), request, &self.dialog, self.start) => result?,
            };
            if let Some(reply) = reply {
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::QueryReply(&reply))?,
                );
            }
        }
        actions.extend(self.dialog.step(self.start.elapsed(), Event::Tick)?);

        Ok(false)
    }
    async fn commit(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        raw: Vec<u8>,
        machine: &intake::ConnectionEvidence<'_>,
        store: &Handle,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let id = ReceiptId::new();
        match crate::machine::receive::custody::store_before_ack(
            machine.machine(),
            intake::receive(store, machine, id, raw),
        )
        .await
        {
            Ok(_) => {
                if self.pending.len() >= MAX_PENDING_RECEIPTS {
                    return Err(Error::PendingReceiptLimitExceeded);
                }
                self.pending.insert(token, id);
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::Stored(token))?,
                );
            }
            Err(error) => {
                self.commit_error = Some(error.into());
                actions.extend(
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
        }
        self.queries
            .as_mut()
            .ok_or(Error::DisplayServicesAbsent)?
            .outcome(store, code, accepted)
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
    ) -> Result<(), Error> {
        match conn
            .write_all(
                bytes,
                Some(tokio::time::Instant::from_std(
                    self.start
                        .checked_add(until)
                        .ok_or(Error::QueryWriteDeadlineOverflow)?,
                )),
            )
            .await
        {
            Ok(()) => actions.extend(
                self.dialog
                    .step(self.start.elapsed(), Event::QueryWritten(part))?,
            ),
            Err(error) => {
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::QueryWriteFailed(part))?,
                );
                self.write_error = Some(error);
            }
        }

        Ok(())
    }
    async fn order_response(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        message: &analyte_hl7::Message,
        store: &Handle,
    ) -> Result<(), Error> {
        self.pending
            .remove(&token)
            .ok_or(Error::OrderResponseReceiptAbsent)?;
        self.queries
            .as_mut()
            .ok_or(Error::OrderServicesAbsent)?
            .order_response(store, Some(message))
            .await?;

        Ok(())
    }
    fn respond(
        &mut self,
        token: analyte_dialog::hl7::ExchangeId,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let metadata = crate::orders::hl7::response::response_metadata(
            crate::orders::hl7::response::Profile::Hl7,
        );
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
    ) -> Result<bool, Error> {
        intake::park_bytes(store, machine, bytes, ByteReason::Hl7Unsolicited).await?;
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
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
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
        if self.receive_bytes(bytes, conn, stop).await? {
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
        if let Some(query) = &mut self.queries {
            tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(Flow::Stop),
                result = query.idle(store, machine, conn, &mut self.dialog, self.start, self.settings) => result?,
            }
        }
        self.wake = self.dialog.next_deadline();
        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let machine = self.machine;
        let store = self.store;
        let cleanup = self.input_evidence.retain(store, machine).await;
        crate::machine::receive::custody::retain_cleanup(result, cleanup.map_err(Into::into));
        // A cancelled or failed host operation must expose partial input as well.
        let actions = match self.dialog.step(self.start.elapsed(), Event::Disconnected) {
            Ok(actions) => actions,
            Err(error) => {
                crate::machine::receive::custody::retain_cleanup(result, Err(error.into()));
                Vec::new()
            }
        };
        for action in actions {
            if let Action::Rejected(error, bytes) = action {
                let cleanup =
                    intake::park_bytes(store, machine, bytes, ByteReason::Mllp(&error)).await;
                crate::machine::receive::custody::retain_cleanup(
                    result,
                    cleanup.map_err(Into::into),
                );
            }
        }
        if let Some(query) = &mut self.queries {
            let cleanup = query
                .close(
                    store,
                    machine,
                    if result.is_err() {
                        Code::Hl7DsrSessionError.as_str()
                    } else {
                        Code::Hl7DsrDisconnect.as_str()
                    },
                )
                .await;
            crate::machine::receive::custody::retain_cleanup(result, cleanup.map_err(Into::into));
        }
        Ok(())
    }
}
