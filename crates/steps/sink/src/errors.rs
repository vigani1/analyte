//! Separates destination setup, immutable delivery and order polling failures.

/// Failures configuring an endpoint or resolving its current credentials.
#[derive(Debug, thiserror::Error)]
pub enum DestinationError {
    /// Invalid destination URL or operational limits.
    #[error("invalid destination configuration")]
    Configuration,
    /// URL violates the shared endpoint policy; retains the precise rejection reason.
    #[error("destination URL policy rejected: {0}")]
    UrlPolicy(&'static str),
    /// URL joining failed; the parser cause remains available without logging the URL.
    #[error("destination URL path rejected")]
    UrlJoin(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// Host could not resolve named credentials.
    #[error("named credentials unavailable")]
    Secrets,
    /// Signing metadata or secret rejected.
    #[error("signing failed")]
    Signature(#[from] analyte_contract::SignatureError),
    /// Network failure; an app may already have committed the delivery.
    #[error("HTTP operation failed")]
    Http(#[from] reqwest::Error),
}

/// Failures constructing, sending or durably recording an app delivery.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    /// Delivery identity is not an RFC `UUIDv7`.
    #[error("invalid delivery identity")]
    Identity(#[from] analyte_model::InvalidUuid),
    /// Delivery identity text cannot be decoded as a UUID.
    #[error("invalid delivery UUID text")]
    Uuid(#[from] uuid::Error),
    /// Order identity exceeds the store text bounds.
    #[error("invalid order identity")]
    OrderId(#[from] analyte_model::TextError),
    /// Retry duration cannot fit the timestamp representation.
    #[error("retry duration exceeds timestamp range")]
    Integer(#[from] std::num::TryFromIntError),
    /// Retry timestamp addition exceeded its representable range.
    #[error("retry timestamp overflow: {now} + {delay}")]
    RetryOverflow {
        /// Unix timestamp before scheduling the retry.
        now: i64,
        /// Requested retry delay in seconds.
        delay: i64,
    },
    /// Outgoing immutable bytes exceed the contract ceiling.
    #[error("outgoing payload exceeds document limit")]
    PayloadTooLarge,
    /// Non-production processing cannot enter the production outbox.
    #[error("processing mode is not eligible for production delivery")]
    ProcessingMode,
    /// Unresolved or unsupported evidence needs operator reconciliation.
    #[error("result requires parking")]
    RequiresParking,
    /// Invalid immutable document.
    #[error("contract document rejected")]
    Document(#[from] analyte_contract::DocumentError),
    /// Document serialization failed.
    #[error("document encoding failed")]
    Json(#[from] serde_json::Error),
    /// Signing metadata or secret rejected.
    #[error("signing failed")]
    Signature(#[from] analyte_contract::SignatureError),
    /// Network failure; an app may already have committed the delivery.
    #[error("HTTP operation failed")]
    Http(#[from] reqwest::Error),
    /// Deadline expired; a sent request may have committed.
    #[error("operation deadline expired")]
    Timeout,
    /// Peer exceeded the document byte limit.
    #[error("response exceeds document limit")]
    ResponseTooLarge,
    /// Stored destination is not configured; claimed delivery remains uncertain.
    #[error("claimed destination is not configured")]
    UnknownDestination,
    /// Endpoint setup failed; retains the setup boundary cause.
    #[error(transparent)]
    Destination(#[from] DestinationError),
    /// Storage failed; a submitted write may have committed.
    #[error(transparent)]
    Store(#[from] analyte_store::DeliveryError),
}

/// Failures fetching or durably applying an order feed.
#[derive(Debug, thiserror::Error)]
pub enum PollError {
    /// Reading the durable order cursor failed.
    #[error(transparent)]
    Cursor(#[from] analyte_store::OperatorError),
    /// Invalid immutable document.
    #[error("contract document rejected")]
    Document(#[from] analyte_contract::DocumentError),
    /// Document serialization failed.
    #[error("document encoding failed")]
    Json(#[from] serde_json::Error),
    /// Network failure; an app may already have committed the delivery.
    #[error("HTTP operation failed")]
    Http(#[from] reqwest::Error),
    /// Deadline expired; a sent request may have committed.
    #[error("operation deadline expired")]
    Timeout,
    /// Peer exceeded the document byte limit.
    #[error("response exceeds document limit")]
    ResponseTooLarge,
    /// Order endpoint returned a non-success status.
    #[error("order poll returned HTTP {0}")]
    PollStatus(u16),
    /// Page count or cursor progress violated polling bounds.
    #[error("order feed did not complete within its bounds")]
    FeedProgress,
    /// Endpoint setup failed; retains the setup boundary cause.
    #[error(transparent)]
    Destination(#[from] DestinationError),
    /// Storage failed; a submitted write may have committed.
    #[error(transparent)]
    Store(#[from] analyte_store::OrderError),
}
