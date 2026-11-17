//! Checks the shared driver loop, deferred failures and the close-time flush.
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
        include_str!("../../../../core/config/examples/hl7.toml"),
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
    let (mut conn, side) =
        analyte_transport::connection(analyte_transport::Limits::new(4, Duration::from_secs(5))?);
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
