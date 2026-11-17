//! Retains the triggering failure and independent cleanup failures across service boundaries.
/// Retains both the triggering failure and a failure to save its dialog effects.
#[cfg(test)]
#[derive(Debug, thiserror::Error)]
#[error("{original}; dialog effect failed: {effect}")]
struct EffectFailure {
    #[source]
    original: crate::Error,
    effect: crate::Error,
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
