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

pub(super) fn effect_failure(original: Option<Error>, effect: Error) -> Error {
    match original {
        Some(original) => Box::new(EffectFailure { original, effect }),
        None => effect,
    }
}
