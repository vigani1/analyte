//! Defines stored ASTM order transfer outcome codes.

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AstmEnd {
    Confirmed,
    Aborted,
    Timeout,
    Interrupted,
    Incomplete,
}
impl AstmEnd {
    /// Returns the exact historical spelling retained in the protected store.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "ASTM-transfer-confirmed",
            Self::Aborted => "ASTM-transfer-aborted",
            Self::Timeout => "ASTM-transfer-timeout",
            Self::Interrupted => "ASTM-transfer-interrupted",
            Self::Incomplete => "ASTM-transfer-incomplete",
        }
    }
}
impl std::fmt::Display for AstmEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
