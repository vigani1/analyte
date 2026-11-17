//! Defines stored order outcome codes the host writes for any protocol.

/// Stable stored codes; spellings are compatibility data, never derived from Debug.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostEnd {
    QueryReservationRefused,
    QueryDeadlineBeforeSend,
    HostRestart,
    ConnectionEndedBeforeConfirmation,
}
impl HostEnd {
    /// Returns the exact historical spelling retained in the protected store.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::QueryReservationRefused => "query-reservation-refused",
            Self::QueryDeadlineBeforeSend => "query-deadline-before-send",
            Self::HostRestart => "host-restart",
            Self::ConnectionEndedBeforeConfirmation => {
                "connection-ended-before-transfer-confirmation"
            }
        }
    }
}
impl std::fmt::Display for HostEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
