//! Classifies cached orders, machine reservations, lifecycle changes and poll custody failures.
storage_boundary! {
    /// Failures of order storage and host event construction; retries never authorize another send.
    #[derive(Debug, thiserror::Error)]
    pub enum OrderError {
        /// A host event builder supplied an invalid delivery identity.
        #[error(transparent)]
        Identity(#[from] analyte_model::InvalidUuid),
        /// The host invoked a single-use rendering callback more than once.
        #[error("order preflight callback already consumed")]
        PreflightConsumed,
        /// Reservation completed without the required rendered command.
        #[error("order preflight output missing")]
        PreflightMissing,
        /// An immutable identity or state transition conflicted.
        #[error("identity or state constraint conflict")]
        Conflict,
        /// Machine-order input violates its declared ownership or item structure.
        #[error("invalid machine-order input")]
        InvalidOrderInput,
        /// Purged evidence can no longer be replayed.
        #[error("item was purged by retention; reprocessing and resending are unavailable")]
        Purged,
        /// An operator action or target does not match the operation being authorized.
        #[error("audit action or target mismatch")]
        AuditMismatch,
        /// Analyzer cancellation requires visible reconciliation before any new cancel admission.
        #[error("AWOS is analyzer-cancelled; app cancellation intent requires reconciliation")]
        AnalyzerCancelled,
        /// The model rejected a machine-order lifecycle transition.
        #[error("machine-order lifecycle transition rejected: {0}")]
        OrderTransition(#[from] analyte_model::TransitionError),
        /// A previous poll may already have moved this machine's cursor.
        #[error("poll intent remains unresolved; reconcile before polling again")]
        PollIntentUnresolved,
        /// No retained poll has the requested identity.
        #[error("poll intent does not exist")]
        UnknownPollIntent,
        /// The generated local command identity exceeds the caller's byte budget.
        #[error("command identity exceeds caller byte limit")]
        ItemIdentityTooLong,
        /// A finite policy requires the original host credential.
        #[error("retention digest key is required")]
        RetentionKeyRequired,
        /// The host's transactional delivery builder refused construction; the cause is retained.
        #[error("delivery construction failed")]
        DeliveryBuild(#[source] Box<dyn std::error::Error + Send + Sync>),
        /// The keyed retention digest could not be constructed.
        #[error("retention digest key rejected")]
        DigestKey(#[from] hmac::digest::InvalidLength),
    }
}
impl From<OrderError> for crate::Error {
    fn from(error: OrderError) -> Self {
        match error {
            OrderError::Worker(cause) => Self::Worker(cause),
            OrderError::Identity(cause) => Self::DeliveryBuild(Box::new(cause)),
            OrderError::PreflightConsumed => Self::PreflightConsumed,
            OrderError::PreflightMissing => Self::PreflightMissing,
            OrderError::StorageCorruption(cause) => Self::StorageCorruption(cause),
            OrderError::Capacity => Self::Capacity,
            OrderError::Corruption => Self::Corruption,
            OrderError::DiskFull(cause) => Self::DiskFull(cause),
            OrderError::LimitExceeded { what } => Self::LimitExceeded { what },
            OrderError::Busy(cause) => Self::Busy(cause),
            OrderError::ConstraintViolation(cause) => Self::ConstraintViolation(cause),
            OrderError::Text(cause) => Self::Text(cause),
            OrderError::Io(cause) => Self::Io(cause),
            OrderError::Sqlite(cause) => Self::Sqlite(cause),
            OrderError::Json(cause) => Self::Json(cause),
            OrderError::Conflict => Self::Conflict,
            OrderError::InvalidOrderInput => Self::InvalidOrderInput,
            OrderError::Purged => Self::Purged,
            OrderError::AuditMismatch => Self::AuditMismatch,
            OrderError::AnalyzerCancelled => Self::AnalyzerCancelled,
            OrderError::OrderTransition(cause) => Self::OrderTransition(cause),
            OrderError::PollIntentUnresolved => Self::PollIntentUnresolved,
            OrderError::UnknownPollIntent => Self::UnknownPollIntent,
            OrderError::ItemIdentityTooLong => Self::ItemIdentityTooLong,
            OrderError::RetentionKeyRequired => Self::RetentionKeyRequired,
            OrderError::DeliveryBuild(cause) => Self::DeliveryBuild(cause),
            OrderError::DigestKey(cause) => Self::DigestKey(cause),
        }
    }
}
