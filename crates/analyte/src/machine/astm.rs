//! ASTM journaling precedes every data ACK (ADR 0003).
use super::connection_input::{self, Input};
use crate::orders::astm::{Pending, Request};
use crate::{Error, deadline, evidence, invalid};
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Event, Role, Timers};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::Handle;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub(super) async fn session(
    machine: &evidence::Connection<'_>,
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
    machine: &'a evidence::Connection<'m>,
    settings: &'a analyte_config::AstmStack,
    store: &'a Handle,
    config: Config,
    dialog: Dialog,
    start: Instant,
    wake: Option<Duration>,
    transfer: Option<Identifier>,
    raw: Vec<u8>,
    payload: Vec<u8>,
    partial: Vec<u8>,
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
        machine: &'a evidence::Connection<'m>,
        settings: &'a analyte_config::AstmStack,
        store: &'a Handle,
    ) -> Result<Self, Error> {
        let timers = Timers {
            establish: Duration::from_millis(u64::from(settings.timeouts().send_timeout_ms())),
            frame: Duration::from_millis(u64::from(settings.timeouts().send_timeout_ms())),
            receive: Duration::from_millis(u64::from(settings.timeouts().receive_timeout_ms())),
            commit: Duration::from_secs(5),
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
        )?;
        let epoch = Epoch(uuid::Uuid::now_v7().as_u128());
        let dialog = Dialog::new(config.clone(), epoch);
        let start = Instant::now();
        let wake = None;
        let transfer: Option<Identifier> = None;
        let raw = Vec::new();
        let payload = Vec::new();
        let partial = Vec::new();
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
            wake,
            transfer,
            raw,
            payload,
            partial,
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
                () = std::future::ready(()), if self.response.is_some() && self.wake.is_none() => None,
                () = tokio::time::sleep(self.wake.map_or(Duration::from_secs(1), |d: Duration| d.saturating_sub(self.start.elapsed()))) => None,
                event = connection_input::receive(conn) => match event {
                    Ok(event) => Some(event),
                    Err(analyte_transport::Error::Deadline(_)) => None,
                    Err(error) => return Err(error.into()),
                },
                answer = async { match self.pending.as_mut() { Some(query) => query.finish(machine).await, None => std::future::pending().await } }, if self.pending.is_some() => {
                    self.response_deadline = self.pending.as_ref().map(Pending::deadline);
                    self.pending = None;
                    self.response = answer?;
                    None
                },
            };
            let bytes = match event {
                Some(Input::Opened { session }) => {
                    tracing::info!(
                        machine = machine.id.as_str(),
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
        let settings = self.settings;
        let events = self.outgoing_events(bytes).await?;
        for event in events {
            if let Event::Byte(byte) = &event {
                if *byte == analyte_framing::astm::STX {
                    self.partial.clear();
                }
                if *byte == analyte_framing::astm::STX || !self.partial.is_empty() {
                    if self.partial.len() > settings.frame_limit() {
                        return Err(invalid("ASTM partial frame bound exceeded"));
                    }
                    self.partial.push(*byte);
                }
            }
            self.process(event, conn, stop).await?;
        }
        Ok(())
    }
    async fn outgoing_events(&mut self, bytes: Vec<u8>) -> Result<Vec<Event>, Error> {
        let machine = self.machine;
        let store = self.store;
        let outgoing = if bytes.is_empty()
            && self.wake.is_none()
            && self.response.is_none()
            && self.pending.is_none()
            && Instant::now() >= self.next_order_poll
        {
            self.next_order_poll = Instant::now()
                .checked_add(Duration::from_secs(1))
                .ok_or_else(|| invalid("order poll deadline overflow"))?;
            crate::orders::sending::next(store, machine, &mut self.order_cursor).await?
        } else {
            None
        };
        let events = if let Some(reserved) = outgoing {
            self.active_orders.push(reserved.order);
            vec![Event::Send(crate::orders::astm::envelope(
                vec![reserved.payload.record()?.to_vec()],
                b'N',
            ))]
        } else if bytes.is_empty() && self.response.is_some() && self.wake.is_none() {
            if self
                .response_deadline
                .is_some_and(|until| Instant::now() >= until)
            {
                self.response = crate::orders::astm::failure(machine);
            }
            match self.response.take() {
                Some(reply) => match crate::orders::sending::reserve_query(
                    store,
                    machine,
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
                self.response = crate::orders::astm::failure(machine);
                self.response_deadline = None;
            }
            let expected = settings
                .frame_limit()
                .checked_add(machine.snapshot.canonical().len())
                .ok_or_else(|| invalid("ASTM admission size overflow"))?;
            let capacity = store
                .request(deadline(), move |s| s.admission(expected))
                .await;
            if let Err(error) = &capacity {
                tracing::error!(machine = machine.id.as_str(), error_kind = %error, "storage admission refused");
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
                if bytes.as_slice() == [analyte_framing::astm::ACK] {
                    self.partial.clear();
                }
                conn.write(bytes).await?;
            }
            Action::Commit(id, number, bytes) => {
                self.commit_frame(id, number, bytes, stop, actions).await?;
            }
            Action::Record(record) => {
                if record.first() == Some(&b'Q') && self.query_started.is_none() {
                    self.query_started = Some(Instant::now());
                }
                if self
                    .payload
                    .len()
                    .checked_add(record.len())
                    .is_none_or(|n| n > settings.syntax().limits().bytes())
                {
                    return Err(invalid("ASTM message bound exceeded"));
                }
                self.payload.extend_from_slice(&record);
            }
            Action::Ended(end) => {
                self.end_transfer(end).await?;
            }
            Action::Deadline(next) => self.wake = next,
            Action::Refused(error) => return Err(error.into()),
            Action::Conflict(bytes) => {
                evidence::park_bytes(
                    store,
                    machine,
                    bytes,
                    "same ASTM frame number carried conflicting bytes",
                )
                .await?;
            }
            Action::UnexpectedFrame(bytes) => {
                evidence::park_bytes(store, machine, bytes, "unsolicited ASTM frame").await?;
            }
            Action::Rejected(error, bytes) => {
                if self.partial.len() != 1 {
                    self.partial.clear();
                }
                evidence::park_bytes(store, machine, bytes, &error.to_string()).await?;
            }
            Action::CommitFailed(_) | Action::LimitReached => {
                return Err(invalid("ASTM storage or resource limit refused transfer"));
            }
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
            return Err(invalid("ASTM transfer wire-byte bound exceeded"));
        }
        self.raw.extend_from_slice(&bytes);
        let provenance = analyte_store::FrameProvenance {
            machine: machine.id.clone(),
            driver: evidence::driver(machine)?,
            config: machine.snapshot.canonical().as_bytes().to_vec(),
        };
        let sequence = analyte_store::FrameSequence::new(
            number.value(),
            analyte_framing::astm::FrameNumber::MAX.value(),
        )?;
        let committed = store
            .request(deadline(), move |s| {
                s.journal_frame(&transfer_id, sequence, &bytes, &provenance)
            })
            .await;
        if let Err(error) = &committed {
            tracing::error!(machine = machine.id.as_str(), error_kind = %error, "ASTM frame commit refused");
        }
        #[cfg(feature = "test-hooks")]
        if committed.is_ok()
            && let Some(hook) = &machine.astm_committed
        {
            hook.pause(stop).await?;
        }
        actions.extend(self.dialog.step(
            self.start.elapsed(),
            if committed.is_ok() {
                Event::Committed(id)
            } else {
                Event::CommitFailed(id)
            },
        )?);

        Ok(())
    }
    async fn end_transfer(&mut self, end: End) -> Result<(), Error> {
        let machine = self.machine;
        let store = self.store;

        if let Some(transfer_id) = self.transfer.take() {
            if end == End::Normal {
                let id = ReceiptId::new();
                evidence::receive(store, machine, id, std::mem::take(&mut self.raw)).await?;
                match crate::orders::astm::request(machine, &self.payload)? {
                    None => {
                        evidence::interpret(store, machine, id).await?;
                    }
                    Some(Request::Cancel) => {
                        self.pending = None;
                        self.response = None;
                        self.response_deadline = None;
                    }
                    Some(Request::Unsupported) => {
                        self.pending = None;
                        self.response = crate::orders::astm::failure(machine);
                        self.response_deadline = None;
                    }
                    Some(Request::Orders(samples)) => {
                        if self.pending.is_some() || self.response.is_some() {
                            self.pending = None;
                            self.response = crate::orders::astm::failure(machine);
                            self.response_deadline = None;
                        } else {
                            self.pending = Some(Pending::new(
                                store,
                                machine,
                                samples,
                                self.query_started.unwrap_or_else(Instant::now),
                            )?);
                        }
                    }
                }
                store
                    .request(deadline(), move |s| {
                        s.finish_transfer(
                            &transfer_id,
                            analyte_store::TransferOutcome::Complete(id),
                        )
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
    async fn close(&mut self, result: Result<(), Error>) -> Result<(), Error> {
        for order in self.active_orders.drain(..) {
            crate::orders::sending::uncertain(
                self.store,
                &order,
                "connection-ended-before-transfer-confirmation",
            )
            .await?;
        }
        if let Some(transfer) = self.transfer.take() {
            finish_incomplete(self.store, transfer).await?;
        }
        if !self.partial.is_empty() {
            evidence::park_bytes(
                self.store,
                self.machine,
                std::mem::take(&mut self.partial),
                "incomplete ASTM frame when session stopped",
            )
            .await?;
        }
        result
    }
}

async fn finish_incomplete(store: &Handle, transfer: Identifier) -> Result<(), Error> {
    let reason =
        analyte_model::Reason::new("ASTM transfer ended before a complete receipt was retained")?;
    store
        .request(deadline(), move |store| {
            store.finish_transfer(
                &transfer,
                analyte_store::TransferOutcome::Incomplete(reason),
            )
        })
        .await?;
    Ok(())
}
