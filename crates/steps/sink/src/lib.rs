//! Delivers HTTP results and polls orders without analyzer I/O or clinical decisions.
#![forbid(unsafe_code)]

mod delivery;
mod destination;
mod polling;
mod webhook;
pub use webhook::WebhookUrl;

pub use delivery::{DeliveryOutcome, Worker, build_delivery, build_order_event};
pub use destination::{Credentials, Destination, Limits, SecretsSource};
pub use polling::Poller;

use std::time::{Duration, Instant};
use tokio::sync::watch;

/// Failures retain their owning boundary and never include payloads or credentials.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid destination URL or operational limits.
    #[error("invalid destination configuration")]
    Configuration,
    /// Host could not resolve named credentials.
    #[error("named credentials unavailable")]
    Secrets,
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
    /// Storage failure; a submitted write may have committed.
    #[error("storage operation failed")]
    Store(#[from] analyte_store::Error),
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
    /// Stored destination is not configured; claimed delivery remains uncertain.
    #[error("claimed destination is not configured")]
    UnknownDestination,
}

fn deadline(duration: Duration) -> Result<Instant, Error> {
    Instant::now().checked_add(duration).ok_or(Error::Timeout)
}

async fn cancelled(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

async fn body(mut response: reqwest::Response) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > analyte_contract::MAX_DOCUMENT_BYTES.saturating_sub(bytes.len()) {
            return Err(Error::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
