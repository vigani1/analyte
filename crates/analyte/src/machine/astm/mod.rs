//! Drives ASTM connections and journals each data frame before ACK (ADR 0003).
use super::connection_input::{self, Input};
use crate::intake::ByteReason;
mod error;
use crate::orders::astm::{Pending, Request};
use crate::{deadline, intake};
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event, Role, Timers};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::Handle;
pub(crate) use error::Error;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &intake::Connection<'_>,
    settings: &analyte_config::AstmStack,
    store: &Handle,
    conn: &mut analyte_transport::Connection,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut state = ConnectionState::open(machine, settings, store)?;
    let result = state.receive_loop(conn, &mut stop).await;
    state.close(result).await
}

struct ConnectionState<'a, 'm> {
    machine: &'a intake::Connection<'m>,
    settings: &'a analyte_config::AstmStack,
    store: &'a Handle,
    config: Config,
    dialog: Dialog,
    start: Instant,
    transfer: Option<Identifier>,
    // Keep the failure until the dialog's NAK and refusal actions have run.
    commit_failure: Option<super::custody::StoreWaitError<analyte_store::IntakeError>>,
    raw: Vec<u8>,
    payload: Vec<u8>,
    input_evidence: connection_input::Custody,
    pending: Option<Pending<'a>>,
    response: Option<crate::orders::astm::Reply>,
    response_deadline: Option<Instant>,
    query_started: Option<Instant>,
    active_orders: Vec<analyte_store::MachineOrder>,
    order_cursor: String,
    next_order_poll: Instant,
}

impl<'a, 'm> ConnectionState<'a, 'm> {
    fn open(
        machine: &'a intake::Connection<'m>,
        settings: &'a analyte_config::AstmStack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let timers = Timers {
            establish: Duration::from_millis(u64::from(settings.timeouts().send_timeout_ms())),
            frame: Duration::from_millis(u64::from(settings.timeouts().send_timeout_ms())),
            receive: Duration::from_millis(u64::from(settings.timeouts().receive_timeout_ms())),
            commit: super::custody::budget(machine.machine()),
            ..Timers::default()
        };
        let config = Config::new(
            analyte_framing::astm::Limit::new(settings.frame_limit())?,
            timers,
            match settings.contention_priority() {
                analyte_config::Priority::Host => Role::Instrument,
                analyte_config::Priority::Instrument => Role::Computer,
            },
            settings.syntax().limits().bytes(),
            settings.syntax().limits().records(),
            true,
        )?
        .with_record_boundaries(settings.record_boundaries())
        .with_checksum_hex(settings.checksum_policy());
        let epoch = Epoch(uuid::Uuid::now_v7().as_u128());
        let dialog = Dialog::new(config.clone(), epoch);
        let start = Instant::now();
        let transfer: Option<Identifier> = None;
        let raw = Vec::new();
        let payload = Vec::new();
        let pending: Option<Pending<'_>> = None;
        let response: Option<crate::orders::astm::Reply> = None;
        let response_deadline = None;
        let query_started = None;
        let active_orders = Vec::new();
        let order_cursor = String::new();
        let next_order_poll = Instant::now();
        Ok(Self {
            machine,
            settings,
            store,
            config,
            dialog,
            start,
            transfer,
            commit_failure: None,
            raw,
            payload,
            input_evidence: connection_input::Custody::default(),
            pending,
            response,
            response_deadline,
            query_started,
            active_orders,
            order_cursor,
            next_order_poll,
        })
    }
    async fn receive_loop(
        &mut self,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        let machine = self.machine;
        loop {
            let event = tokio::select! {
                biased;
                () = super::stopping(stop) => break,
                () = std::future::ready(()), if self.response.is_some() && self.dialog.next_deadline().is_none() => None,
                () = tokio::time::sleep(self.dialog.next_deadline().map_or(Duration::from_secs(1), |d: Duration| d.saturating_sub(self.start.elapsed()))) => None,
                event = connection_input::receive(conn) => match event {
                    Ok(event) => Some(event),
                    Err(analyte_transport::Error::Deadline(_)) => None,
                    Err(error) => return Err(error.into()),
                },
                answer = async { match self.pending.as_mut() { Some(query) => query.finish(machine.machine()).await, None => std::future::pending().await } }, if self.pending.is_some() => {
                    self.response_deadline = self.pending.as_ref().map(Pending::deadline);
                    self.pending = None;
                    self.response = answer?;
                    None
                },
            };
            let bytes = match event {
                Some(Input::Opened { session }) => {
                    tracing::info!(
                        machine = machine.machine().id.as_str(),
                        session,
                        "ASTM connection opened"
                    );
                    self.dialog =
                        Dialog::new(self.config.clone(), Epoch(uuid::Uuid::now_v7().as_u128()));
                    Vec::new()
                }
                Some(Input::Bytes { bytes }) => bytes,
                Some(Input::Closed(_)) => break,
                Some(Input::Failed(_) | Input::RecordingFailed(_)) | Some(Input::Tick) | None => {
                    Vec::new()
                }
            };
            self.receive_bytes(bytes, conn, stop).await?;
        }
        Ok(())
    }
    async fn receive_bytes(
        &mut self,
        bytes: Vec<u8>,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        self.input_evidence.begin(&bytes);
        let events = self.outgoing_events(bytes).await?;
        for event in events {
            if let Event::Byte(_) = &event {
                self.input_evidence.fed();
            }
            self.process(event, conn, stop).await?;
        }
        self.input_evidence.complete();
        Ok(())
    }
    async fn outgoing_events(&mut self, bytes: Vec<u8>) -> Result<Vec<Event>, Error> {
        let machine = self.machine;
        let store = self.store;
        let outgoing = if bytes.is_empty()
            && self.dialog.next_deadline().is_none()
            && self.response.is_none()
            && self.pending.is_none()
            && Instant::now() >= self.next_order_poll
        {
            self.next_order_poll = Instant::now()
                .checked_add(Duration::from_secs(1))
                .ok_or_else(|| Error::OrderPollDeadlineOverflow)?;
            crate::orders::sending::next(store, machine.machine(), &mut self.order_cursor).await?
        } else {
            None
        };
        let events = if let Some(reserved) = outgoing {
            self.active_orders.push(reserved.order);
            vec![Event::Send(crate::orders::astm::envelope(
                machine.machine(),
                vec![reserved.payload.record()?.to_vec()],
                analyte_astm::OrderTermination::Normal,
            ))]
        } else if bytes.is_empty()
            && self.response.is_some()
            && self.dialog.next_deadline().is_none()
        {
            if self
                .response_deadline
                .is_some_and(|until| Instant::now() >= until)
            {
                self.response = crate::orders::astm::failure(machine.machine());
            }
            match self.response.take() {
                Some(reply) => match crate::orders::astm::reserve_query(
                    store,
                    machine.machine(),
                    reply,
                    self.response_deadline.unwrap_or_else(Instant::now),
                    &mut self.active_orders,
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
    async fn process(
        &mut self,
        event: Event,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        let machine = self.machine;
        let settings = self.settings;
        let store = self.store;
        let mut actions = VecDeque::new();
        if matches!(event, Event::Byte(analyte_framing::astm::ENQ)) {
            // Start the query budget before admission and journaling.
            // A new transfer supersedes a pending lookup, so no cached
            // order response can wait behind another machine message.
            self.query_started = Some(Instant::now());
            if self.pending.is_some() || self.response.is_some() {
                self.pending = None;
                self.response = crate::orders::astm::failure(machine.machine());
                self.response_deadline = None;
            }
            let expected = settings
                .frame_limit()
                .checked_add(machine.machine().snapshot.canonical().len())
                .ok_or_else(|| Error::AstmAdmissionSizeOverflow)?;
            let capacity = store
                .request(deadline()?, move |s| s.admission(expected))
                .await;
            if let Err(error) = &capacity {
                tracing::error!(machine = machine.machine().id.as_str(), error_kind = %error, "storage admission refused");
            }
            actions.extend(
                self.dialog
                    .step(self.start.elapsed(), Event::Capacity(capacity.is_ok()))?,
            );
        }
        actions.extend(self.dialog.step(self.start.elapsed(), event)?);
        while let Some(action) = actions.pop_front() {
            self.effect(action, conn, stop, &mut actions).await?;
        }
        Ok(())
    }
    async fn effect(
        &mut self,
        action: Action,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
        actions: &mut VecDeque<Action>,
    ) -> Result<(), Error> {
        let machine = self.machine;
        let settings = self.settings;
        let store = self.store;
        match action {
            Action::Write(bytes) => {
                conn.write(bytes).await?;
            }
            Action::Commit(id, number, bytes) => {
                self.commit_frame(id, number, bytes, stop, actions).await?;
            }
            Action::Record(record) => {
                if analyte_astm::starts_query(&record) && self.query_started.is_none() {
                    self.query_started = Some(Instant::now());
                }
                if self
                    .payload
                    .len()
                    .checked_add(record.len())
                    .is_none_or(|n| n > settings.syntax().limits().bytes())
                {
                    return Err(Error::AstmMessageBoundExceeded);
                }
                self.payload.extend_from_slice(&record);
            }
            Action::Ended(end) => {
                self.end_transfer(end).await?;
            }
            Action::Deadline(_) => {}
            Action::Refused(error) => return Err(error.into()),
            Action::Conflict(bytes) => {
                intake::park_bytes(store, machine, bytes, ByteReason::AstmConflict).await?;
            }
            Action::UnexpectedFrame(bytes) => {
                intake::park_bytes(store, machine, bytes, ByteReason::AstmUnsolicited).await?;
            }
            Action::Rejected(error, bytes) => {
                intake::park_bytes(store, machine, bytes, ByteReason::AstmFrame(&error)).await?;
            }
            Action::CommitFailed(_) => {
                return Err(self
                    .commit_failure
                    .take()
                    .map_or(Error::AstmCommitDeadline, Error::AstmCommitRefused));
            }
            Action::LimitReached => return Err(Error::AstmResourceLimitRefusedTransfer),
            Action::Sent(end) => {
                for order in self.active_orders.drain(..) {
                    crate::orders::sending::finished(store, &order, end).await?;
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
    ) -> Result<(), Error> {
        #[cfg(not(feature = "test-hooks"))]
        let _ = stop;
        let machine = self.machine;
        let store = self.store;
        let settings = self.settings;

        let transfer_id = Identifier::new(&format!("{:032x}-{}", id.epoch().0, id.transfer()))?;
        self.transfer = Some(transfer_id.clone());
        if self
            .raw
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > settings.syntax().limits().bytes())
        {
            return Err(Error::AstmTransferWireByteBoundExceeded);
        }
        self.raw.extend_from_slice(&bytes);
        let provenance = analyte_store::FrameProvenance {
            machine: machine.machine().id.clone(),
            driver: intake::driver(machine.machine())?,
            config: machine.machine().snapshot.canonical().as_bytes().to_vec(),
        };
        let sequence = analyte_store::FrameSequence::new(
            number.value(),
            analyte_framing::astm::FrameNumber::MAX.value(),
        )
        .map_err(analyte_store::Error::from)?;
        let committed = super::custody::store_before_ack(
            machine.machine(),
            store.request(deadline()?, move |s| {
                s.journal_frame(&transfer_id, sequence, &bytes, &provenance)
            }),
        )
        .await;
        #[cfg(feature = "test-hooks")]
        if committed.is_ok()
            && let Some(hook) = &machine.machine().hooks.astm_committed
        {
            hook.pause(stop).await.map_err(Error::TestHook)?;
        }
        let event = match committed {
            Ok(_) => Event::Committed(id),
            Err(error) => {
                self.commit_failure = Some(error);
                Event::CommitFailed(id)
            }
        };
        actions.extend(self.dialog.step(self.start.elapsed(), event)?);

        Ok(())
    }
    async fn end_transfer(&mut self, end: End) -> Result<(), Error> {
        let machine = self.machine;
        let store = self.store;

        if let Some(transfer_id) = self.transfer.take() {
            if end == End::Normal {
                let id = ReceiptId::new();
                intake::receive(store, machine, id, std::mem::take(&mut self.raw)).await?;
                match crate::orders::astm::request(machine.machine(), &self.payload)? {
                    None => {
                        intake::interpret(store, machine.machine(), id).await?;
                    }
                    Some(Request::Cancel) => {
                        self.pending = None;
                        self.response = None;
                        self.response_deadline = None;
                    }
                    Some(Request::Unsupported) => {
                        self.pending = None;
                        self.response = crate::orders::astm::failure(machine.machine());
                        self.response_deadline = None;
                    }
                    Some(Request::Orders(samples)) => {
                        if self.pending.is_some() || self.response.is_some() {
                            self.pending = None;
                            self.response = crate::orders::astm::failure(machine.machine());
                            self.response_deadline = None;
                        } else {
                            self.pending = Some(Pending::new(
                                store,
                                machine.machine(),
                                samples,
                                self.query_started.unwrap_or_else(Instant::now),
                            )?);
                        }
                    }
                }
                store
                    .request::<_, analyte_store::Error>(deadline()?, move |s| {
                        Ok::<_, analyte_store::Error>(s.finish_transfer(
                            &transfer_id,
                            analyte_store::TransferOutcome::Complete(id),
                        )?)
                    })
                    .await?;
            } else {
                finish_incomplete(store, transfer_id).await?;
            }
        }
        self.raw.clear();
        self.payload.clear();
        self.query_started = None;

        Ok(())
    }
    async fn close(&mut self, mut result: Result<(), Error>) -> Result<(), Error> {
        // A failed NAK write can interrupt the action queue before CommitFailed.
        // Keep that transport failure and the original custody cause together.
        if let Some(error) = self.commit_failure.take() {
            super::custody::retain_cleanup(&mut result, Err(Error::AstmCommitRefused(error)));
        }
        // Disconnect drains the dialog-owned decoder on every exit, including failed
        // host effects. Transfer and order identities are reconciled below even if
        // parking one piece of framing evidence fails.
        match self.dialog.step(self.start.elapsed(), Event::Disconnect) {
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
                        super::custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
                    }
                }
            }
            Err(error) => super::custody::retain_cleanup(&mut result, Err(error.into())),
        }
        let cleanup = self.input_evidence.retain(self.store, self.machine).await;
        super::custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
        for order in self.active_orders.drain(..) {
            let cleanup = crate::orders::sending::uncertain(
                self.store,
                &order,
                "connection-ended-before-transfer-confirmation",
            )
            .await;
            super::custody::retain_cleanup(&mut result, cleanup.map_err(Into::into));
        }
        if let Some(transfer) = self.transfer.take() {
            let cleanup = finish_incomplete(self.store, transfer).await;
            super::custody::retain_cleanup(&mut result, cleanup);
        }

        result
    }
}

async fn finish_incomplete(store: &Handle, transfer: Identifier) -> Result<(), Error> {
    let reason =
        analyte_model::Reason::new("ASTM transfer ended before a complete receipt was retained")?;
    store
        .request::<_, analyte_store::Error>(deadline()?, move |store| {
            Ok::<_, analyte_store::Error>(store.finish_transfer(
                &transfer,
                analyte_store::TransferOutcome::Incomplete(reason),
            )?)
        })
        .await?;
    Ok(())
}
