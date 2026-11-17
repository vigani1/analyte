//! Supplies the observation edit guard for independent POCT readings.
use crate::machine::interpretation::policy::Policy;
pub(crate) fn policy() -> Policy {
    Policy::independent(
        "edited POCT observation with no detectable common-content change; source context may have changed",
    )
}
