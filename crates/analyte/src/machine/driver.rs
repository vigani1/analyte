//! Owns connection receive waits, input custody and ordered protocol cleanup.
use super::{
    connection_input::{self, Input},
    receive::custody::{self, CleanupFailure},
};
use crate::intake;
use analyte_store::Handle;
use std::{collections::VecDeque, time::Duration};
use tokio::sync::watch;

/// Transport input custody follows the protocol's existing durable boundary.
#[derive(Clone, Copy)]
pub(super) enum Custody {
    /// ASTM frames and HL7/LAW messages commit through dialog effects.
    Dialog,
    /// Vendor and POCT input chunks additionally commit before dialog input.
    Chunks,
}
/// The one result shape of every driver step and every host effect.
///
/// Each host runs its dialog actions through one [`drain`] (LAW keeps its own
/// deadline-bounded loop) returning `Result<Flow, Error>`: `Stop` ends the
/// session without an error and an error ends it with that cause. A failure
/// that must wait until the dialog's own queued replies (NAK, refusal, park)
/// have run is kept in the run's one [`Failures`] and reported when the
/// run ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Flow {
    /// Keep serving this connection.
    Continue,
    /// End this session normally.
    Stop,
}

/// Holds the failures one run defers until its actions have run.
/// Nothing kept is dropped: a second failure, or an effect error that ends the
/// queue early, joins the first one as its effect.
pub(super) struct Failures<E>(Option<E>);

impl<E: CleanupFailure> Failures<E> {
    pub(super) const fn new() -> Self {
        Self(None)
    }
    /// Keeps a failure; the first one stays the original cause.
    pub(super) fn keep(&mut self, error: E) {
        self.0 = Some(custody::effect_failure(self.0.take(), error));
    }
    /// Hands a kept failure to the action that reports it inside the queue.
    pub(super) fn take(&mut self) -> Option<E> {
        self.0.take()
    }
    /// Ends one run: a kept failure outranks the flow, and an error that
    /// ended the queue early is retained beside it.
    pub(super) fn finish(self, result: Result<Flow, E>) -> Result<Flow, E> {
        match (self.0, result) {
            (Some(kept), Ok(_)) => Err(kept),
            (Some(kept), Err(effect)) => Err(E::with_effect(kept, effect)),
            (None, result) => result,
        }
    }
}

/// One [`drain`] run: the actions still to run, the failures that wait for
/// them, and the connection and stop signal its effects use.
pub(super) struct Run<'a, A, E> {
    /// Actions still to run; an effect appends the dialog's follow-up actions.
    pub actions: VecDeque<A>,
    /// Failures reported once the run ends.
    pub failures: Failures<E>,
    pub conn: &'a mut analyte_transport::Connection,
    pub stop: &'a mut watch::Receiver<bool>,
    stop_when_drained: bool,
}

impl<A, E> Run<'_, A, E> {
    /// Ends the session normally once every queued action has run.
    pub(super) const fn stop_when_drained(&mut self) {
        self.stop_when_drained = true;
    }
}

/// A host runs one dialog action at a time; [`drain`] owns the queue loop.
pub(super) trait Drain: Effects {
    type Action;
    /// Runs one action. `Stop` ends the run at once; follow-up actions and
    /// deferred failures join `run`.
    async fn effect(
        &mut self,
        action: Self::Action,
        run: &mut Run<'_, Self::Action, Self::Error>,
    ) -> Result<Flow, Self::Error>;
}

/// Runs a host's actions in order until none is left, an effect stops the run
/// or an effect fails. A kept failure outranks the flow, and an error that
/// ended the run early is retained beside it.
pub(super) async fn drain<H: Drain>(
    host: &mut H,
    actions: impl IntoIterator<Item = H::Action>,
    conn: &mut analyte_transport::Connection,
    stop: &mut watch::Receiver<bool>,
) -> Result<Flow, H::Error> {
    let mut run = Run {
        actions: actions.into_iter().collect(),
        failures: Failures::new(),
        conn,
        stop,
        stop_when_drained: false,
    };
    let result = async {
        while let Some(action) = run.actions.pop_front() {
            if host.effect(action, &mut run).await? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(if run.stop_when_drained {
            Flow::Stop
        } else {
            Flow::Continue
        })
    }
    .await;
    run.failures.finish(result)
}

/// The input a host was handling when its session ended. A vendor host sets it
/// once per input and works out its stored order outcome from it and the
/// session result when it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Trigger {
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
    pub(super) fn of(input: &Input, stop: &watch::Receiver<bool>) -> Self {
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

/// Shared host ownership; cancellation never cancels a queued durable-store request.
pub(super) struct Context<'a, 'm> {
    pub machine: &'a intake::ConnectionEvidence<'m>,
    pub store: &'a Handle,
    pub conn: &'a mut analyte_transport::Connection,
    pub stop: watch::Receiver<bool>,
}

/// A protocol supplies dialog transitions and effects, never its own receive loop.
pub(super) trait Effects {
    const CUSTODY: Custody = Custody::Dialog;
    type Error: CleanupFailure + From<super::link::error::Error> + From<analyte_transport::Error>;
    fn wait(&self) -> Result<Option<Duration>, Self::Error>;
    fn receive_error(&self, error: analyte_transport::Error) -> Result<Input, Self::Error> {
        match error {
            analyte_transport::Error::Deadline(_) => Ok(Input::Tick),
            error @ (analyte_transport::Error::InvalidLimits
            | analyte_transport::Error::OversizedWrite
            | analyte_transport::Error::ConsumerClosed
            | analyte_transport::Error::Offline
            | analyte_transport::Error::Io { .. }) => Err(error.into()),
        }
    }
    async fn prepare(&mut self, _host: &mut Context<'_, '_>) -> Result<Flow, Self::Error> {
        Ok(Flow::Continue)
    }
    /// Protocol-specific pending host work races input; dropping it never proves rollback.
    async fn background(&mut self) -> Result<(), Self::Error> {
        std::future::pending().await
    }
    async fn input(
        &mut self,
        host: &mut Context<'_, '_>,
        input: Input,
    ) -> Result<Flow, Self::Error>;
    /// Retains every owned effect even when another cleanup operation already failed.
    async fn close(
        &mut self,
        host: &mut Context<'_, '_>,
        result: &mut Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
}

/// Opens a protocol owner, drives ordered effects, and always attempts its cleanup.
/// A cancelled store or write can still have happened; protocol effects retain its identity.
/// Returns only after the session's queued replies have left the transport, so the
/// owner's close cannot discard a final refusal (ADR 0003: refuse visibly).
pub(super) async fn run<P: Effects>(
    open: impl FnOnce() -> Result<P, P::Error>,
    mut host: Context<'_, '_>,
) -> Result<(), P::Error> {
    let mut protocol = open()?;
    let mut result = drive(&mut protocol, &mut host).await;
    let cleanup = protocol.close(&mut host, &mut result).await;
    custody::retain_cleanup(&mut result, cleanup);
    custody::retain_cleanup(&mut result, sent(host.conn).await.map_err(Into::into));
    result
}

/// Waits until every write the session queued has been handed to the operating system.
/// Writes only queue bytes, and the owner cancels the transport once this session
/// returns, so without this wait a refusal written just before an error return (an HL7
/// AR or an ASTM NAK after failed storage) could be dropped unsent. An offline session
/// was ended by the transport, which reports its own reason; nothing more can be sent.
async fn sent(conn: &analyte_transport::Connection) -> Result<(), analyte_transport::Error> {
    match conn.flush().await {
        Err(analyte_transport::Error::Offline) => Ok(()),
        flushed @ (Ok(())
        | Err(
            analyte_transport::Error::InvalidLimits
            | analyte_transport::Error::OversizedWrite
            | analyte_transport::Error::ConsumerClosed
            | analyte_transport::Error::Deadline(_)
            | analyte_transport::Error::Io { .. },
        )) => flushed,
    }
}
async fn drive<P: Effects>(protocol: &mut P, host: &mut Context<'_, '_>) -> Result<(), P::Error> {
    loop {
        if matches!(protocol.prepare(host).await?, Flow::Stop) {
            return Ok(());
        }
        let wait = protocol.wait()?;
        let custody = P::CUSTODY;
        let input = tokio::select! {
            biased;
            _ = async { let _ = host.stop.wait_for(|&stopped| stopped).await; } => {
                match custody {
                    Custody::Dialog => return Ok(()),
                    Custody::Chunks => Input::Closed(None),
                }
            },
            () = wait_for(wait) => Input::Tick,
            input = connection_input::receive(host.conn) => match input {
                Ok(input) => input,
                Err(error) => protocol.receive_error(error)?,
            },
            result = protocol.background() => {
                result?;
                Input::Tick
            },
        };
        if let (Custody::Chunks, Input::Bytes { bytes }) = (custody, &input) {
            intake::receive(
                host.store,
                host.machine,
                analyte_model::ReceiptId::new(),
                bytes.clone(),
            )
            .await
            .map_err(super::link::error::Error::from)?;
        }
        if matches!(protocol.input(host, input).await?, Flow::Stop) {
            return Ok(());
        }
    }
}
/// A byte transition includes all ordered effects before another byte may be fed.
pub(super) trait ByteEffects: Effects + Send {
    fn apply_byte(
        &mut self,
        byte: u8,
        conn: &mut analyte_transport::Connection,
        stop: &mut watch::Receiver<bool>,
    ) -> impl Future<Output = Result<Flow, Self::Error>> + Send;
}

/// Completes every byte's effects before advancing a merged input chunk.
/// A stop or failure leaves subsequent bytes in the caller's retained input
/// custody; cancellation can leave the current durable operation pending.
pub(super) async fn feed<H: ByteEffects>(
    host: &mut H,
    bytes: Vec<u8>,
    conn: &mut analyte_transport::Connection,
    stop: &mut watch::Receiver<bool>,
) -> Result<Flow, H::Error> {
    for byte in bytes {
        if host.apply_byte(byte, conn, stop).await? == Flow::Stop {
            return Ok(Flow::Stop);
        }
    }
    Ok(Flow::Continue)
}

/// Supplies the common input-journal and monotonic-wait policy for chunk hosts.
/// Hosts keep their protocol transitions and effects explicit; this only shares
/// the Effects boilerplate for a dialog with a connection `start` epoch.
macro_rules! chunk_effects {
    ($error:ty) => {
        type Error = $error;
        const CUSTODY: $crate::machine::driver::Custody = $crate::machine::driver::Custody::Chunks;
        fn wait(&self) -> Result<Option<std::time::Duration>, $error> {
            Ok(self
                .dialog
                .next_deadline()
                .map(|deadline| deadline.saturating_sub(self.start.elapsed())))
        }
    };
}
pub(super) use chunk_effects;

async fn wait_for(wait: Option<Duration>) {
    match wait {
        Some(wait) if wait.is_zero() => {}
        Some(wait) => tokio::time::sleep(wait).await,
        None => std::future::pending().await,
    }
}

/// Exercises each concrete host without opening a real analyzer connection.
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, thiserror::Error)]
    enum Error {
        #[error("probe deadline")]
        Deadline,
        #[error("probe cleanup")]
        Cleanup,
        #[error("unexpected probe input")]
        UnexpectedInput,
        #[error("probe refused the message")]
        Refused,
        #[error(transparent)]
        Link(#[from] super::super::link::error::Error),
        #[error(transparent)]
        Transport(#[from] analyte_transport::Error),
        #[error(transparent)]
        CleanupPair(Box<custody::Cleanup<Self>>),
    }
    impl CleanupFailure for Error {
        fn with_effect(original: Self, effect: Self) -> Self {
            Self::CleanupPair(Box::new(custody::Cleanup { original, effect }))
        }
    }
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, Copy)]
    enum Scenario {
        Deadline,
        Stop(Custody),
    }
    struct Probe<const CHUNKS: bool> {
        trace: Rc<RefCell<Vec<&'static str>>>,
    }
    impl<const CHUNKS: bool> Effects for Probe<CHUNKS> {
        type Error = Error;
        const CUSTODY: Custody = if CHUNKS {
            Custody::Chunks
        } else {
            Custody::Dialog
        };
        fn wait(&self) -> Result<Option<Duration>, Error> {
            Ok(Some(Duration::ZERO))
        }
        async fn background(&mut self) -> Result<(), Error> {
            self.trace.borrow_mut().push("background");
            Err(Error::UnexpectedInput)
        }
        async fn input(&mut self, _: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
            match input {
                Input::Tick => {
                    self.trace.borrow_mut().push("deadline");
                    Err(Error::Deadline)
                }
                Input::Closed(None) => {
                    self.trace.borrow_mut().push("disconnect");
                    Ok(Flow::Stop)
                }
                Input::Closed(Some(_))
                | Input::Opened { .. }
                | Input::Bytes { .. }
                | Input::Failed(_) => Err(Error::UnexpectedInput),
            }
        }
        async fn close(
            &mut self,
            _: &mut Context<'_, '_>,
            _: &mut Result<(), Error>,
        ) -> Result<(), Error> {
            self.trace.borrow_mut().push("close");
            Err(Error::Cleanup)
        }
    }
    #[test]
    fn adr0012_apply_failures_outrank_stop_and_keep_every_cause() {
        let mut failures = Failures::new();
        assert!(matches!(
            Failures::<Error>::new().finish(Ok(Flow::Stop)),
            Ok(Flow::Stop)
        ));
        failures.keep(Error::Deadline);
        assert!(matches!(
            failures.finish(Ok(Flow::Stop)),
            Err(Error::Deadline)
        ));

        let mut failures = Failures::new();
        failures.keep(Error::Deadline);
        assert!(matches!(
            failures.finish(Err(Error::Cleanup)),
            Err(Error::CleanupPair(causes))
                if matches!(causes.original, Error::Deadline) && matches!(causes.effect, Error::Cleanup)
        ));

        let mut failures = Failures::new();
        failures.keep(Error::Deadline);
        failures.keep(Error::UnexpectedInput);
        assert!(matches!(
            failures.take(),
            Some(Error::CleanupPair(causes))
                if matches!(causes.original, Error::Deadline) && matches!(causes.effect, Error::UnexpectedInput)
        ));
        assert!(matches!(
            failures.finish(Ok(Flow::Continue)),
            Ok(Flow::Continue)
        ));
    }
    /// One synthetic machine and a temporary store for driving probe hosts.
    async fn synthetic(
        temp: &std::path::Path,
    ) -> Result<(crate::installation::config::Program, Handle), crate::Error> {
        let path = analyte_fixtures::write_machine(
            temp,
            include_str!("../../../core/config/examples/hl7.toml"),
            "127.0.0.1:5600",
        )?;
        let program = crate::installation::config::Program::load(&path)?;
        let store = Handle::start(
            temp.join("store.db"),
            analyte_fixtures::store_limits(),
            Box::new(analyte_fixtures::UnlimitedSpace),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
        Ok((program, store))
    }
    #[tokio::test]
    async fn adr0001_0003_stop_deadline_and_cleanup_preserve_priority_and_both_causes()
    -> Result<(), crate::Error> {
        let temp = tempfile::tempdir()?;
        let (program, mut store) = synthetic(temp.path()).await?;
        let machine = program.machines().first().ok_or("synthetic machine")?;
        let connection = intake::ConnectionEvidence::new(machine)?;
        for scenario in [
            Scenario::Deadline,
            Scenario::Stop(Custody::Dialog),
            Scenario::Stop(Custody::Chunks),
        ] {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let (mut conn, side) = analyte_transport::connection(analyte_transport::Limits::new(
                4,
                Duration::from_secs(1),
            )?);
            // EOF and background are ready as well: deadline must win, and stop must win both.
            drop(side);
            let (_signal, stop) = watch::channel(matches!(scenario, Scenario::Stop(_)));
            let host = Context {
                machine: &connection,
                store: &store,
                conn: &mut conn,
                stop,
            };
            trace.borrow_mut().push("open");
            let result = match scenario {
                Scenario::Stop(Custody::Chunks) => {
                    run(
                        || {
                            Ok(Probe::<true> {
                                trace: trace.clone(),
                            })
                        },
                        host,
                    )
                    .await
                }
                Scenario::Deadline | Scenario::Stop(Custody::Dialog) => {
                    run(
                        || {
                            Ok(Probe::<false> {
                                trace: trace.clone(),
                            })
                        },
                        host,
                    )
                    .await
                }
            };
            match scenario {
                Scenario::Deadline => {
                    assert_eq!(*trace.borrow(), ["open", "deadline", "close"]);
                    assert!(
                        matches!(result, Err(Error::CleanupPair(causes)) if matches!(causes.original, Error::Deadline) && matches!(causes.effect, Error::Cleanup))
                    );
                }
                Scenario::Stop(Custody::Dialog) => {
                    assert_eq!(*trace.borrow(), ["open", "close"]);
                    assert!(matches!(result, Err(Error::Cleanup)));
                }
                Scenario::Stop(Custody::Chunks) => {
                    assert_eq!(*trace.borrow(), ["open", "disconnect", "close"]);
                    assert!(matches!(result, Err(Error::Cleanup)));
                }
            }
        }
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }

    const REFUSAL: &[u8] = b"synthetic refusal";

    /// Queues a refusal and ends the session with an error at once: the shape of an
    /// HL7 AR or an ASTM NAK after failed storage.
    struct Refuser;
    impl Effects for Refuser {
        type Error = Error;
        fn wait(&self) -> Result<Option<Duration>, Error> {
            Ok(None)
        }
        async fn input(&mut self, host: &mut Context<'_, '_>, input: Input) -> Result<Flow, Error> {
            match input {
                Input::Opened { .. } => {
                    host.conn.write_all(REFUSAL, None).await?;
                    Err(Error::Refused)
                }
                Input::Bytes { .. } | Input::Closed(_) | Input::Failed(_) | Input::Tick => {
                    Err(Error::UnexpectedInput)
                }
            }
        }
        async fn close(
            &mut self,
            _: &mut Context<'_, '_>,
            _: &mut Result<(), Error>,
        ) -> Result<(), Error> {
            Ok(())
        }
    }

    /// Regression: a refusal queued just before an error return was dropped when the
    /// owner cancelled the transport first, and the analyzer saw only a close.
    /// The current-thread runtime makes the order deterministic: the session and the
    /// TCP pump share one task, so the owner's cancel runs before the pump next looks
    /// at its queue unless `run` waits for the queue to be sent.
    #[tokio::test(flavor = "current_thread")]
    async fn adr0003_refusal_queued_before_an_error_is_sent_before_the_owner_closes()
    -> Result<(), crate::Error> {
        use tokio::io::AsyncReadExt as _;
        let temp = tempfile::tempdir()?;
        let (program, mut store) = synthetic(temp.path()).await?;
        let machine = program.machines().first().ok_or("synthetic machine")?;
        let connection = intake::ConnectionEvidence::new(machine)?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut peer = tokio::net::TcpStream::connect(listener.local_addr()?).await?;
        let (stream, _) = listener.accept().await?;
        let (mut conn, side) = analyte_transport::connection(analyte_transport::Limits::new(
            4,
            Duration::from_secs(5),
        )?);
        let cancel = conn.cancellation();
        let (_signal, stop) = watch::channel(false);
        let session = async {
            let result = run(
                || Ok(Refuser),
                Context {
                    machine: &connection,
                    store: &store,
                    conn: &mut conn,
                    stop,
                },
            )
            .await;
            // The link owner closes the transport as soon as the session returns.
            cancel.cancel();
            result
        };
        let (result, pumped) = tokio::join!(session, analyte_transport::tcp::pump(stream, side));
        assert!(matches!(result, Err(Error::Refused)), "{result:?}");
        pumped?;
        let mut received = Vec::new();
        tokio::time::timeout(
            analyte_fixtures::HANG_GUARD,
            peer.read_to_end(&mut received),
        )
        .await??;
        assert_eq!(
            received, REFUSAL,
            "the peer must see the refusal before the close"
        );
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }
}
