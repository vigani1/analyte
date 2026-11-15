//! ASTM syntax code tables belong here to keep standard tokens beside their grammar.
/// Source codes retained without collapsing distinct protocol meanings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessingId {
    /// LIS02 code P.
    Production,
    /// LIS02 code T.
    Training,
    /// LIS02 code D.
    Debug,
    /// LIS02 code Q.
    QualityControl,
    /// An unrecognized, empty, or composite wire code, retained exactly.
    Other(Vec<u8>),
}
impl ProcessingId {
    /// Recognizes exact standard codes and retains every other spelling.
    #[must_use]
    pub fn from_bytes(raw: &[u8]) -> Self {
        match raw {
            b"P" => Self::Production,
            b"T" => Self::Training,
            b"D" => Self::Debug,
            b"Q" => Self::QualityControl,
            _ => Self::Other(raw.to_vec()),
        }
    }
}
/// Source codes retained without collapsing distinct protocol meanings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultStatus {
    /// LIS02 code C.
    Correction,
    /// LIS02 code P.
    Preliminary,
    /// LIS02 code F.
    Final,
    /// LIS02 code X.
    CannotPerform,
    /// LIS02 code I.
    Pending,
    /// LIS02 code S.
    Partial,
    /// LIS02 code M.
    Mic,
    /// LIS02 code R.
    PreviouslySent,
    /// LIS02 code N.
    NewOrder,
    /// LIS02 code Q.
    QueryResponse,
    /// LIS02 code V.
    Verified,
    /// LIS02 code W.
    Warning,
    /// An unrecognized, empty, or composite wire code, retained exactly.
    Other(Vec<u8>),
}
impl ResultStatus {
    /// Recognizes exact standard codes and retains every other spelling.
    #[must_use]
    pub fn from_bytes(raw: &[u8]) -> Self {
        match raw {
            b"C" => Self::Correction,
            b"P" => Self::Preliminary,
            b"F" => Self::Final,
            b"X" => Self::CannotPerform,
            b"I" => Self::Pending,
            b"S" => Self::Partial,
            b"M" => Self::Mic,
            b"R" => Self::PreviouslySent,
            b"N" => Self::NewOrder,
            b"Q" => Self::QueryResponse,
            b"V" => Self::Verified,
            b"W" => Self::Warning,
            _ => Self::Other(raw.to_vec()),
        }
    }
}
/// Source codes retained without collapsing distinct protocol meanings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminationCode {
    /// LIS02 code N.
    Normal,
    /// LIS02 code T.
    SenderAbort,
    /// LIS02 code R.
    ReceiverAbort,
    /// LIS02 code E.
    SystemError,
    /// LIS02 code Q.
    QueryError,
    /// LIS02 code I.
    NoInformation,
    /// LIS02 code F.
    Finished,
    /// An unrecognized, empty, or composite wire code, retained exactly.
    Other(Vec<u8>),
}
impl TerminationCode {
    /// Recognizes exact standard codes and retains every other spelling.
    #[must_use]
    pub fn from_bytes(raw: &[u8]) -> Self {
        match raw {
            b"" | b"N" => Self::Normal,
            b"T" => Self::SenderAbort,
            b"R" => Self::ReceiverAbort,
            b"E" => Self::SystemError,
            b"Q" => Self::QueryError,
            b"I" => Self::NoInformation,
            b"F" => Self::Finished,
            _ => Self::Other(raw.to_vec()),
        }
    }
}
