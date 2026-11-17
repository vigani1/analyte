//! Stores machine receipts and interpretation outcomes independently of vendor dialogs.
use std::time::Duration;

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

pub(crate) async fn write(
    conn: &mut analyte_transport::Connection,
    bytes: &[u8],
    budget: Duration,
) -> Result<(), WriteError> {
    let until = tokio::time::Instant::now()
        .checked_add(budget)
        .ok_or(WriteError::DeadlineOverflow)?;
    conn.write_all(bytes, Some(until)).await.map_err(|error| {
        if matches!(error, analyte_transport::Error::Deadline("write all")) {
            WriteError::Deadline(error)
        } else {
            WriteError::Transport(error)
        }
    })
}

pub(crate) fn ms(value: u32) -> Duration {
    Duration::from_millis(u64::from(value))
}

/// Retains both the triggering failure and a failure to save its dialog effects.
#[cfg(test)]
#[derive(Debug, thiserror::Error)]
#[error("{original}; dialog effect failed: {effect}")]
struct EffectFailure {
    #[source]
    original: crate::Error,
    effect: crate::Error,
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
    /// A transport operation or input journal could not complete.
    #[error(transparent)]
    Link(Box<crate::machine::link::error::Error>),
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

/// A boundary keeps its own error type when retaining independent cleanup failures.
pub(crate) trait CleanupFailure: Sized {
    fn with_effect(original: Self, effect: Self) -> Self;
}
#[cfg(test)]
impl CleanupFailure for crate::Error {
    fn with_effect(original: Self, effect: Self) -> Self {
        Box::new(EffectFailure { original, effect })
    }
}
pub(crate) fn effect_failure<E: CleanupFailure>(original: Option<E>, effect: E) -> E {
    match original {
        Some(original) => E::with_effect(original, effect),
        None => effect,
    }
}

/// Retains the triggering failure and every independent cleanup failure.
pub(crate) fn retain_cleanup<E: CleanupFailure>(
    result: &mut Result<(), E>,
    cleanup: Result<(), E>,
) {
    if let Err(error) = cleanup {
        *result = Err(effect_failure(
            std::mem::replace(result, Ok(())).err(),
            error,
        ));
    }
}

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
    use analyte_config::Stack;
    let stack = machine.snapshot.effective().stack();
    let operation = ms(stack.operation_timeout_ms());
    match stack {
        Stack::Vidas(settings) => operation.min(ms(settings.policy().commit_ms)),
        Stack::Autolyser(settings) => operation.min(ms(settings.policy().storage_ms)),
        Stack::Astm(_) | Stack::Hl7(_) | Stack::Law(_) | Stack::Poct(_) => operation,
    }
}

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
    fn vendor_write_errors_keep_recovery_kind_and_original_cause() {
        let deadline = WriteError::Deadline(analyte_transport::Error::Deadline("write all"));
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
