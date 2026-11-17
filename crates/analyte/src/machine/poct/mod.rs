//! Drives POCT DML reviewer connections with durable custody before acceptance.
pub(crate) mod policy;
use super::connection_input::Input;
use super::driver::{self, Context, Effects, Flow, Run};
use crate::intake;
use crate::intake::ByteReason;
use crate::machine::receive::custody::{self, ms};
mod error;
pub(crate) mod settings;
use analyte_dialog::poct::{Action, Dialog, Event, StoreOutcome};
use analyte_model::ReceiptId;
use analyte_poct::{Body, ControlId, Header, Message, Timestamp};
use analyte_store::Handle;
pub(crate) use error::Error;
use std::time::Instant;
use tokio::sync::watch;

pub(super) struct Host<'a, 'm> {
    machine: &'a intake::ConnectionEvidence<'m>,
    settings: &'a analyte_config::PoctStack,
    store: &'a Handle,
    dialog: Dialog,
    start: Instant,
}

impl<'a, 'm> Host<'a, 'm> {
    pub(super) fn open(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a analyte_config::PoctStack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let dialog = Dialog::new(
            settings::dialog(settings),
            ControlId::new(&uuid::Uuid::now_v7().simple().to_string())?,
        )?;
        let start = Instant::now();
        Ok(Self {
            machine,
            settings,
            store,
            dialog,
            start,
        })
    }
}

impl driver::ByteEffects for Host<'_, '_> {
    async fn apply_byte(
        &mut self,
        byte: u8,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        let actions = self.dialog.step(self.start.elapsed(), Event::Byte(byte))?;
        if *stop.borrow() {
            let mut raw = self.dialog.take_input();
            for action in actions {
                if let Action::Parse(wire)
                | Action::ParseLimit(wire)
                | Action::ParseDeadline(wire) = action
                {
                    raw.extend(wire);
                }
            }
            intake::park_bytes(store, machine, raw, ByteReason::PoctCancelled).await?;
            return Ok(Flow::Stop);
        }
        driver::drain(self, actions, conn, stop).await
    }
}

impl Effects for Host<'_, '_> {
    driver::chunk_effects!(Error);
    async fn prepare(&mut self, _host: &mut Context<'_, '_>) -> Result<Flow, Error> {
        Ok(if self.dialog.next_deadline().is_some() {
            Flow::Continue
        } else {
            Flow::Stop
        })
    }
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        let conn = &mut *host.conn;
        let stop = &mut host.stop;
        // Every input first applies due time, so incoming fragments never
        // postpone the absolute dialog deadline; a tick needs nothing more.
        let actions = self.dialog.step(self.start.elapsed(), Event::Tick)?;
        if driver::drain(self, actions, conn, stop).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
        match input {
            Input::Opened { .. } | Input::Tick => {}
            Input::Closed(_) | Input::Failed(_) => {
                let actions = self
                    .dialog
                    .step(self.start.elapsed(), Event::Disconnected)?;
                driver::drain(self, actions, conn, stop).await?;
                return Ok(Flow::Stop);
            }
            Input::Bytes { bytes } => {
                if driver::feed(self, bytes, conn, stop).await? == Flow::Stop {
                    return Ok(Flow::Stop);
                }
            }
        }

        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_>,
        _result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let raw = self.dialog.take_input();
        if !raw.is_empty() {
            intake::park_bytes(self.store, self.machine, raw, ByteReason::PoctIncomplete).await?;
        }
        Ok(())
    }
}

async fn interpret_input(
    wire: Vec<u8>,
    dialog: &mut Dialog,
    start: Instant,
    machine: &intake::ConnectionEvidence<'_>,
    store: &Handle,
    limits: analyte_poct::Limits,
) -> Result<Vec<Action>, Error> {
    match Message::parse(&wire, limits) {
        Ok(message) => Ok(dialog.step(start.elapsed(), Event::Received(Box::new(message)))?),
        Err(error) => {
            let reference = analyte_poct::hello_error_reference(&wire, limits);
            let ack_error = error.acknowledgement_error();
            intake::park_bytes(store, machine, wire, ByteReason::PoctSyntax(&error)).await?;
            if let Some(reference) = reference {
                Ok(dialog.step(
                    start.elapsed(),
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

impl driver::Drain for Host<'_, '_> {
    type Action = Action;
    async fn effect(
        &mut self,
        action: Action,
        run: &mut Run<'_, Action, Error>,
    ) -> Result<Flow, Error> {
        let machine = self.machine;
        let store = self.store;
        let start = self.start;
        let limits = super::interpretation::poct_limits(self.settings);
        let dialog = &mut self.dialog;
        let Run {
            actions,
            failures,
            conn,
            ..
        } = run;
        match action {
            Action::Parse(wire) => {
                actions.extend(interpret_input(wire, dialog, start, machine, store, limits).await?);
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
                actions.extend(dialog.step(start.elapsed(), Event::HelloDecision(true))?);
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
                actions.extend(dialog.step(start.elapsed(), event)?);
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
                let send_timeout_ms = self.settings.settings().timeouts().send_timeout_ms();
                custody::write(conn, message.document().raw(), ms(send_timeout_ms)).await?;
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
                actions.extend(dialog.step(start.elapsed(), Event::Finish)?);
            }
            Action::Close { normal, unresolved } => {
                retain_closure(store, machine, normal, &unresolved).await?;
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }
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
            intake::remember_context(store, machine.machine(), id).await?;
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
