//! Drives POCT DML reviewer connections with durable custody before acceptance.
use super::connection_input::Input;
use super::custody::{self, ms};
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
    machine: &intake::Connection<'_>,
    settings: &analyte_config::PoctStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut state = ConnectionState::open(settings)?;
    match state
        .receive_loop(machine, settings, store, conn, &mut stop)
        .await?
    {
        Close::Drain => state.close(machine, store).await,
        Close::Complete => Ok(()),
    }
}

enum Close {
    Drain,
    Complete,
}
enum Parsed {
    Message(Box<Message>),
    Skip,
    Close,
}

struct ConnectionState {
    dialog: Dialog,
    start: Instant,
    raw: Vec<u8>,
    framer: analyte_framing::poct::Decoder,
    limits: analyte_poct::Limits,
}

impl ConnectionState {
    fn open(settings: &analyte_config::PoctStack) -> Result<Self, Error> {
        let policy = &settings.policy();
        let limits = super::interpretation::poct_limits(settings);
        let dialog = Dialog::new(
            Config {
                pending_messages: policy.pending_messages,
                message_bytes: limits.message_bytes,
                response_timeout: ms(policy.response_ms),
                unannounced_ack_timeout: ms(policy.acknowledgment_ms),
            },
            ControlId::new(&uuid::Uuid::now_v7().simple().to_string())?,
        )?;
        let start = Instant::now();
        let raw = Vec::new();
        let framer = analyte_framing::poct::Decoder::new(limits.message_bytes, policy.depth);
        Ok(Self {
            dialog,
            start,
            raw,
            framer,
            limits,
        })
    }

    async fn receive_loop(
        &mut self,
        machine: &intake::Connection<'_>,
        settings: &analyte_config::PoctStack,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Close, Error> {
        loop {
            let Some(deadline) = self.dialog.next_deadline() else {
                return Ok(Close::Drain);
            };
            let input = super::connection_input::journaled(
                machine,
                store,
                conn,
                stop,
                Some(deadline.saturating_sub(self.start.elapsed())),
            )
            .await?;
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
                return Ok(Close::Drain);
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
                        return Ok(Close::Drain);
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
                    return Ok(Close::Drain);
                }
                Input::Bytes { bytes } => {
                    if self
                        .receive_bytes(bytes, machine, settings, store, conn, stop)
                        .await?
                    {
                        return Ok(Close::Complete);
                    }
                }
            }
        }
    }

    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        machine: &intake::Connection<'_>,
        settings: &analyte_config::PoctStack,
        store: &Handle,
        conn: &mut analyte_transport::Connection,
        stop: &watch::Receiver<bool>,
    ) -> Result<bool, Error> {
        let start = self.start;
        let limits = self.limits;
        let Self {
            dialog,
            raw,
            framer,
            ..
        } = self;
        for byte in bytes {
            if raw.len() >= limits.message_bytes {
                intake::park_bytes(store, machine, std::mem::take(raw), ByteReason::PoctLimit)
                    .await?;
                return Err(Error::MessageLimitExceeded);
            }
            raw.push(byte);
            if *stop.borrow() {
                intake::park_bytes(
                    store,
                    machine,
                    std::mem::take(raw),
                    ByteReason::PoctCancelled,
                )
                .await?;
                return Ok(true);
            }
            if byte != b'>' {
                continue;
            }
            let actions = dialog.step(start.elapsed(), Event::Tick)?;
            if process(actions, dialog, start, machine, store, conn, settings).await? {
                intake::park_bytes(
                    store,
                    machine,
                    std::mem::take(raw),
                    ByteReason::PoctDeadline,
                )
                .await?;
                return Ok(true);
            }
            let length = match framer.scan(raw) {
                Ok(Some(length)) => length,
                Ok(None) => continue,
                // Parse the retained bytes through the syntax boundary to
                // classify a framing failure and recover a Hello header.
                Err(_) => raw.len(),
            };
            let wire = raw
                .get(..length)
                .ok_or_else(|| Error::FramingLength)?
                .to_vec();
            raw.drain(..length);
            framer.reset();
            let message =
                match parse_message(wire, dialog, start, machine, settings, store, conn).await? {
                    Parsed::Message(message) => message,
                    Parsed::Skip => continue,
                    Parsed::Close => return Ok(true),
                };
            let actions = dialog.step(start.elapsed(), Event::Received(message))?;
            if process(actions, dialog, start, machine, store, conn, settings).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn close(self, machine: &intake::Connection<'_>, store: &Handle) -> Result<(), Error> {
        let raw = self.raw;
        if !raw.is_empty() {
            intake::park_bytes(store, machine, raw, ByteReason::PoctIncomplete).await?;
        }
        Ok(())
    }
}

async fn parse_message(
    wire: Vec<u8>,
    dialog: &mut Dialog,
    start: Instant,
    machine: &intake::Connection<'_>,
    settings: &analyte_config::PoctStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
) -> Result<Parsed, Error> {
    let limits = super::interpretation::poct_limits(settings);
    match Message::parse(&wire, limits) {
        Ok(message) => Ok(Parsed::Message(Box::new(message))),
        Err(error) => {
            let reference = analyte_poct::hello_error_reference(&wire, limits);
            let ack_error = error.acknowledgement_error();
            intake::park_bytes(store, machine, wire, ByteReason::PoctSyntax(&error)).await?;
            if let Some(reference) = reference {
                let actions = dialog.step(
                    start.elapsed(),
                    Event::InvalidHello {
                        reference,
                        error: ack_error,
                    },
                )?;
                if process(actions, dialog, start, machine, store, conn, settings).await? {
                    return Ok(Parsed::Close);
                }
                return Ok(Parsed::Skip);
            }
            Err(error.into())
        }
    }
}

async fn process(
    actions: Vec<Action>,
    dialog: &mut Dialog,
    start: Instant,
    machine: &intake::Connection<'_>,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    settings: &analyte_config::PoctStack,
) -> Result<bool, Error> {
    let limits = super::interpretation::poct_limits(settings);
    let send_timeout_ms = settings.timeouts().send_timeout_ms();
    let mut actions: VecDeque<_> = actions.into();
    let mut failure = None;
    let result = async {
        while let Some(action) = actions.pop_front() {
            match action {
                Action::InspectHello {
                    device_id: _,
                    capabilities: _,
                } => {
                    actions.extend(dialog.step(start.elapsed(), Event::HelloDecision(true))?);
                }
                Action::Store { ticket, message } => {
                    let operation = store_message(store, machine, &message);
                    let outcome = match custody::store_before_ack(machine, operation).await {
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
                    let mut limits = limits;
                    limits.message_bytes =
                        limits.message_bytes.min(dialog.outbound_message_limit());
                    let message = analyte_poct::encode(
                        &Header {
                            control_id,
                            created: Timestamp::parse(
                                &jiff::Timestamp::now()
                                    .strftime("%Y-%m-%dT%H:%M:%S+00:00")
                                    .to_string(),
                            )?,
                        },
                        &body,
                        limits,
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
    machine: &intake::Connection<'_>,
    message: &Message,
) -> Result<CommitOutcome, Error> {
    let id = ReceiptId::new();
    intake::receive(store, machine, id, message.document().raw().to_vec()).await?;
    match message.body() {
        Body::PatientObservations(_) | Body::NonPatientObservations(_) => {
            intake::interpret(store, machine, id).await?;
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
            intake::remember_hello(store, machine, id).await?;
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
