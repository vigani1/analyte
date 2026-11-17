//! Defines stable POCT refusal evidence spellings.
/// Closed refusal evidence retained with an incoming POCT message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefusalCode {
    /// POCT reused incoming control ID.
    ReusedControlId,
    /// POCT conversation control-ID budget exhausted.
    ControlIdBudget,
    /// POCT unsupported Keep Alive.
    KeepAlive,
    /// POCT Hello rejected by host policy.
    HelloPolicy,
    /// POCT Hello capabilities could not be interpreted.
    HelloCapabilities,
}
impl RefusalCode {
    /// Returns the historical protected-store spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReusedControlId => "POCT reused incoming control ID",
            Self::ControlIdBudget => "POCT conversation control-ID budget exhausted",
            Self::KeepAlive => "POCT unsupported Keep Alive",
            Self::HelloPolicy => "POCT Hello rejected by host policy",
            Self::HelloCapabilities => "POCT Hello capabilities could not be interpreted",
        }
    }
}
#[test]
fn park_reason_spellings_are_pinned() {
    assert_eq!(
        RefusalCode::ReusedControlId.as_str(),
        "POCT reused incoming control ID"
    );
    assert_eq!(
        RefusalCode::ControlIdBudget.as_str(),
        "POCT conversation control-ID budget exhausted"
    );
    assert_eq!(
        RefusalCode::KeepAlive.as_str(),
        "POCT unsupported Keep Alive"
    );
    assert_eq!(
        RefusalCode::HelloPolicy.as_str(),
        "POCT Hello rejected by host policy"
    );
    assert_eq!(
        RefusalCode::HelloCapabilities.as_str(),
        "POCT Hello capabilities could not be interpreted"
    );
}
