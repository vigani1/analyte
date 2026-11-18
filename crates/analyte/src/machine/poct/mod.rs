//! Drives POCT DML reviewer connections with durable custody before acceptance.
use super::driver::{Flow, HostInput, Protocol, Session};
use crate::intake;
use crate::machine::receive::custody::{self, ms};
mod error;
pub(crate) mod settings;
use analyte_config::PoctStack;
use analyte_dialog::poct::{Action, Dialog, Event, StoreOutcome};
use analyte_model::ReceiptId;
use analyte_poct::{Body, ControlId, Header, Message, Timestamp};
use analyte_store::Handle;
pub(crate) use error::Error;
use std::time::Duration;

// Parking reasons are stored text; `receive::bytes` pins their spellings.
pub(crate) const LIMIT_REASON: &str = "POCT XML message byte limit exceeded";
pub(crate) const CANCELLED_REASON: &str = "POCT partial XML on cancellation";
pub(crate) const DEADLINE_REASON: &str = "POCT partial XML exceeded dialog deadline";
pub(crate) const INCOMPLETE_REASON: &str = "POCT incomplete XML on timeout or disconnect";
pub(crate) const DEFERRED_REASON: &str = "POCT message deferred by bounded custody queue";
pub(crate) fn syntax_reason(error: &analyte_poct::MessageError) -> String {
    format!("POCT syntax: {error}")
}
/// Guards edits of independent POCT readings, which carry no execution context.
pub(crate) fn policy() -> crate::machine::interpretation::policy::Policy {
    crate::machine::interpretation::policy::Policy::independent(
        "edited POCT observation with no detectable common-content change; source context may have changed",
    )
}
pub(crate) fn closure_reason(normal: bool, unresolved: &str) -> String {
    format!(
        "POCT dialog closure: normal={normal}; unresolved custody tickets: {unresolved}; inspect retained transport journal"
    )
}

/// The POCT dialog of one connection.
pub(super) struct Host {
    dialog: Dialog,
}

impl Host {
    pub(super) fn open(settings: &PoctStack) -> Result<Self, Error> {
        let dialog = Dialog::new(
            settings::dialog(settings),
            ControlId::new(&uuid::Uuid::now_v7().simple().to_string())?,
        )?;
        Ok(Self { dialog })
    }
    /// Parks the dialog's unfinished input, and any complete root the stopping
    /// byte finished, instead of storing or acknowledging it.
    async fn cancel(
        &mut self,
        s: &Session<'_, '_, Self>,
        actions: Vec<Action>,
    ) -> Result<(), Error> {
        let mut raw = self.dialog.take_input();
        for action in actions {
            if let Action::Parse(wire) | Action::ParseLimit(wire) | Action::ParseDeadline(wire) =
                action
            {
                raw.extend(wire);
            }
        }
        intake::park_bytes(s.store, s.machine, raw, CANCELLED_REASON).await?;
        Ok(())
    }
}

impl Protocol for Host {
    type Settings = PoctStack;
    type Action = Action;
    type Error = Error;
    fn deadline(&self, _s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Error> {
        Ok(self.dialog.next_deadline())
    }
    /// A closed dialog has no deadline and nothing more to serve.
    async fn idle(&mut self, _s: &mut Session<'_, '_, Self>) -> Result<Flow, Error> {
        Ok(if self.dialog.next_deadline().is_some() {
            Flow::Continue
        } else {
            Flow::Stop
        })
    }
    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Error> {
        if let HostInput::Byte(byte) = input {
            let actions = self.dialog.step(s.elapsed(), Event::Byte(byte))?;
            if *s.stop.borrow() {
                self.cancel(s, actions).await?;
                return Ok(Flow::Stop);
            }
            s.actions.extend(actions);
            return Ok(Flow::Continue);
        }
        // Every other input first applies due time, so incoming fragments never
        // postpone the absolute dialog deadline. An expired deadline closes the
        // dialog, whose Close effect stops the queue before a disconnect's
        // actions, which the closed dialog leaves empty anyway.
        let actions = self.dialog.step(s.elapsed(), Event::Tick)?;
        s.actions.extend(actions);
        if input == HostInput::Disconnected {
            let actions = self.dialog.step(s.elapsed(), Event::Disconnected)?;
            s.actions.extend(actions);
        }
        Ok(Flow::Continue)
    }
    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Action,
    ) -> Result<Flow, Error> {
        let machine = s.machine;
        let store = s.store;
        let now = s.elapsed();
        let limits = settings::limits(s.settings);
        let dialog = &mut self.dialog;
        match action {
            Action::Parse(wire) => {
                let actions = interpret_input(wire, dialog, s, limits).await?;
                s.actions.extend(actions);
            }
            Action::ParseLimit(wire) => {
                intake::park_bytes(store, machine, wire, LIMIT_REASON).await?;
                return Err(Error::MessageLimitExceeded);
            }
            Action::ParseDeadline(wire) => {
                intake::park_bytes(store, machine, wire, DEADLINE_REASON).await?;
            }
            Action::InspectHello {
                device_id: _,
                capabilities: _,
            } => {
                s.actions
                    .extend(dialog.step(now, Event::HelloDecision(true))?);
            }
            Action::Store { ticket, message } => {
                let operation = store_message(store, machine, &message);
                let event = match custody::store_before_ack(machine.machine(), operation).await {
                    Ok(outcome) => Event::Stored { ticket, outcome },
                    Err(error) => {
                        s.failures.keep(Error::from(error));
                        Event::StoreFailed(ticket)
                    }
                };
                s.actions.extend(dialog.step(s.elapsed(), event)?);
            }
            Action::Park { message, reason } => {
                intake::park_bytes(
                    store,
                    machine,
                    message.document().raw().to_vec(),
                    reason.as_str(),
                )
                .await?;
            }
            Action::Write { control_id, body } => {
                let message =
                    outbound_message(control_id, &body, limits, dialog.outbound_message_limit())?;
                let send_timeout_ms = s.settings.settings().timeouts().send_timeout_ms();
                let deadline = custody::WriteDeadline::Within(ms(send_timeout_ms));
                custody::write(s.conn, message.document().raw(), deadline).await?;
            }
            Action::ReadEnabled(_) => {}
            Action::Deferred(message) => {
                intake::park_bytes(
                    store,
                    machine,
                    message.document().raw().to_vec(),
                    DEFERRED_REASON,
                )
                .await?;
                return Err(Error::BackpressureViolated);
            }
            Action::TopicComplete => {
                s.actions.extend(dialog.step(now, Event::Finish)?);
            }
            Action::Close { normal, unresolved } => {
                retain_closure(store, machine, normal, &unresolved).await?;
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        _result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let raw = self.dialog.take_input();
        if !raw.is_empty() {
            intake::park_bytes(s.store, s.machine, raw, INCOMPLETE_REASON).await?;
        }
        Ok(())
    }
}

async fn interpret_input(
    wire: Vec<u8>,
    dialog: &mut Dialog,
    s: &Session<'_, '_, Host>,
    limits: analyte_poct::Limits,
) -> Result<Vec<Action>, Error> {
    match Message::parse(&wire, limits) {
        Ok(message) => Ok(dialog.step(s.elapsed(), Event::Received(Box::new(message)))?),
        Err(error) => {
            let reference = analyte_poct::hello_error_reference(&wire, limits);
            let ack_error = error.acknowledgement_error();
            intake::park_bytes(s.store, s.machine, wire, &syntax_reason(&error)).await?;
            if let Some(reference) = reference {
                Ok(dialog.step(
                    s.elapsed(),
                    Event::InvalidHello {
                        reference,
                        error: ack_error,
                    },
                )?)
            } else {
                Err(error.into())
            }
        }
    }
}

fn outbound_message(
    control_id: ControlId,
    body: &analyte_poct::Outbound,
    mut limits: analyte_poct::Limits,
    maximum: usize,
) -> Result<Message, Error> {
    limits.message_bytes = limits.message_bytes.min(maximum);
    Ok(analyte_poct::encode(
        &Header {
            control_id,
            created: Timestamp::parse(
                &jiff::Timestamp::now()
                    .strftime("%Y-%m-%dT%H:%M:%S+00:00")
                    .to_string(),
            )?,
        },
        body,
        limits,
    )?)
}

async fn store_message(
    store: &Handle,
    machine: &intake::ConnectionEvidence<'_>,
    message: &Message,
) -> Result<StoreOutcome, Error> {
    let id = ReceiptId::new();
    intake::receive(store, machine, id, message.document().raw().to_vec()).await?;
    match message.body() {
        Body::PatientObservations(_) | Body::NonPatientObservations(_) => {
            intake::interpret(store, machine.machine(), id).await?;
            // This fresh receipt has exactly one interpretation. POCT promises
            // readable custody even when mapped findings require operator review.
            let readable = store
                .request::<_, analyte_store::OperatorError>(crate::deadline()?, move |s| {
                    let evidence = s
                        .interpretation(id, 1)?
                        .ok_or(analyte_store::OperatorError::Corruption)?;
                    Ok::<_, analyte_store::OperatorError>(matches!(
                        evidence.outcome,
                        analyte_store::Reading::Results(_)
                    ))
                })
                .await?;
            if !readable {
                return Ok(StoreOutcome::ApplicationError(
                    analyte_poct::ApplicationError::UnsupportedValue,
                ));
            }
        }
        Body::Hello { .. } => {
            // Mark the complete, parsed Hello as connection context. Transport
            // journal fragments are also receipts, but are not interpreted Hellos.
            intake::mark_handled(store, machine.machine(), id).await?;
        }
        Body::Status { .. }
        | Body::EndTopic(_)
        | Body::Terminate(_)
        | Body::Acknowledgement { .. }
        | Body::Escape { .. }
        | Body::KeepAlive
        | Body::Request(_) => {}
    }
    Ok::<_, Error>(StoreOutcome::Stored)
}

/// Retains unresolved commit identities even when the peer closes normally.
async fn retain_closure(
    store: &Handle,
    machine: &intake::ConnectionEvidence<'_>,
    normal: bool,
    unresolved: &[analyte_dialog::poct::CommitId],
) -> Result<(), Error> {
    if !normal || !unresolved.is_empty() {
        let unresolved = analyte_dialog::poct::ticket_evidence(unresolved);
        let reason = closure_reason(normal, &unresolved);
        intake::park_bytes(store, machine, Vec::new(), &reason).await?;
    }
    Ok(())
}
