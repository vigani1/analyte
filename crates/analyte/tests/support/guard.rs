//! Supplies synthetic guard test fixtures and scenarios.
/// Upper bound so a broken test cannot hang; never part of the behaviour under test.
pub(crate) const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(60);
