//! Checks the shared connection clock against ADR 0001 time ownership.
use analyte_dialog::{Clock, ClockError};
use std::time::Duration;

#[test]
fn adr0001_clock_refuses_reversal_and_overflow_without_mutation() {
    let mut clock = Clock::default();
    assert_eq!(
        clock.advance(Duration::from_secs(2), Duration::from_secs(3)),
        Ok(())
    );
    assert_eq!(
        clock.advance(Duration::from_secs(1), Duration::ZERO),
        Err(ClockError::Reversed)
    );
    assert_eq!(
        clock.advance(Duration::MAX, Duration::from_nanos(1)),
        Err(ClockError::Exhausted)
    );
    assert_eq!(clock.now(), Duration::from_secs(2));
}
