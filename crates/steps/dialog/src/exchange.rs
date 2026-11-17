//! Correlates host completions with connection-local dialog exchanges.
/// Connection-scoped host request identity. The host must supply a unique epoch after restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExchangeId(pub(crate) u64, pub(crate) u64);
impl ExchangeId {
    /// Returns the host-selected connection incarnation for journaling.
    #[must_use]
    pub fn epoch(self) -> u64 {
        self.0
    }
    /// Returns the unique request ordinal within this incarnation.
    #[must_use]
    pub fn ordinal(self) -> u64 {
        self.1
    }
}
