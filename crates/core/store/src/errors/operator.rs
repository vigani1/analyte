//! Classifies operator queries, parked resolutions, alerts and notification custody failures.
storage_boundary! {
    /// Failures of protected operator operations and their audit evidence.
    #[derive(Debug, thiserror::Error)]
    pub enum OperatorError {
        /// Parked item does not exist, is already resolved or belongs to other evidence.
        #[error("parked item is missing, resolved or unrelated to this receipt")]
        ReprocessParked,
        /// Purged evidence can no longer be replayed.
        #[error("item was purged by retention; reprocessing and resending are unavailable")]
        Purged,
        /// An immutable identity or state transition conflicted.
        #[error("identity or state constraint conflict")]
        Conflict,
        /// Operator or notification input cannot authorize the requested operation.
        #[error("invalid operator input")]
        InvalidOperatorInput,
        /// An operator action or target does not match the operation being authorized.
        #[error("audit action or target mismatch")]
        AuditMismatch,
        /// A linked poll could not be reconciled with the operator resolution.
        #[error(transparent)]
        PollResolution(#[from] crate::OrderError),
    }
}
