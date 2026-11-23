//! Runs one chunk-journaled protocol host: receive loop, chunk journal, byte loop and close.
use super::{Failures, Flow, sent, tick_on_deadline, wait_for};
use crate::intake;
use crate::machine::{
    connection_input::{self, Input},
    link,
    receive::custody::{self, CleanupFailure},
};
use analyte_store::Handle;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// The input a host was handling when its session ended. The runner sets it
/// once per transport input; a host works out its stored order outcome from it
/// and the session result when it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::machine) enum Trigger {
    /// The connection opened or the machine sent bytes.
    Exchange,
    /// A dialog deadline was due.
    Deadline,
    /// The connection closed or failed while the service kept running.
    Disconnected,
    /// The connection closed because the service is stopping.
    Cancelled,
}

impl Trigger {
    /// Names the trigger of one input; a closed connection during shutdown is cancelled.
    fn of(input: &Input, stop: &watch::Receiver<bool>) -> Self {
        match input {
            Input::Opened { .. } | Input::Bytes { .. } => Self::Exchange,
            Input::Tick => Self::Deadline,
            Input::Closed(_) | Input::Failed(_) => {
                if *stop.borrow() {
                    Self::Cancelled
                } else {
                    Self::Disconnected
                }
            }
        }
    }
}

/// One input the runner hands a host. A journaled transport chunk arrives as
/// `Chunk`, then one `Byte` per byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::machine) enum HostInput {
    Opened,
    Chunk,
    Byte(u8),
    /// A deadline was due.
    Tick,
    /// The connection closed or failed, or the service is stopping; the
    /// session ends once these actions have run.
    Disconnected,
}

/// What a host borrows for one connection, the connection clock its dialog
/// deadlines use, the trigger of the latest input and the actions of the
/// current step. The caller owns every borrowed resource through shutdown.
pub(in crate::machine) struct Session<'a, 'm, P: Protocol> {
    pub(in crate::machine) machine: &'a intake::ConnectionEvidence<'m>,
    pub(in crate::machine) settings: &'a P::Settings,
    pub(in crate::machine) store: &'a Handle,
    pub(in crate::machine) conn: &'a mut analyte_transport::Connection,
    pub(in crate::machine) stop: watch::Receiver<bool>,
    /// Actions the current step still runs; an effect appends follow-ups.
    pub(in crate::machine) actions: VecDeque<P::Action>,
    /// Failures reported once the current step's actions have run.
    pub(in crate::machine) failures: Failures<P::Error>,
    stop_when_drained: bool,
    start: Instant,
    trigger: Trigger,
}

impl<'a, 'm, P: Protocol> Session<'a, 'm, P> {
    pub(in crate::machine) fn new(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a P::Settings,
        store: &'a Handle,
        conn: &'a mut analyte_transport::Connection,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self {
            machine,
            settings,
            store,
            conn,
            stop,
            actions: VecDeque::new(),
            failures: Failures::new(),
            stop_when_drained: false,
            start: Instant::now(),
            trigger: Trigger::Exchange,
        }
    }
    /// Time on the connection clock.
    pub(in crate::machine) fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    /// When the connection clock started; dialog write deadlines count from it.
    pub(in crate::machine) const fn started(&self) -> Instant {
        self.start
    }
    /// Starts the connection clock again for a fresh dialog.
    pub(in crate::machine) fn restart_clock(&mut self) {
        self.start = Instant::now();
    }
    pub(in crate::machine) const fn trigger(&self) -> Trigger {
        self.trigger
    }
    /// Ends the session normally once the current step's actions have run.
    pub(in crate::machine) const fn stop_when_drained(&mut self) {
        self.stop_when_drained = true;
    }
}

/// A chunk-journaled protocol host: dialog transitions and effects, never its
/// own receive loop. `idle` runs before every wait, `input` once per input.
/// Each step's queued actions then run in order through `effect`. `Stop` from
/// any call ends the session normally and an error ends it with that cause.
pub(in crate::machine) trait Protocol: Sized {
    type Settings;
    type Action;
    type Error: CleanupFailure + From<link::error::Error> + From<analyte_transport::Error>;
    /// The next deadline on the connection clock; `None` waits for input or stop.
    fn deadline(&self, s: &Session<'_, '_, Self>) -> Result<Option<Duration>, Self::Error>;
    async fn idle(&mut self, _s: &mut Session<'_, '_, Self>) -> Result<Flow, Self::Error> {
        Ok(Flow::Continue)
    }
    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        input: HostInput,
    ) -> Result<Flow, Self::Error>;
    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        action: Self::Action,
    ) -> Result<Flow, Self::Error>;
    /// Retains every owned effect even when another cleanup operation already failed.
    async fn close(
        &mut self,
        s: &mut Session<'_, '_, Self>,
        result: &mut Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
}

/// Opens a host, serves its connection and always attempts its cleanup. The
/// connection clock starts when `open` returns; a failed open ends the session
/// before any input, without cleanup. Returns only after the session's queued
/// replies have left the transport (ADR 0003: refuse visibly). A cancelled
/// store or write can still have happened; the host's effects retain its identity.
pub(in crate::machine) async fn serve<P: Protocol>(
    open: impl Future<Output = Result<P, P::Error>>,
    mut s: Session<'_, '_, P>,
) -> Result<(), P::Error> {
    let mut host = open.await?;
    s.restart_clock();
    let mut result = receive(&mut host, &mut s).await;
    let cleanup = host.close(&mut s, &mut result).await;
    custody::retain_cleanup(&mut result, cleanup);
    custody::retain_cleanup(&mut result, sent(s.conn).await.map_err(Into::into));
    result
}

/// Waits for the next input and hands it to the host. A stop outranks a due
/// deadline and waiting input, and reaches the host as a disconnect so its
/// dialog parks what is open. Every chunk is durably journaled before the
/// dialog sees its first byte (ADR 0010).
async fn receive<P: Protocol>(host: &mut P, s: &mut Session<'_, '_, P>) -> Result<(), P::Error> {
    loop {
        if step(host, s, None).await? == Flow::Stop {
            return Ok(());
        }
        let wait = host.deadline(s)?.map(|at| at.saturating_sub(s.elapsed()));
        let input = tokio::select! {
            biased;
            _ = async { let _ = s.stop.wait_for(|&stopped| stopped).await; } => Input::Closed(None),
            () = wait_for(wait) => Input::Tick,
            input = connection_input::receive(s.conn) => match input {
                Ok(input) => input,
                Err(error) => tick_on_deadline(error)?,
            },
        };
        s.trigger = Trigger::of(&input, &s.stop);
        let flow = match input {
            Input::Opened { .. } => step(host, s, Some(HostInput::Opened)).await?,
            Input::Tick => step(host, s, Some(HostInput::Tick)).await?,
            Input::Closed(_) | Input::Failed(_) => {
                step(host, s, Some(HostInput::Disconnected)).await?;
                Flow::Stop
            }
            Input::Bytes { bytes } => {
                let id = analyte_model::ReceiptId::new();
                intake::receive(s.store, s.machine, id, bytes.clone())
                    .await
                    .map_err(link::error::Error::from)?;
                feed(host, s, bytes).await?
            }
        };
        if flow == Flow::Stop {
            return Ok(());
        }
    }
}

/// Feeds one journaled chunk, running each byte's actions before the next byte.
/// A stop or failure leaves the rest unfed; the chunk journal keeps them.
pub(super) async fn feed<P: Protocol>(
    host: &mut P,
    s: &mut Session<'_, '_, P>,
    bytes: Vec<u8>,
) -> Result<Flow, P::Error> {
    let inputs = std::iter::once(HostInput::Chunk).chain(bytes.into_iter().map(HostInput::Byte));
    for input in inputs {
        if step(host, s, Some(input)).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
    }
    Ok(Flow::Continue)
}

/// Runs `idle` (no input) or `input`, then the actions it queued in order until
/// none is left, an effect stops or an effect fails. A kept failure outranks
/// the flow, and an error that ended the step early is retained beside it.
async fn step<P: Protocol>(
    host: &mut P,
    s: &mut Session<'_, '_, P>,
    input: Option<HostInput>,
) -> Result<Flow, P::Error> {
    s.actions.clear();
    s.stop_when_drained = false;
    let result = async {
        let flow = match input {
            None => host.idle(s).await?,
            Some(input) => host.input(s, input).await?,
        };
        if flow == Flow::Stop {
            return Ok(Flow::Stop);
        }
        while let Some(action) = s.actions.pop_front() {
            if host.effect(s, action).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(if s.stop_when_drained {
            Flow::Stop
        } else {
            Flow::Continue
        })
    }
    .await;
    std::mem::replace(&mut s.failures, Failures::new()).finish(result)
}
