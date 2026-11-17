//! Drives HL7 connections and chooses ACKs after durable interpretation.
use super::connection_input::{self, Input};
use crate::intake;
use crate::machine::error::SessionError as Error;
use crate::orders::outcome::Code;
use analyte_dialog::hl7::{Action, Config, Dialog, Event};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &intake::Connection<'_>,
    settings: &analyte_config::Hl7Stack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut state = ConnectionState::open(settings)?;
    let result = state.receive_loop(machine, store, conn, &mut stop).await;
    state.close(machine, store, result).await
}

struct DisplayOutcome {
    receipt: Option<analyte_dialog::hl7::ReceiptId>,
    accepted: bool,
    code: &'static str,
    detail: Option<String>,
}

struct QueryRequest {
    token: analyte_dialog::hl7::ReceiptId,
    message: analyte_hl7::Message,
    started: Duration,
    edition_matches: bool,
}

struct ConnectionState {
    dialog: Dialog,
    start: Instant,
    wake: Option<Duration>,
    pending: BTreeMap<(u64, u64), ReceiptId>,
    queries: Option<crate::orders::hl7::Connection>,
    input_evidence: connection_input::Custody,
    write_error: Option<analyte_transport::Error>,
}

impl ConnectionState {
    fn open(settings: &analyte_config::Hl7Stack) -> Result<Self, Error> {
        let mut options = settings.syntax().options();
        options.limits.message_bytes = options.limits.message_bytes.min(settings.frame_limit());
        let config = Config::new(
            options,
            Duration::from_millis(u64::from(settings.timeouts().receive_timeout_ms())),
            1,
        )?;
        let mut dialog = Dialog::new(config, 0);
        let start = Instant::now();
        let wake = None;
        let pending = BTreeMap::new();
        let enabled = settings
            .orders()
            .as_ref()
            .is_some_and(|o| o.query().is_some() || o.send().is_some());
        let queries = if enabled {
            Some(crate::orders::hl7::Connection::new(options)?)
        } else {
            None
        };
        if enabled {
            dialog.enable_orders(settings.syntax().edition().into())?;
        }
        let input_evidence = connection_input::Custody::default();
        let write_error = None;
        Ok(Self {
            dialog,
            start,
            wake,
            pending,
            queries,
            input_evidence,
            write_error,
        })
    }
    async fn receive_loop(
        &mut self,
        machine: &intake::Connection<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        loop {
            let wait = self.wake.map_or(Duration::from_secs(1), |d: Duration| {
                d.saturating_sub(self.start.elapsed())
            });

            let event = tokio::select! {
                biased;
                () = super::stopping(stop) => break,
                () = tokio::time::sleep(wait) => None,
                event = connection_input::receive(conn) => match event {
                    Ok(event) => Some(event),
                    Err(analyte_transport::Error::Deadline(_)) => None,
                    Err(error) => return Err(error.into()),
                },
            };
            let bytes = match event {
                Some(Input::Opened { session }) => {
                    tracing::info!(
                        machine = machine.id.as_str(),
                        session,
                        "HL7 connection opened"
                    );
                    Vec::new()
                }
                Some(Input::Bytes { bytes }) => bytes,
                Some(Input::Closed(_)) => break,
                Some(Input::Failed(_) | Input::RecordingFailed(_)) | Some(Input::Tick) | None => {
                    transport_failure_as_empty_input()
                }
            };
            if self
                .receive_bytes(bytes, machine, store, conn, stop)
                .await?
            {
                return Ok(());
            }
            self.input_evidence.complete();
            if let Some(query) = &mut self.queries {
                tokio::select! {biased; ()=super::stopping(stop)=>break,result=query.idle(store,machine,conn,&mut self.dialog,self.start)=>result?,}
            }
            self.wake = self.dialog.next_deadline();
        }
        Ok::<_, Error>(())
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        machine: &intake::Connection<'_>,
        store: &Handle,
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
            if self.process(event, machine, store, conn, stop).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn process(
        &mut self,
        event: Event<'_>,
        machine: &intake::Connection<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<bool, Error> {
        let mut actions: VecDeque<_> = self.dialog.step(self.start.elapsed(), event)?.into();
        while let Some(action) = actions.pop_front() {
            if self
                .effect(action, machine, store, conn, stop, &mut actions)
                .await?
            {
                return Ok(true);
            }
        }
        if let Some(error) = self.write_error.take() {
            return Err(error.into());
        }
        Ok(false)
    }
    async fn effect(
        &mut self,
        action: Action,
        machine: &intake::Connection<'_>,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
    ) -> Result<bool, Error> {
        match action {
            Action::ReceiptRetained(token) => {
                self.pending
                    .remove(&(token.epoch(), token.ordinal()))
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
                    .ok_or(Error::Hl7OrderServicesAbsent)?
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
            Action::Commit(token, raw) => {
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
            Action::Write(bytes) => {
                conn.write_all(&bytes, None).await?;
            }
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
            Action::Refused(error) => return Err(error.into()),
            Action::TimedOut(_) => {
                return Err(Error::Hl7HostPhaseTimedOutReceiptRequiresReconciliation);
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
        machine: &intake::Connection<'_>,
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
            .remove(&(token.epoch(), token.ordinal()))
            .ok_or(Error::QueryReceiptAbsent)?;
        if !edition_matches {
            intake::failed_reading(
                store,
                machine,
                id,
                "HL7 query edition differs from configured edition",
            )
            .await?;
        } else {
            let query = self.queries.as_mut().ok_or(Error::Hl7QueryServicesAbsent)?;
            let reply = tokio::select! { biased; () = super::stopping(stop) => return Ok(true), result = query.answer(store,machine,crate::orders::hl7::Request { message: &message, started: self.start.checked_add(started).ok_or(Error::QueryClockOverflow)?, receipt: id },&self.dialog,self.start) => result? };
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
        token: analyte_dialog::hl7::ReceiptId,
        raw: Vec<u8>,
        machine: &intake::Connection<'_>,
        store: &Handle,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let id = ReceiptId::new();
        match super::custody::store_within(
            super::custody::budget(machine),
            intake::receive(store, machine, id, raw),
        )
        .await
        {
            Ok(_) => {
                if self.pending.len() >= 2 {
                    return Err(Error::Hl7PendingReceiptLimitExceeded);
                }
                self.pending.insert((token.epoch(), token.ordinal()), id);
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::Committed(token))?,
                );
            }
            Err(error) => {
                tracing::error!(
                    machine = machine.id.as_str(),
                    error_kind = %error,
                    "HL7 durable commit refused"
                );
                actions.extend(
                    self.dialog
                        .step(self.start.elapsed(), Event::CommitFailed(token))?,
                );
            }
        }

        Ok(())
    }
    async fn interpret(
        &mut self,
        token: analyte_dialog::hl7::ReceiptId,
        machine: &intake::Connection<'_>,
        store: &Handle,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let id = self
            .pending
            .remove(&(token.epoch(), token.ordinal()))
            .ok_or_else(|| Error::Hl7ReceiptCorrelationMissing)?;
        let (outcome, location) = intake::interpret_with_location(store, machine, id).await?;
        let event = if matches!(outcome, intake::InterpretationOutcome::Accepted) {
            Event::Interpreted(token)
        } else {
            Event::NotInterpretable(
                token,
                super::error_location::build(location.as_ref(), b"207", b"operator-review"),
                false,
            )
        };
        actions.extend(self.dialog.step(self.start.elapsed(), event)?);

        Ok(())
    }
    async fn display_outcome(
        &mut self,
        outcome: DisplayOutcome,
        machine: &intake::Connection<'_>,
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
                .remove(&(token.epoch(), token.ordinal()))
                .ok_or(Error::DisplayReceiptAbsent)?;
            if let Some(detail) = detail {
                intake::failed_reading(store, machine, id, &detail).await?;
            }
        }
        self.queries
            .as_mut()
            .ok_or(Error::Hl7DisplayServicesAbsent)?
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
        token: analyte_dialog::hl7::ReceiptId,
        message: &analyte_hl7::Message,
        store: &Handle,
    ) -> Result<(), Error> {
        self.pending
            .remove(&(token.epoch(), token.ordinal()))
            .ok_or(Error::OrderResponseReceiptAbsent)?;
        self.queries
            .as_mut()
            .ok_or(Error::Hl7OrderServicesAbsent)?
            .order_response(store, Some(message))
            .await?;

        Ok(())
    }
    fn respond(
        &mut self,
        token: analyte_dialog::hl7::ReceiptId,
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
        machine: &intake::Connection<'_>,
        store: &Handle,
        bytes: Vec<u8>,
    ) -> Result<bool, Error> {
        intake::park_bytes(
            store,
            machine,
            bytes,
            "HL7 unsolicited message or queue overflow",
        )
        .await?;
        Err(Error::Hl7MessageRefused)
    }
    async fn close(
        &mut self,
        machine: &intake::Connection<'_>,
        store: &Handle,
        mut result: Result<(), Error>,
    ) -> Result<(), Error> {
        let cleanup = self.input_evidence.retain(store, machine).await;
        super::custody::retain_cleanup(&mut result, cleanup);
        // A cancelled or failed host operation must expose partial input as well.
        let actions = match self.dialog.step(self.start.elapsed(), Event::Disconnect) {
            Ok(actions) => actions,
            Err(error) => {
                super::custody::retain_cleanup(&mut result, Err(error.into()));
                Vec::new()
            }
        };
        for action in actions {
            if let Action::Rejected(error, bytes) = action {
                let cleanup = intake::park_bytes(store, machine, bytes, &error.to_string()).await;
                super::custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
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
            super::custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
        }
        result
    }
}

// Compatibility: a failed transport attempt advances the dialog with empty input.
// It does not imply EOF or durable acceptance of a message.
fn transport_failure_as_empty_input() -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests;
