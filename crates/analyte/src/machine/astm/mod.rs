//! Drives ASTM connections and journals each data frame before ACK (ADR 0003).
use super::connection_input::{self, Input};
use super::driver::{self, Context, Effects, Flow, Run};
use crate::machine::hooks::Pause;
mod error;
// Parking reasons are stored text; `receive::bytes` pins their spellings.
pub(crate) const CONFLICT_REASON: &str = "same ASTM frame number carried conflicting bytes";
pub(crate) const UNSOLICITED_REASON: &str = "unsolicited ASTM frame";
/// The reason parked when a machine ends a transfer before its receipt is complete.
pub(crate) const CUT_OFF_REASON: &str =
    "ASTM transfer ended before a complete receipt was retained";
/// The reason parked at startup for a transfer that was open when Analyte stopped.
pub(crate) const STOPPED_REASON: &str =
    "ASTM transfer was still open when Analyte stopped; its acknowledged frames are kept";
pub(crate) mod orders;
pub(crate) mod settings;
use crate::{deadline, intake};
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::Handle;
pub(crate) use error::Error;
use orders::{Pending, Queries, Request};
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
        reply: Option<orders::Reply>,
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
    /// Present only when the machine is configured to ask for orders.
    queries: Option<Queries<'a>>,
    query_orders: Vec<analyte_store::MachineOrder>,
    work: crate::orders::sending::Work<'a, analyte_config::AstmOrders, Vec<Vec<u8>>>,
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
            queries: Queries::new(settings),
            query_orders: Vec::new(),
            work: crate::orders::sending::Work::new(
                settings.orders().filter(|o| o.send_ahead()),
                machine.machine(),
            ),
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
                .poll_one(self.store, self.machine.machine(), |order, orders| {
                    let limits = self.settings.settings().syntax().limits();
                    let record = analyte_mapping::astm::map_order(order, limits, orders)?.encode();
                    let normal = analyte_astm::OrderTermination::Normal;
                    Ok::<_, Error>(orders::envelope(orders, vec![record], normal))
                })
                .await?
        } else {
            None
        };
        let events = if let Some(reserved) = outgoing {
            vec![Event::Send(reserved)]
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
                reply = self.failure();
            }
            let records = match reply {
                Some(reply) => orders::reserve_query(
                    self.store,
                    self.machine.machine(),
                    reply,
                    until,
                    &mut self.query_orders,
                )
                .await?
                .or_else(|| self.failure().map(|reply| reply.records)),
                None => None,
            };
            records.map_or_else(|| vec![Event::Tick], |records| vec![Event::Send(records)])
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
        let settings = self.settings;
        match action {
            Action::Write(bytes) => {
                let deadline = crate::machine::receive::custody::WriteDeadline::Transport;
                crate::machine::receive::custody::write(run.conn, &bytes, deadline).await?;
            }
            Action::Store(id, number, bytes) => {
                self.commit_frame(id, number, bytes, run).await?;
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
                intake::park_bytes(self.store, self.machine, bytes, CONFLICT_REASON).await?;
            }
            Action::UnexpectedFrame(bytes) => {
                intake::park_bytes(self.store, self.machine, bytes, UNSOLICITED_REASON).await?;
            }
            Action::Rejected(error, bytes) => {
                intake::park_bytes(self.store, self.machine, bytes, &error.to_string()).await?;
            }
            Action::StoreFailed(_) => {
                return Err(run.failures.take().unwrap_or(Error::CommitDeadline));
            }
            Action::LimitReached => return Err(Error::ResourceLimitRefusedTransfer),
            Action::Sent(end) => {
                self.work.astm_finished(self.store, end).await?;
                for order in self.query_orders.drain(..) {
                    orders::finished(self.store, &self.machine.machine().model, &order, end)
                        .await?;
                }
            }
        }
        Ok(Flow::Continue)
    }
}

impl Host<'_, '_> {
    /// The configured failure reply; a machine that cannot query gets silence.
    fn failure(&self) -> Option<orders::Reply> {
        self.queries.and_then(orders::failure)
    }
    async fn commit_frame(
        &mut self,
        id: analyte_dialog::astm::CommitId,
        number: analyte_framing::astm::FrameNumber,
        bytes: Vec<u8>,
        run: &mut Run<'_, Action, Error>,
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
                .pause(Pause::AstmCommitted, run.stop)
                .await
                .map_err(Error::from)?;
        }
        let event = match committed {
            Ok(_) => Event::Stored(id),
            Err(error) => {
                run.failures.keep(Error::CommitRefused(error));
                Event::StoreFailed(id)
            }
        };
        run.actions
            .extend(self.dialog.step(self.start.elapsed(), event)?);

        Ok(())
    }
    async fn end_transfer(&mut self, end: End) -> Result<(), Error> {
        if let Some(transfer_id) = self.transfer.clone() {
            if end == End::Normal {
                let id = ReceiptId::new();
                intake::receive(self.store, self.machine, id, std::mem::take(&mut self.raw))
                    .await?;
                // Link the receipt before interpreting it. Until then the transfer stays
                // with this host, so a failed store parks it as incomplete on close and
                // operator reprocessing rebuilds it from its acknowledged frames.
                self.store
                    .request::<_, analyte_store::IntakeError>(deadline()?, move |s| {
                        s.finish_transfer(
                            &transfer_id,
                            analyte_store::TransferOutcome::Complete(id),
                        )
                    })
                    .await?;
                self.transfer = None;
                let request = match self.queries {
                    Some(queries) => {
                        orders::request(queries, &self.payload)?.map(|request| (queries, request))
                    }
                    None => None,
                };
                if request.is_some() {
                    // A query or cancel carries no results; the order lifecycle, not this
                    // receipt, records what was answered (LIS02 §11).
                    intake::mark_handled(self.store, self.machine.machine(), id).await?;
                }
                match request {
                    None => {
                        intake::interpret(self.store, self.machine.machine(), id).await?;
                    }
                    Some((_, Request::Cancel)) => {
                        self.query = Query::Idle;
                    }
                    Some((_, Request::Unsupported)) => {
                        let started = self.query.started()?;
                        self.query = Query::Answer {
                            reply: self.failure(),
                            until: started,
                        };
                    }
                    Some((queries, Request::Orders(samples))) => {
                        let started = self.query.started()?;
                        if matches!(
                            self.query,
                            Query::Receiving {
                                superseded: true,
                                ..
                            }
                        ) {
                            self.query = Query::Answer {
                                reply: self.failure(),
                                until: started,
                            };
                        } else {
                            self.query = Query::LookingUp {
                                pending: Pending::new(
                                    self.store,
                                    self.machine.machine(),
                                    queries,
                                    samples,
                                    started,
                                )?,
                            };
                        }
                    }
                }
            } else {
                self.transfer = None;
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
                    reply: self.failure(),
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
        let answer = pending.finish().await?;
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
    async fn input(&mut self, host: &mut Context<'_>, input: Input) -> Result<Flow, Error> {
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
        _host: &mut Context<'_>,
        result: &mut Result<(), Error>,
    ) -> Result<(), Error> {
        // Disconnect drains the dialog-owned decoder on every exit, including failed
        // host effects. Transfer and order identities are reconciled below even if
        // parking one piece of framing evidence fails.
        match self.dialog.step(self.start.elapsed(), Event::Disconnected) {
            Ok(actions) => {
                for action in actions {
                    if let Action::Rejected(error, bytes) = action {
                        let cleanup =
                            intake::park_bytes(self.store, self.machine, bytes, &error.to_string())
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
                analyte_dialog::outcome::Host::ConnectionEndedBeforeConfirmation.as_str(),
            )
            .await;
        crate::machine::receive::custody::retain_cleanup(result, cleanup.map_err(Into::into));
        for order in self.query_orders.drain(..) {
            let cleanup = crate::orders::sending::uncertain(
                self.store,
                &self.machine.machine().model,
                &order,
                analyte_dialog::outcome::Host::ConnectionEndedBeforeConfirmation.as_str(),
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
    let reason = analyte_model::Reason::new(CUT_OFF_REASON)?;
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
