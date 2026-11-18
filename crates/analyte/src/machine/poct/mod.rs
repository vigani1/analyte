//! Drives POCT DML reviewer connections with durable custody before acceptance.
pub(crate) mod policy;
use super::driver::{Flow, HostInput, Protocol, Queue, Session};
use crate::intake;
use crate::intake::ByteReason;
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
        s: &Session<'_, '_, PoctStack>,
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
        intake::park_bytes(s.store, s.machine, raw, ByteReason::PoctCancelled).await?;
        Ok(())
    }
}

impl Protocol for Host {
    type Settings = PoctStack;
    type Action = Action;
    type Error = Error;
    fn deadline(&self, _s: &Session<'_, '_, PoctStack>) -> Result<Option<Duration>, Error> {
        Ok(self.dialog.next_deadline())
    }
    /// A closed dialog has no deadline and nothing more to serve.
    async fn idle(
        &mut self,
        _s: &mut Session<'_, '_, PoctStack>,
        _queue: &mut Queue<Action, Error>,
    ) -> Result<Flow, Error> {
        Ok(if self.dialog.next_deadline().is_some() {
            Flow::Continue
        } else {
            Flow::Stop
        })
    }
    async fn input(
        &mut self,
        s: &mut Session<'_, '_, PoctStack>,
        input: HostInput,
        queue: &mut Queue<Action, Error>,
    ) -> Result<Flow, Error> {
        if let HostInput::Byte(byte) = input {
            let actions = self.dialog.step(s.elapsed(), Event::Byte(byte))?;
            if *s.stop.borrow() {
                self.cancel(s, actions).await?;
                return Ok(Flow::Stop);
            }
            queue.actions.extend(actions);
            return Ok(Flow::Continue);
        }
        // Every other input first applies due time, so incoming fragments never
        // postpone the absolute dialog deadline. An expired deadline closes the
        // dialog, whose Close effect stops the queue before a disconnect's
        // actions, which the closed dialog leaves empty anyway.
        let actions = self.dialog.step(s.elapsed(), Event::Tick)?;
        queue.actions.extend(actions);
        if input == HostInput::Disconnected {
            let actions = self.dialog.step(s.elapsed(), Event::Disconnected)?;
            queue.actions.extend(actions);
        }
        Ok(Flow::Continue)
    }
    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, PoctStack>,
        action: Action,
        queue: &mut Queue<Action, Error>,
    ) -> Result<Flow, Error> {
        let machine = s.machine;
        let store = s.store;
        let now = s.elapsed();
        let limits = super::interpretation::poct_limits(s.settings);
        let dialog = &mut self.dialog;
        let Queue {
            actions, failures, ..
        } = queue;
        match action {
            Action::Parse(wire) => {
                actions.extend(interpret_input(wire, dialog, s, limits).await?);
            }
            Action::ParseLimit(wire) => {
                intake::park_bytes(store, machine, wire, ByteReason::PoctLimit).await?;
                return Err(Error::MessageLimitExceeded);
            }
            Action::ParseDeadline(wire) => {
                intake::park_bytes(store, machine, wire, ByteReason::PoctDeadline).await?;
            }
            Action::InspectHello {
                device_id: _,
                capabilities: _,
            } => {
                actions.extend(dialog.step(now, Event::HelloDecision(true))?);
            }
            Action::Store { ticket, message } => {
                let operation = store_message(store, machine, &message);
                let event = match custody::store_before_ack(machine.machine(), operation).await {
                    Ok(outcome) => Event::Stored { ticket, outcome },
                    Err(error) => {
                        failures.keep(Error::from(error));
                        Event::StoreFailed(ticket)
                    }
                };
                actions.extend(dialog.step(s.elapsed(), event)?);
            }
            Action::Park { message, reason } => {
                intake::park_bytes(
                    store,
                    machine,
                    message.document().raw().to_vec(),
                    ByteReason::PoctDialog(reason),
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
                    ByteReason::PoctDeferred,
                )
                .await?;
                return Err(Error::BackpressureViolated);
            }
            Action::TopicComplete => {
                actions.extend(dialog.step(now, Event::Finish)?);
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
        s: &mut Session<'_, '_, PoctStack>,
        _result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let raw = self.dialog.take_input();
        if !raw.is_empty() {
            intake::park_bytes(s.store, s.machine, raw, ByteReason::PoctIncomplete).await?;
        }
        Ok(())
    }
}

async fn interpret_input(
    wire: Vec<u8>,
    dialog: &mut Dialog,
    s: &Session<'_, '_, PoctStack>,
    limits: analyte_poct::Limits,
) -> Result<Vec<Action>, Error> {
    match Message::parse(&wire, limits) {
        Ok(message) => Ok(dialog.step(s.elapsed(), Event::Received(Box::new(message)))?),
        Err(error) => {
            let reference = analyte_poct::hello_error_reference(&wire, limits);
            let ack_error = error.acknowledgement_error();
            let reason = ByteReason::PoctSyntax(&error);
            intake::park_bytes(s.store, s.machine, wire, reason).await?;
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
        intake::park_bytes(
            store,
            machine,
            Vec::new(),
            ByteReason::PoctClosure {
                normal,
                unresolved: &unresolved,
            },
        )
        .await?;
    }
    Ok(())
}
