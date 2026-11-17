//! Stores machine receipts and interpretation outcomes and writes vendor bytes for every dialog.
use std::time::{Duration, Instant};

/// Distinguishes an unstarted write, an uncertain deadline and a transport failure.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WriteError {
    #[error("vendor write deadline overflow")]
    DeadlineOverflow,
    #[error("vendor write deadline exceeded; outcome uncertain")]
    Deadline(#[source] analyte_transport::Error),
    #[error(transparent)]
    Transport(#[from] analyte_transport::Error),
}

/// When a vendor write must be fully queued. Only an expired overall deadline leaves
/// the outcome uncertain; a per-chunk queue deadline is an ordinary transport failure.
#[derive(Debug, Clone, Copy)]
pub(crate) enum WriteDeadline {
    /// No overall deadline; the transport's per-chunk queue budget applies.
    Transport,
    /// An overall budget that starts with the write.
    Within(Duration),
    /// An overall absolute deadline.
    At(Instant),
}
impl WriteDeadline {
    /// A dialog's `Write` deadline on the connection clock that began at `start`;
    /// none means the transport budget.
    pub(crate) fn dialog(start: Instant, until: Option<Duration>) -> Result<Self, WriteError> {
        until.map_or(Ok(Self::Transport), |until| {
            start
                .checked_add(until)
                .map(Self::At)
                .ok_or(WriteError::DeadlineOverflow)
        })
    }
}

/// The one path by which a protocol host writes to its connection. Success means
/// queued, not acknowledged; a failed or uncertain write is never retried here.
pub(crate) async fn write(
    conn: &mut analyte_transport::Connection,
    bytes: &[u8],
    deadline: WriteDeadline,
) -> Result<(), WriteError> {
    let until = match deadline {
        WriteDeadline::Transport => None,
        WriteDeadline::Within(budget) => Some(
            tokio::time::Instant::now()
                .checked_add(budget)
                .ok_or(WriteError::DeadlineOverflow)?,
        ),
        WriteDeadline::At(until) => Some(tokio::time::Instant::from_std(until)),
    };
    conn.write_all(bytes, until).await.map_err(classify_write)
}

fn classify_write(error: analyte_transport::Error) -> WriteError {
    match error {
        analyte_transport::Error::Deadline(analyte_transport::DeadlineKind::WriteAll) => {
            WriteError::Deadline(error)
        }
        error @ (analyte_transport::Error::InvalidLimits
        | analyte_transport::Error::OversizedWrite
        | analyte_transport::Error::ConsumerClosed
        | analyte_transport::Error::Offline
        | analyte_transport::Error::Deadline(_)
        | analyte_transport::Error::Io { .. }) => WriteError::Transport(error),
    }
}

pub(crate) fn ms(value: u32) -> Duration {
    Duration::from_millis(u64::from(value))
}

/// Keeps a triggering failure and a later independent cleanup failure in order.
#[derive(Debug, thiserror::Error)]
#[error("{original}; dialog effect failed: {effect}")]
pub(crate) struct Cleanup<E> {
    /// Failure that caused the session to refuse further positive effects.
    #[source]
    pub(crate) original: E,
    /// Secondary failure while retaining evidence or settling owned work.
    pub(crate) effect: E,
}

/// Shared causes that require a protocol owner to refuse positive effects.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Refusal<E: std::error::Error + 'static> {
    /// A durable-store or interpretation operation failed.
    #[error(transparent)]
    Evidence(Box<crate::machine::receive::error::Error>),
    /// Both the original failure and the attempted cleanup failed.
    #[error(transparent)]
    Cleanup(Box<Cleanup<E>>),
    /// A custody operation failed or its completion remains uncertain.
    #[error(transparent)]
    Custody(Box<StoreWaitError<E>>),
}

pub(crate) use crate::cleanup::{CleanupFailure, effect_failure, retain_cleanup};

/// Uses the validated transport operation timeout as the complete custody budget.
/// Cancelling the wait does not prove rollback: callers must refuse positive effects.
pub(crate) async fn store_within<T, E>(
    budget: Duration,
    operation: impl Future<Output = Result<T, E>>,
) -> Result<T, StoreWaitError<E>> {
    tokio::time::timeout(budget, operation)
        .await
        .map_err(StoreWaitError::Timeout)?
        .map_err(StoreWaitError::Operation)
}

/// Commits custody before a driver may emit a positive acknowledgment.
/// Failure retains its cause and is visible without logging machine payloads.
/// Cancellation leaves custody uncertain; callers must refuse positive effects.
pub(crate) async fn store_before_ack<T, E: std::fmt::Display>(
    machine: &crate::installation::config::Machine,
    operation: impl Future<Output = Result<T, E>>,
) -> Result<T, StoreWaitError<E>> {
    let result = store_within(budget(machine), operation).await;
    if let Err(error) = &result {
        tracing::error!(machine = machine.id.as_str(), error_kind = %error, "durable custody refused");
    }
    result
}

/// A lost completion is distinct from an operation's typed failure.
/// Timeout never proves rollback; the queued store request may still finish.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StoreWaitError<E> {
    #[error("storage deadline exceeded; custody uncertain")]
    Timeout(#[source] tokio::time::error::Elapsed),
    #[error(transparent)]
    Operation(E),
}

pub(crate) fn budget(machine: &crate::installation::config::Machine) -> Duration {
    ms(machine.snapshot.effective().stack().commit_budget_ms())
}

/// Gives one protocol host error, which must have a `Refusal(Refusal<Self>)` variant,
/// the shared custody conversions: link and evidence causes, uncertain store waits and
/// independent cleanup failures. One definition keeps every host's refusal identical.
macro_rules! refusal_conversions {
    ($target:ty) => {
        impl<C> From<C> for $target
        where
            $crate::machine::receive::error::Error: From<C>,
        {
            fn from(cause: C) -> Self {
                Self::Refusal($crate::machine::receive::custody::Refusal::Evidence(
                    Box::new($crate::machine::receive::error::Error::from(cause)),
                ))
            }
        }
        impl From<$crate::machine::receive::custody::StoreWaitError<Self>> for $target {
            fn from(cause: $crate::machine::receive::custody::StoreWaitError<Self>) -> Self {
                Self::Refusal($crate::machine::receive::custody::Refusal::Custody(
                    Box::new(cause),
                ))
            }
        }
        impl $crate::machine::receive::custody::CleanupFailure for $target {
            fn with_effect(original: Self, effect: Self) -> Self {
                Self::Refusal($crate::machine::receive::custody::Refusal::Cleanup(
                    Box::new($crate::machine::receive::custody::Cleanup { original, effect }),
                ))
            }
        }
    };
}
pub(crate) use refusal_conversions;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[derive(Debug, thiserror::Error)]
    #[error("disk full")]
    struct DiskFull;

    #[tokio::test]
    async fn adr0010_all_protocols_refuse_failed_custody_with_original_cause()
    -> Result<(), crate::Error> {
        let temp = tempfile::tempdir()?;
        std::fs::write(
            temp.path().join("machine.toml"),
            include_str!("../../../../core/config/examples/hl7.toml"),
        )?;
        std::fs::write(
            temp.path().join("site.toml"),
            include_str!("../../../../core/config/examples/site.toml"),
        )?;
        let path = temp.path().join("program.toml");
        std::fs::write(
            &path,
            analyte_fixtures::program(
                "[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n",
            ),
        )?;
        let program = crate::installation::config::Program::load(&path)?;
        let machine = program.machines.first().ok_or("machine missing")?;
        let error = store_before_ack(machine, async { Err::<(), _>(DiskFull) })
            .await
            .unwrap_err();
        assert!(matches!(error, StoreWaitError::Operation(DiskFull)));
        assert_eq!(error.to_string(), "disk full");
        let custody = crate::machine::poct::Error::from(StoreWaitError::Operation(
            crate::IntakeError::UnknownReceipt,
        ));
        let mut result = Err(custody);
        retain_cleanup(
            &mut result,
            Err(crate::machine::poct::Error::BackpressureViolated),
        );
        let error = result.unwrap_err();
        assert!(
            matches!(&error, crate::machine::poct::Error::Refusal(Refusal::Cleanup(failure)) if matches!(&failure.original, crate::machine::poct::Error::Refusal(Refusal::Evidence(error)) if matches!(error.as_ref(), crate::machine::receive::error::Error::IntakeWait(StoreWaitError::Operation(_)))))
        );
        assert!(error.source().is_some());
        Ok(())
    }

    #[test]
    fn only_write_all_deadlines_have_uncertain_vendor_write_outcomes() {
        use analyte_transport::{DeadlineKind, Error};
        for kind in [
            DeadlineKind::Connect,
            DeadlineKind::Bind,
            DeadlineKind::TcpRead,
            DeadlineKind::TcpWrite,
            DeadlineKind::ServerNotification,
            DeadlineKind::ServerShutdown,
            DeadlineKind::SerialBackpressure,
            DeadlineKind::SerialWrite,
            DeadlineKind::WriteAll,
            DeadlineKind::Receive,
            DeadlineKind::QueueWrite,
            DeadlineKind::EventBackpressure,
            DeadlineKind::Flush,
        ] {
            assert_eq!(
                matches!(
                    classify_write(Error::Deadline(kind)),
                    WriteError::Deadline(_)
                ),
                kind == DeadlineKind::WriteAll
            );
        }
    }

    #[test]
    fn dialog_write_deadlines_use_the_connection_clock_or_the_transport_budget() {
        let start = Instant::now();
        assert!(matches!(
            WriteDeadline::dialog(start, None),
            Ok(WriteDeadline::Transport)
        ));
        assert!(matches!(
            WriteDeadline::dialog(start, Some(Duration::from_millis(1_500))),
            Ok(WriteDeadline::At(until)) if Some(until) == start.checked_add(Duration::from_millis(1_500))
        ));
        assert!(matches!(
            WriteDeadline::dialog(start, Some(Duration::MAX)),
            Err(WriteError::DeadlineOverflow)
        ));
    }

    #[test]
    fn vendor_write_errors_keep_recovery_kind_and_original_cause() {
        let deadline = WriteError::Deadline(analyte_transport::Error::Deadline(
            analyte_transport::DeadlineKind::WriteAll,
        ));
        assert_eq!(
            deadline.to_string(),
            "vendor write deadline exceeded; outcome uncertain"
        );
        assert!(deadline.source().unwrap().is::<analyte_transport::Error>());
        assert_eq!(
            WriteError::DeadlineOverflow.to_string(),
            "vendor write deadline overflow"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn adr0010_store_budget_refuses_uncertain_custody() {
        let operation = std::future::pending::<Result<(), DiskFull>>();
        let error = store_within(Duration::from_millis(17), operation)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "storage deadline exceeded; custody uncertain"
        );
        assert!(matches!(&error, StoreWaitError::Timeout(_)));
        let cause = error
            .source()
            .expect("a custody timeout retains its elapsed cause");
        assert!(cause.is::<tokio::time::error::Elapsed>());
    }

    #[tokio::test]
    async fn adr0010_store_reports_only_completed_custody_and_keeps_failure() {
        assert_eq!(
            store_within(Duration::from_secs(1), async { Ok::<_, DiskFull>(42) })
                .await
                .unwrap(),
            42
        );
        let error = store_within(Duration::from_secs(1), async { Err::<(), _>(DiskFull) })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "disk full");
        assert!(matches!(error, StoreWaitError::Operation(DiskFull)));
    }

    #[test]
    fn adr0003_cleanup_preserves_session_retention_and_settlement_errors() {
        let mut result = Err(crate::invalid("session failure"));
        retain_cleanup(&mut result, Err(crate::invalid("retention failure")));
        retain_cleanup(&mut result, Err(crate::invalid("settlement failure")));
        assert_eq!(
            result.unwrap_err().to_string(),
            "session failure; dialog effect failed: retention failure; dialog effect failed: settlement failure"
        );
    }
}
