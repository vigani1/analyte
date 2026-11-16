//! upkeep/startup regression tests kept beside the storage behavior they guard.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0003_rule7_startup_budget_is_inclusive_and_cancellation_is_sticky() -> TestResult {
    use crate::startup::{Startup, StartupStep};
    use std::sync::Arc;
    let base = Instant::now();
    let mut startup = Startup::new(base);
    startup.clock = Some(Arc::new(move || base));
    assert!(startup.expired());
    assert!(matches!(
        startup.check(),
        Err(Error::StartupTimeout {
            step: StartupStep::Opening
        })
    ));

    let mut startup = Startup::new(base.checked_add(HANG_GUARD).ok_or("deadline")?);
    startup.clock = Some(Arc::new(move || base));
    assert!(!startup.expired());
    startup.cancel();
    assert!(startup.expired());
    assert!(
        startup
            .run::<()>(StartupStep::Finalizing, || panic!(
                "cancelled startup must not run work"
            ))
            .is_err()
    );
    Ok(())
}
