//! Drives POCT DML reviewer connections with durable custody before acceptance.
use super::connection_input::Input;
use super::custody::{self, ms};
use super::driver::{self, Context, Effects, Flow};
use crate::intake;
use crate::intake::ByteReason;
mod error;
use analyte_dialog::poct::{Action, CommitOutcome, Config, Dialog, Event};
use analyte_model::ReceiptId;
use analyte_poct::{Body, ControlId, Header, Message, Timestamp};
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{collections::VecDeque, time::Instant};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &intake::ConnectionEvidence<'_>,
    settings: &analyte_config::PoctStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    driver::run(
        || ConnectionState::open(settings),
        Context {
            machine,
            settings,
            store,
            conn,
            stop,
        },
    )
    .await
}

struct ConnectionState {
    dialog: Dialog,
    start: Instant,
}

impl ConnectionState {
    fn open(settings: &analyte_config::PoctStack) -> Result<Self, Error> {
        let policy = &settings.policy();
        let limits = super::interpretation::poct_limits(settings);
        let dialog = Dialog::new(
            Config {
                depth: policy.depth,
                pending_messages: policy.pending_messages,
                message_bytes: limits.message_bytes,
                response_timeout: ms(policy.response_ms),
                unannounced_ack_timeout: ms(policy.acknowledgment_ms),
            },
            ControlId::new(&uuid::Uuid::now_v7().simple().to_string())?,
        )?;
        let start = Instant::now();
        Ok(Self { dialog, start })
    }

    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        machine: &intake::ConnectionEvidence<'_>,
        settings: &analyte_config::PoctStack,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &watch::Receiver<bool>,
    ) -> Result<bool, Error> {
        for byte in bytes {
            let actions = self.dialog.step(self.start.elapsed(), Event::Byte(byte))?;
            if *stop.borrow() {
                let mut raw = self.dialog.take_input();
                for action in actions {
                    if let Action::Input(wire)
                    | Action::InputLimit(wire)
                    | Action::InputDeadline(wire) = action
                    {
                        raw.extend(wire);
                    }
                }
                intake::park_bytes(store, machine, raw, ByteReason::PoctCancelled).await?;
                return Ok(true);
            }
            if process(
                actions,
                &mut self.dialog,
                self.start,
                machine,
                store,
                conn,
                settings,
            )
            .await?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Effects for ConnectionState {
    type Settings = analyte_config::PoctStack;
    type Error = Error;
    fn custody(&self) -> driver::Custody {
        driver::Custody::Chunks
    }
    fn wait(&self) -> Result<Option<std::time::Duration>, Error> {
        Ok(self
            .dialog
            .next_deadline()
            .map(|deadline| deadline.saturating_sub(self.start.elapsed())))
    }
    async fn prepare(
        &mut self,
        _host: &mut Context<'_, '_, Self::Settings>,
    ) -> Result<Flow, Error> {
        Ok(if self.dialog.next_deadline().is_some() {
            Flow::Continue
        } else {
            Flow::Stop
        })
    }
    async fn input(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        input: Input,
    ) -> Result<Flow, Error> {
        let Context {
            machine,
            store,
            conn,
            stop,
            settings,
        } = host;
        // Incoming fragments must not postpone the absolute dialog deadline.
        let actions = self.dialog.step(self.start.elapsed(), Event::Tick)?;
        if process(
            actions,
            &mut self.dialog,
            self.start,
            machine,
            store,
            conn,
            settings,
        )
        .await?
        {
            return Ok(Flow::Stop);
        }
        match input {
            Input::Opened { .. } => {}
            Input::Tick => {
                let actions = self.dialog.step(self.start.elapsed(), Event::Tick)?;
                if process(
                    actions,
                    &mut self.dialog,
                    self.start,
                    machine,
                    store,
                    conn,
                    settings,
                )
                .await?
                {
                    return Ok(Flow::Stop);
                }
            }
            Input::Closed(_) | Input::Failed(_) | Input::RecordingFailed(_) => {
                let actions = self
                    .dialog
                    .step(self.start.elapsed(), Event::Disconnected)?;
                process(
                    actions,
                    &mut self.dialog,
                    self.start,
                    machine,
                    store,
                    conn,
                    settings,
                )
                .await?;
                return Ok(Flow::Stop);
            }
            Input::Bytes { bytes } => {
                if self
                    .receive_bytes(bytes, machine, settings, store, conn, stop)
                    .await?
                {
                    return Ok(Flow::Stop);
                }
            }
        }

        Ok(Flow::Continue)
    }
    async fn close(
        &mut self,
        host: &mut Context<'_, '_, Self::Settings>,
        _result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        let raw = self.dialog.take_input();
        if !raw.is_empty() {
            intake::park_bytes(host.store, host.machine, raw, ByteReason::PoctIncomplete).await?;
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

async fn process(
    actions: Vec<Action>,
    dialog: &mut Dialog,
    start: Instant,
    machine: &intake::ConnectionEvidence<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    settings: &analyte_config::PoctStack,
) -> Result<bool, Error> {
    let limits = super::interpretation::poct_limits(settings);
    let send_timeout_ms = settings.settings().timeouts().send_timeout_ms();
    let mut actions: VecDeque<_> = actions.into();
    let mut failure = None;
    let result = async {
        while let Some(action) = actions.pop_front() {
            match action {
                Action::Input(wire) => actions
                    .extend(interpret_input(wire, dialog, start, machine, store, limits).await?),
                Action::InputLimit(wire) => {
                    intake::park_bytes(store, machine, wire, ByteReason::PoctLimit).await?;
                    return Err(Error::MessageLimitExceeded);
                }
                Action::InputDeadline(wire) => {
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
                    let outcome =
                        match custody::store_before_ack(machine.machine(), operation).await {
                            Ok(outcome) => outcome,
                            Err(error) => {
                                failure = Some(Error::from(error));
                                CommitOutcome::Failed
                            }
                        };
                    actions.extend(
                        dialog.step(start.elapsed(), Event::Committed { ticket, outcome })?,
                    );
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
                Action::Send { control_id, body } => {
                    let message = outbound_message(
                        control_id,
                        &body,
                        limits,
                        dialog.outbound_message_limit(),
                    )?;
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
                    if !normal || !unresolved.is_empty() {
                        let unresolved = analyte_dialog::poct::ticket_evidence(&unresolved);
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
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    .await;
    match (failure, result) {
        (Some(error), Ok(_)) => Err(error),
        (Some(error), Err(effect)) => Err(custody::effect_failure(Some(error), effect)),
        (None, result) => result,
    }
}

async fn store_message(
    store: &Handle,
    machine: &intake::ConnectionEvidence<'_>,
    message: &Message,
) -> Result<CommitOutcome, Error> {
    let id = ReceiptId::new();
    intake::receive(store, machine, id, message.document().raw().to_vec()).await?;
    match message.body() {
        Body::PatientObservations(_) | Body::NonPatientObservations(_) => {
            intake::interpret(store, machine.machine(), id).await?;
            // This fresh receipt has exactly one interpretation. POCT promises
            // readable custody even when mapped findings require operator review.
            let readable = store
                .request::<_, analyte_store::Error>(crate::deadline()?, move |s| {
                    let evidence = s
                        .interpretation(id, 1)?
                        .ok_or(analyte_store::Error::Corruption)?;
                    Ok::<_, analyte_store::Error>(matches!(
                        evidence.outcome,
                        analyte_store::Reading::Results(_)
                    ))
                })
                .await?;
            if !readable {
                return Ok(CommitOutcome::ApplicationError(
                    analyte_poct::ApplicationError::UnsupportedValue,
                ));
            }
        }
        Body::Hello { .. } => {
            // Mark the complete, parsed Hello as connection context. Transport
            // journal fragments are also receipts, but are not interpreted Hellos.
            intake::remember_hello(store, machine.machine(), id).await?;
        }
        Body::Status { .. }
        | Body::EndTopic(_)
        | Body::Terminate(_)
        | Body::Acknowledgement { .. }
        | Body::Escape { .. }
        | Body::KeepAlive
        | Body::Request(_) => {}
    }
    Ok::<_, Error>(CommitOutcome::Stored)
}
