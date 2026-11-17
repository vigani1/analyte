//! Distinguishes receipt and transfer refusals from persistence failures.
storage_boundary! {
    /// Failures admitting raw receipts and transfer frames before acknowledgment.
    #[derive(Debug, thiserror::Error)]
    pub enum IntakeError {
        /// Receipt digests or protocol-supplied frame bounds do not agree.
        #[error("invalid receipt input")]
        InvalidReceiptInput,
        /// Transfer state or immutable provenance conflicts with retained evidence.
        #[error("identity or state constraint conflict")]
        Conflict,
    }
}
