//! Owns connection receive waits, input custody and ordered protocol cleanup.
use super::{
    connection_input::{self, Input},
    receive::custody::{self, CleanupFailure},
};
use crate::intake;
use analyte_store::Handle;
use std::time::Duration;
use tokio::sync::watch;

/// Transport input custody follows the protocol's existing durable boundary.
#[derive(Clone, Copy)]
pub(super) enum Custody {
    /// ASTM frames and HL7/LAW messages commit through dialog effects.
    Dialog,
    /// Vendor and POCT input chunks additionally commit before dialog input.
    Chunks,
}
pub(super) enum Flow {
    Continue,
    Stop,
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
pub(super) async fn run<P: Effects>(
    open: impl FnOnce() -> Result<P, P::Error>,
    mut host: Context<'_, '_>,
) -> Result<(), P::Error> {
    let mut protocol = open()?;
    let mut result = drive(&mut protocol, &mut host).await;
    let cleanup = protocol.close(&mut host, &mut result).await;
    custody::retain_cleanup(&mut result, cleanup);
    result
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
            () = crate::runtime::stopped(&mut host.stop) => {
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
        stop: &watch::Receiver<bool>,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Completes every byte's effects before advancing a merged input chunk.
/// A stop or failure leaves subsequent bytes in the caller's retained input
/// custody; cancellation can leave the current durable operation pending.
pub(super) async fn feed<H: ByteEffects>(
    host: &mut H,
    bytes: Vec<u8>,
    conn: &mut analyte_transport::Connection,
    stop: &watch::Receiver<bool>,
) -> Result<bool, H::Error> {
    for byte in bytes {
        if host.apply_byte(byte, conn, stop).await? {
            return Ok(true);
        }
    }
    Ok(false)
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
    #[tokio::test]
    async fn adr0001_0003_stop_deadline_and_cleanup_preserve_priority_and_both_causes()
    -> Result<(), crate::Error> {
        let temp = tempfile::tempdir()?;
        let path = analyte_fixtures::write_machine(
            temp.path(),
            include_str!("../../../core/config/examples/hl7.toml"),
            "127.0.0.1:5600",
        )?;
        let program = crate::installation::config::Program::load(&path)?;
        let machine = program.machines().first().ok_or("synthetic machine")?;
        let connection = intake::ConnectionEvidence::new(machine)?;
        let mut store = Handle::start(
            temp.path().join("store.db"),
            analyte_fixtures::store_limits(),
            Box::new(analyte_fixtures::UnlimitedSpace),
            4,
            crate::test_startup_deadline()?,
        )
        .await?;
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
}
