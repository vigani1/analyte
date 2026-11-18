//! Runs one chunk-journaled protocol host: receive loop, chunk journal, byte loop and close.
use super::{Failures, Flow, sent, wait_for};
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

/// One input the runner hands a host. A transport chunk is journaled, then
/// arrives as `Chunk` followed by one `Byte` per byte; each byte's actions run
/// before the next byte is fed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::machine) enum HostInput {
    /// The transport opened the connection.
    Opened,
    /// A journaled chunk; its bytes follow.
    Chunk,
    /// One byte of the current chunk.
    Byte(u8),
    /// A deadline was due.
    Tick,
    /// The connection closed or failed, or the service is stopping. The session
    /// ends once these actions have run.
    Disconnected,
}

/// What a host borrows for one connection, with the connection clock its dialog
/// deadlines use and the trigger of the latest transport input. The caller owns
/// every borrowed resource through shutdown.
pub(in crate::machine) struct Session<'a, 'm, S> {
    pub(in crate::machine) machine: &'a intake::ConnectionEvidence<'m>,
    pub(in crate::machine) settings: &'a S,
    pub(in crate::machine) store: &'a Handle,
    pub(in crate::machine) conn: &'a mut analyte_transport::Connection,
    pub(in crate::machine) stop: watch::Receiver<bool>,
    start: Instant,
    trigger: Trigger,
}

impl<'a, 'm, S> Session<'a, 'm, S> {
    pub(in crate::machine) fn new(
        machine: &'a intake::ConnectionEvidence<'m>,
        settings: &'a S,
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
            start: Instant::now(),
            trigger: Trigger::Exchange,
        }
    }
    /// Time on the connection clock.
    pub(in crate::machine) fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    /// Starts the connection clock again for a fresh dialog.
    pub(in crate::machine) fn restart_clock(&mut self) {
        self.start = Instant::now();
    }
    /// The trigger of the latest transport input.
    pub(in crate::machine) const fn trigger(&self) -> Trigger {
        self.trigger
    }
}

/// The actions one host call queued, and the failures that wait for them.
pub(in crate::machine) struct Queue<A, E> {
    /// Actions still to run; an effect appends the dialog's follow-up actions.
    pub(in crate::machine) actions: VecDeque<A>,
    /// Failures reported once the queue ends.
    pub(in crate::machine) failures: Failures<E>,
    stop_when_drained: bool,
}

impl<A, E> Queue<A, E> {
    /// Ends the session normally once every queued action has run.
    pub(in crate::machine) const fn stop_when_drained(&mut self) {
        self.stop_when_drained = true;
    }
}

/// A chunk-journaled protocol host: dialog transitions and effects, never its
/// own receive loop. Every call that returns `Stop` ends the session normally;
/// an error ends it with that cause. Actions a call queues run in order before
/// the runner moves on; `Stop` from an effect ends the queue at once.
pub(in crate::machine) trait Protocol: Sized {
    type Settings;
    type Action;
    type Error: CleanupFailure + From<link::error::Error> + From<analyte_transport::Error>;
    /// The next deadline on the connection clock; `None` waits for input or stop.
    fn deadline(
        &self,
        s: &Session<'_, '_, Self::Settings>,
    ) -> Result<Option<Duration>, Self::Error>;
    /// Runs before every wait for input.
    async fn idle(
        &mut self,
        _s: &mut Session<'_, '_, Self::Settings>,
        _queue: &mut Queue<Self::Action, Self::Error>,
    ) -> Result<Flow, Self::Error> {
        Ok(Flow::Continue)
    }
    async fn input(
        &mut self,
        s: &mut Session<'_, '_, Self::Settings>,
        input: HostInput,
        queue: &mut Queue<Self::Action, Self::Error>,
    ) -> Result<Flow, Self::Error>;
    /// Runs one queued action; follow-up actions and deferred failures join `queue`.
    async fn effect(
        &mut self,
        s: &mut Session<'_, '_, Self::Settings>,
        action: Self::Action,
        queue: &mut Queue<Self::Action, Self::Error>,
    ) -> Result<Flow, Self::Error>;
    /// Retains every owned effect even when another cleanup operation already failed.
    async fn close(
        &mut self,
        s: &mut Session<'_, '_, Self::Settings>,
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
    mut s: Session<'_, '_, P::Settings>,
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
async fn receive<P: Protocol>(
    host: &mut P,
    s: &mut Session<'_, '_, P::Settings>,
) -> Result<(), P::Error> {
    loop {
        if call(host, s, Call::Idle).await? == Flow::Stop {
            return Ok(());
        }
        let wait = host
            .deadline(s)?
            .map(|deadline| deadline.saturating_sub(s.elapsed()));
        let input = tokio::select! {
            biased;
            _ = async { let _ = s.stop.wait_for(|&stopped| stopped).await; } => Input::Closed(None),
            () = wait_for(wait) => Input::Tick,
            input = connection_input::receive(s.conn) => match input {
                Ok(input) => input,
                Err(analyte_transport::Error::Deadline(_)) => Input::Tick,
                Err(error @ (analyte_transport::Error::InvalidLimits
                | analyte_transport::Error::OversizedWrite
                | analyte_transport::Error::ConsumerClosed
                | analyte_transport::Error::Offline
                | analyte_transport::Error::Io { .. })) => return Err(error.into()),
            },
        };
        s.trigger = Trigger::of(&input, &s.stop);
        let input = match input {
            Input::Opened { .. } => HostInput::Opened,
            Input::Tick => HostInput::Tick,
            Input::Closed(_) | Input::Failed(_) => {
                call(host, s, Call::Input(HostInput::Disconnected)).await?;
                return Ok(());
            }
            Input::Bytes { bytes } => {
                intake::receive(
                    s.store,
                    s.machine,
                    analyte_model::ReceiptId::new(),
                    bytes.clone(),
                )
                .await
                .map_err(link::error::Error::from)?;
                if feed(host, s, bytes).await? == Flow::Stop {
                    return Ok(());
                }
                continue;
            }
        };
        if call(host, s, Call::Input(input)).await? == Flow::Stop {
            return Ok(());
        }
    }
}

/// Feeds one journaled chunk, running each byte's actions before the next byte.
/// A stop or failure leaves the rest unfed; the chunk journal keeps them.
pub(super) async fn feed<P: Protocol>(
    host: &mut P,
    s: &mut Session<'_, '_, P::Settings>,
    bytes: Vec<u8>,
) -> Result<Flow, P::Error> {
    if call(host, s, Call::Input(HostInput::Chunk)).await? == Flow::Stop {
        return Ok(Flow::Stop);
    }
    for byte in bytes {
        if call(host, s, Call::Input(HostInput::Byte(byte))).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
    }
    Ok(Flow::Continue)
}

/// The host call whose actions one queue runs.
enum Call {
    Idle,
    Input(HostInput),
}

/// Runs one host call, then its queued actions in order until none is left,
/// an effect stops or an effect fails. A kept failure outranks the flow, and an
/// error that ended the queue early is retained beside it.
async fn call<P: Protocol>(
    host: &mut P,
    s: &mut Session<'_, '_, P::Settings>,
    call: Call,
) -> Result<Flow, P::Error> {
    let mut queue = Queue {
        actions: VecDeque::new(),
        failures: Failures::new(),
        stop_when_drained: false,
    };
    let result = async {
        let flow = match call {
            Call::Idle => host.idle(s, &mut queue).await?,
            Call::Input(input) => host.input(s, input, &mut queue).await?,
        };
        if flow == Flow::Stop {
            return Ok(Flow::Stop);
        }
        while let Some(action) = queue.actions.pop_front() {
            if host.effect(s, action, &mut queue).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(if queue.stop_when_drained {
            Flow::Stop
        } else {
            Flow::Continue
        })
    }
    .await;
    queue.failures.finish(result)
}
