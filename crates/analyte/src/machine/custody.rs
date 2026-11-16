//! Stores machine receipts and interpretation outcomes independently of vendor dialogs.
use crate::{Error, invalid};
use std::time::Duration;

pub(super) async fn write(
    conn: &mut analyte_transport::Connection,
    bytes: &[u8],
    budget: Duration,
) -> Result<(), Error> {
    let until = tokio::time::Instant::now()
        .checked_add(budget)
        .ok_or_else(|| invalid("vendor write deadline overflow"))?;
    conn.write_all(bytes, Some(until)).await.map_err(|error| {
        if matches!(error, analyte_transport::Error::Deadline("write all")) {
            invalid("vendor write deadline exceeded; outcome uncertain")
        } else {
            error.into()
        }
    })
}

pub(super) fn ms(value: u32) -> Duration {
    Duration::from_millis(u64::from(value))
}

/// Retains both the triggering failure and a failure to save its dialog effects.
#[derive(Debug, thiserror::Error)]
#[error("{original}; dialog effect failed: {effect}")]
struct EffectFailure {
    #[source]
    original: Error,
    effect: Error,
}

pub(crate) fn effect_failure(original: Option<Error>, effect: Error) -> Error {
    match original {
        Some(original) => Box::new(EffectFailure { original, effect }),
        None => effect,
    }
}

/// Retains the triggering failure and every independent cleanup failure.
pub(crate) fn retain_cleanup(result: &mut Result<(), Error>, cleanup: Result<(), Error>) {
    if let Err(error) = cleanup {
        *result = Err(effect_failure(
            std::mem::replace(result, Ok(())).err(),
            error,
        ));
    }
}

/// Uses the validated transport operation timeout as the complete custody budget.
/// Cancelling the wait does not prove rollback: callers must refuse positive effects.
pub(crate) async fn store_within<T, E: Into<Error>>(
    budget: Duration,
    operation: impl Future<Output = Result<T, E>>,
) -> Result<T, Error> {
    tokio::time::timeout(budget, operation)
        .await
        .map_err(|_| -> Error { Box::new(StoreTimeout) })?
        .map_err(Into::into)
}

/// Marks a lost custody completion; the store may still finish the request.
#[derive(Debug, thiserror::Error)]
#[error("storage deadline exceeded; custody uncertain")]
pub(crate) struct StoreTimeout;

pub(crate) fn budget(machine: &crate::installation::config::Machine) -> Duration {
    use analyte_config::Stack;
    let stack = machine.snapshot.effective().stack();
    let operation = ms(stack.operation_timeout_ms());
    match stack {
        Stack::VidasLiteral(settings) => operation.min(ms(settings.policy().commit_ms)),
        Stack::Autolyser(settings) => operation.min(ms(settings.policy().storage_ms)),
        Stack::Astm(_) | Stack::Hl7(_) | Stack::Law(_) | Stack::Poct(_) => operation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn adr0010_store_budget_refuses_uncertain_custody() {
        let operation = std::future::pending::<Result<(), Error>>();
        let error = store_within(Duration::from_millis(17), operation)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "storage deadline exceeded; custody uncertain"
        );
    }

    #[tokio::test]
    async fn adr0010_store_reports_only_completed_custody_and_keeps_failure() {
        assert_eq!(
            store_within(Duration::from_secs(1), async { Ok::<_, Error>(42) })
                .await
                .unwrap(),
            42
        );
        let error = store_within(Duration::from_secs(1), async {
            Err::<(), _>(invalid("disk full"))
        })
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "disk full");
    }

    #[test]
    fn adr0003_cleanup_preserves_session_retention_and_settlement_errors() {
        let mut result = Err(invalid("session failure"));
        retain_cleanup(&mut result, Err(invalid("retention failure")));
        retain_cleanup(&mut result, Err(invalid("settlement failure")));
        assert_eq!(
            result.unwrap_err().to_string(),
            "session failure; dialog effect failed: retention failure; dialog effect failed: settlement failure"
        );
    }
}
