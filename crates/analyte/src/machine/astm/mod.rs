//! Drives ASTM connections and journals each data frame before ACK (ADR 0003).
use super::connection_input::{self, Input};
use super::driver::{Context, Effects, Failures, Flow};
use crate::intake::ByteReason;
use crate::machine::hooks::Pause;
mod error;
pub(crate) mod settings;
use crate::orders::astm::{Pending, Request};
use crate::{deadline, intake};
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::watch;

enum Query<'a> {
    Idle,
    Receiving {
        started: Instant,
        superseded: bool,
    },
    LookingUp {
        pending: Pending<'a>,
    },
    Answer {
        reply: Option<crate::orders::astm::Reply>,
        until: Instant,
    },
}
impl Query<'_> {
    fn started(&self) -> Result<Instant, Error> {
        match self {
            Self::Receiving { started, .. } => Ok(*started),
            Self::Idle | Self::LookingUp { .. } | Self::Answer { .. } => {
                Err(Error::QueryStartMissing)
            }
        }
    }
}

pub(super) struct Host<'a, 'm> {
    machine: &'a intake::ConnectionEvidence<'m>,
    settings: &'a analyte_config::AstmStack,
    store: &'a Handle,
    config: Config,
    dialog: Dialog,
    start: Instant,
    transfer: Option<Identifier>,
    raw: Vec<u8>,
    payload: Vec<u8>,
    input_evidence: connection_input::Custody,
    query: Query<'a>,
    query_orders: Vec<analyte_store::MachineOrder>,
    work: crate::orders::sending::Work<Vec<u8>>,
}

impl<'a, 'm> Host<'a, 'm> {
    pub(super) fn open(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a analyte_config::AstmStack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let config = settings::dialog(
            settings,
            crate::machine::receive::custody::budget(machine.machine()),
        )?;
        let epoch = Epoch(uuid::Uuid::now_v7().as_u128());
        let dialog = Dialog::new(config.clone(), epoch);
        Ok(Self {
            machine,
            settings,
            store,
            config,
            dialog,
            start: Instant::now(),
            transfer: None,
            raw: Vec::new(),
            payload: Vec::new(),
            input_evidence: connection_input::Custody::default(),
            query: Query::Idle,
            query_orders: Vec::new(),
            work: crate::orders::sending::Work::default(),
        })
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        self.input_evidence.begin(&bytes);
        let events = self.outgoing_events(bytes).await?;
        for event in events {
            if let Event::Byte(_) = &event {
                self.input_evidence.fed();
            }
            if self.transition(event, conn, stop).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        self.input_evidence.complete();
        Ok(Flow::Continue)
    }
    async fn outgoing_events(&mut self, bytes: Vec<u8>) -> Result<Vec<Event>, Error> {
        let outgoing = if bytes.is_empty()
            && self.dialog.next_deadline().is_none()
            && matches!(self.query, Query::Idle | Query::Receiving { .. })
        {
            self.work
                .poll_one(self.store, self.machine.machine(), |order| {
                    analyte_mapping::astm::map_order(
                        order,
                        self.settings.settings().syntax().limits(),
                        self.settings
                            .orders()
                            .ok_or(analyte_mapping::astm::OrderError::Shared(
                                analyte_mapping::OrderMappingError::Disabled,
                            ))?,
                    )
                    .map(|record| record.encode())
                })
                .await?
        } else {
            None
        };
        let events = if let Some(reserved) = outgoing {
            vec![Event::Send(crate::orders::astm::envelope(
                self.machine.machine(),
                vec![reserved],
                analyte_astm::OrderTermination::Normal,
            ))]
        } else if bytes.is_empty()
            && matches!(self.query, Query::Answer { .. })
            && self.dialog.next_deadline().is_none()
        {
            let Query::Answer { mut reply, until } =
                std::mem::replace(&mut self.query, Query::Idle)
            else {
                return Err(Error::QueryStartMissing);
            };
            if Instant::now() >= until {
                reply = crate::orders::astm::failure(self.machine.machine());
            }
            match reply {
                Some(reply) => match crate::orders::astm::reserve_query(
                    self.store,
                    self.machine.machine(),
                    reply,
                    until,
                    &mut self.query_orders,
                )
                .await?
                {
                    Some(records) => vec![Event::Send(records)],
                    None => vec![Event::Tick],
                },
                None => vec![Event::Tick],
            }
        } else if bytes.is_empty() {
            vec![Event::Tick]
        } else {
            bytes.into_iter().map(Event::Byte).collect()
        };
        Ok(events)
    }
    async fn transition(
        &mut self,
        event: Event,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let settings = self.settings;
        let mut actions = VecDeque::new();
        if matches!(event, Event::Byte(analyte_framing::astm::ENQ)) {
            // Start the query budget before admission and journaling.
            // A new transfer supersedes a pending lookup, so no cached
            // order response can wait behind another machine message.
            self.query = Query::Receiving {
                started: Instant::now(),
                superseded: matches!(self.query, Query::LookingUp { .. } | Query::Answer { .. }),
            };
            let expected = settings
                .settings()
                .frame_limit()
                .checked_add(self.machine.machine().snapshot.canonical().len())
                .ok_or_else(|| Error::AdmissionSizeOverflow)?;
            let capacity = self
                .store
                .request(deadline()?, move |s| s.admission(expected))
                .await;
            if let Err(error) = &capacity {
                tracing::error!(machine = self.machine.machine().id.as_str(), error_kind = %error, "storage admission refused");
            }
            actions.extend(
                self.dialog
                    .step(self.start.elapsed(), Event::Capacity(capacity.is_ok()))?,
            );
        }
        actions.extend(self.dialog.step(self.start.elapsed(), event)?);
        self.apply(actions.into(), conn, stop).await
    }
    async fn apply(
        &mut self,
        actions: Vec<Action>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Flow, Error> {
        let mut actions: VecDeque<_> = actions.into();
        let mut failures = Failures::new();
        let result = async {
            while let Some(action) = actions.pop_front() {
                self.effect(action, conn, stop, &mut actions, &mut failures)
                    .await?;
            }
            Ok(Flow::Continue)
        }
        .await;
        failures.finish(result)
    }
    async fn effect(
        &mut self,
        action: Action,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
        failures: &mut Failures<Error>,
    ) -> Result<(), Error> {
        let settings = self.settings;
        match action {
            Action::Write(bytes) => {
                conn.write(bytes).await?;
            }
            Action::Store(id, number, bytes) => {
                self.commit_frame(id, number, bytes, stop, actions, failures)
                    .await?;
            }
            Action::Record(record) => {
                if analyte_astm::starts_query(&record)
                    && !matches!(self.query, Query::Receiving { .. })
                {
                    return Err(Error::QueryStartMissing);
                }
                if self
                    .payload
                    .len()
                    .checked_add(record.len())
                    .is_none_or(|n| n > settings.settings().syntax().limits().bytes())
                {
                    return Err(Error::MessageBoundExceeded);
                }
                self.payload.extend_from_slice(&record);
            }
            Action::Ended(end) => {
                self.end_transfer(end).await?;
            }
            Action::Deadline(_) => {}
            Action::Refused(error) => return Err(error.into()),
            Action::Conflict(bytes) => {
                intake::park_bytes(self.store, self.machine, bytes, ByteReason::AstmConflict)
                    .await?;
            }
            Action::UnexpectedFrame(bytes) => {
                intake::park_bytes(self.store, self.machine, bytes, ByteReason::AstmUnsolicited)
                    .await?;
            }
            Action::Rejected(error, bytes) => {
                intake::park_bytes(
                    self.store,
                    self.machine,
                    bytes,
                    ByteReason::AstmFrame(&error),
                )
                .await?;
            }
            Action::StoreFailed(_) => {
                return Err(failures.take().unwrap_or(Error::CommitDeadline));
            }
            Action::LimitReached => return Err(Error::ResourceLimitRefusedTransfer),
            Action::Sent(end) => {
                self.work.astm_finished(self.store, end).await?;
                for order in self.query_orders.drain(..) {
                    crate::orders::astm::finished(self.store, &order, end).await?;
                }
            }
        }
        Ok(())
    }
    async fn commit_frame(
        &mut self,
        id: analyte_dialog::astm::CommitId,
        number: analyte_framing::astm::FrameNumber,
        bytes: Vec<u8>,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
        failures: &mut Failures<Error>,
    ) -> Result<(), Error> {
        let settings = self.settings;

        let transfer_id = Identifier::new(&format!("{:032x}-{}", id.epoch().0, id.transfer()))?;
        self.transfer = Some(transfer_id.clone());
        if self
            .raw
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > settings.settings().syntax().limits().bytes())
        {
            return Err(Error::TransferWireByteBoundExceeded);
        }
        self.raw.extend_from_slice(&bytes);
        let provenance = analyte_store::FrameProvenance {
            machine: self.machine.machine().id.clone(),
            driver: intake::driver(self.machine.machine())?,
            config: self
                .machine
                .machine()
                .snapshot
                .canonical()
                .as_bytes()
                .to_vec(),
        };
        let sequence = analyte_store::FrameSequence::new(
            number.value(),
            analyte_framing::astm::FrameNumber::MAX.value(),
        )?;
        let committed = crate::machine::receive::custody::store_before_ack(
            self.machine.machine(),
            self.store.request(deadline()?, move |s| {
                s.journal_frame(&transfer_id, sequence, &bytes, &provenance)
            }),
        )
        .await;

        if committed.is_ok() && self.machine.machine().hooks.enabled(Pause::AstmCommitted) {
            self.machine
                .machine()
                .hooks
                .pause(Pause::AstmCommitted, stop)
                .await
                .map_err(Error::from)?;
        }
        let event = match committed {
            Ok(_) => Event::Stored(id),
            Err(error) => {
                failures.keep(Error::CommitRefused(error));
                Event::StoreFailed(id)
            }
        };
        actions.extend(self.dialog.step(self.start.elapsed(), event)?);

        Ok(())
    }
    async fn end_transfer(&mut self, end: End) -> Result<(), Error> {
        if let Some(transfer_id) = self.transfer.take() {
            if end == End::Normal {
                let id = ReceiptId::new();
                intake::receive(self.store, self.machine, id, std::mem::take(&mut self.raw))
                    .await?;
                match crate::orders::astm::request(self.machine.machine(), &self.payload)? {
                    None => {
                        intake::interpret(self.store, self.machine.machine(), id).await?;
                    }
                    Some(Request::Cancel) => {
                        self.query = Query::Idle;
                    }
                    Some(Request::Unsupported) => {
                        let started = self.query.started()?;
                        self.query = Query::Answer {
                            reply: crate::orders::astm::failure(self.machine.machine()),
                            until: started,
                        };
                    }
                    Some(Request::Orders(samples)) => {
                        let started = self.query.started()?;
                        if matches!(
                            self.query,
                            Query::Receiving {
                                superseded: true,
                                ..
                            }
                        ) {
                            self.query = Query::Answer {
                                reply: crate::orders::astm::failure(self.machine.machine()),
                                until: started,
                            };
                        } else {
                            self.query = Query::LookingUp {
                                pending: Pending::new(
                                    self.store,
                                    self.machine.machine(),
                                    samples,
                                    started,
                                )?,
                            };
                        }
                    }
                }
                self.store
                    .request::<_, analyte_store::IntakeError>(deadline()?, move |s| {
                        s.finish_transfer(
                            &transfer_id,
                            analyte_store::TransferOutcome::Complete(id),
                        )
                    })
                    .await?;
            } else {
                finish_incomplete(self.store, transfer_id).await?;
            }
        }
        self.raw.clear();
        self.payload.clear();
        if let Query::Receiving {
            started,
            superseded,
        } = self.query
        {
            self.query = if superseded {
                Query::Answer {
                    reply: crate::orders::astm::failure(self.machine.machine()),
                    until: started,
                }
            } else {
                Query::Idle
            };
        }

        Ok(())
    }
}

impl Effects for Host<'_, '_> {
    type Error = Error;
    fn wait(&self) -> Result<Option<Duration>, Error> {
        if matches!(self.query, Query::Answer { .. }) && self.dialog.next_deadline().is_none() {
            return Ok(Some(Duration::ZERO));
        }
        Ok(Some(self.dialog.next_deadline().map_or(
            crate::orders::sending::ORDER_POLL_INTERVAL,
            |deadline| deadline.saturating_sub(self.start.elapsed()),
        )))
    }
    async fn background(&mut self) -> Result<(), Error> {
        let Query::LookingUp { pending } = &mut self.query else {
            return std::future::pending().await;
        };
        let answer = pending.finish(self.machine.machine()).await?;
        let until = pending.deadline();
        self.query = match answer {
            Some(reply) => Query::Answer {
                reply: Some(reply),
                until,
            },
            None => Query::Idle,
        };
        Ok(())
    }
    async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
        let bytes = match input {
            Input::Opened { session } => {
                tracing::info!(
                    machine = self.machine.machine().id.as_str(),
                    session,
                    "ASTM connection opened"
                );
                self.dialog =
                    Dialog::new(self.config.clone(), Epoch(uuid::Uuid::now_v7().as_u128()));
                Vec::new()
            }
            Input::Bytes { bytes } => bytes,
            Input::Closed(_) => return Ok(Flow::Stop),
            Input::Failed(_) | Input::Tick => Vec::new(),
        };
        self.receive_bytes(bytes, host.conn, &mut host.stop).await
    }
    async fn close(
        &mut self,
        _host: &mut Context<'_, '_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        // Disconnect drains the dialog-owned decoder on every exit, including failed
        // host effects. Transfer and order identities are reconciled below even if
        // parking one piece of framing evidence fails.
        match self.dialog.step(self.start.elapsed(), Event::Disconnected) {
            Ok(actions) => {
                for action in actions {
                    if let Action::Rejected(error, bytes) = action {
                        let cleanup = intake::park_bytes(
                            self.store,
                            self.machine,
                            bytes,
                            ByteReason::AstmFrame(&error),
                        )
                        .await;
                        crate::machine::receive::custody::retain_cleanup(
                            result,
                            cleanup.map_err(Into::into),
                        );
                    }
                }
            }
            Err(error) => {
                crate::machine::receive::custody::retain_cleanup(result, Err(error.into()));
            }
        }
        let cleanup = self.input_evidence.retain(self.store, self.machine).await;
        crate::machine::receive::custody::retain_cleanup(result, cleanup.map_err(Into::into));
        let cleanup = self
            .work
            .close(
                self.store,
                crate::orders::outcome::Code::ConnectionEndedBeforeConfirmation.into(),
            )
            .await;
        crate::machine::receive::custody::retain_cleanup(result, cleanup.map_err(Into::into));
        for order in self.query_orders.drain(..) {
            let cleanup = crate::orders::sending::uncertain(
                self.store,
                &order,
                crate::orders::outcome::Code::ConnectionEndedBeforeConfirmation.into(),
            )
            .await;
            crate::machine::receive::custody::retain_cleanup(result, cleanup.map_err(Into::into));
        }
        if let Some(transfer) = self.transfer.take() {
            let cleanup = finish_incomplete(self.store, transfer).await;
            crate::machine::receive::custody::retain_cleanup(result, cleanup);
        }

        Ok(())
    }
}

async fn finish_incomplete(store: &Handle, transfer: Identifier) -> Result<(), Error> {
    let reason =
        analyte_model::Reason::new("ASTM transfer ended before a complete receipt was retained")?;
    store
        .request::<_, analyte_store::IntakeError>(deadline()?, move |store| {
            store.finish_transfer(
                &transfer,
                analyte_store::TransferOutcome::Incomplete(reason),
            )
        })
        .await?;
    Ok(())
}
