//! Classifies app delivery claims, attempts, listings and audited resend failures.
storage_boundary! {
    /// Failures of durable app delivery operations; never authorize resending machine orders.
    #[derive(Debug, thiserror::Error)]
    pub enum DeliveryError {
        /// An immutable identity or state transition conflicted.
        #[error("identity or state constraint conflict")]
        Conflict,
        /// Delivery construction or attempt evidence is inconsistent.
        #[error("invalid delivery input")]
        InvalidDeliveryInput,
        /// Purged evidence can no longer be replayed.
        #[error("item was purged by retention; reprocessing and resending are unavailable")]
        Purged,
        /// An operator action or target does not match the operation being authorized.
        #[error("audit action or target mismatch")]
        AuditMismatch,
        /// No delivery exists with the requested identity.
        #[error("delivery does not exist")]
        UnknownDelivery,
        /// Only completed, parked or setup-paused deliveries can be resent.
        #[error("delivery state does not permit resend")]
        ResendState,
        /// A newer observation revision exists; explicitly name the old delivery to resend it.
        #[error("delivery is superseded; explicit operator acknowledgement required")]
        SupersededDelivery,
    }
}
impl From<DeliveryError> for crate::Error {
    fn from(error: DeliveryError) -> Self {
        match error {
            DeliveryError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            DeliveryError::Capacity => Self::Capacity,
            DeliveryError::Corruption => Self::Corruption,
            DeliveryError::DiskFull(cause) => Self::DiskFull(cause),
            DeliveryError::LimitExceeded { what } => Self::LimitExceeded { what },
            DeliveryError::Busy(cause) => Self::Busy(cause),
            DeliveryError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            DeliveryError::Text(cause) => Self::Text(cause),
            DeliveryError::Io(cause) => Self::Io(cause),
            DeliveryError::Sqlite(cause) => Self::Sqlite(cause),
            DeliveryError::Json(cause) => Self::Json(cause),
            DeliveryError::Conflict => Self::Conflict,
            DeliveryError::InvalidDeliveryInput => Self::InvalidDeliveryInput,
            DeliveryError::Purged => Self::Purged,
            DeliveryError::AuditMismatch => Self::AuditMismatch,
            DeliveryError::UnknownDelivery => Self::UnknownDelivery,
            DeliveryError::ResendState => Self::ResendState,
            DeliveryError::SupersededDelivery => Self::SupersededDelivery,
        }
    }
}
