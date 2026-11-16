//! Tests bounded integration test waits.
/// Upper bound so a broken test cannot hang; never part of the behaviour under test.
pub(crate) const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(60);
