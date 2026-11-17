//! Delivers HTTP results and polls orders without analyzer I/O or clinical decisions.
#![forbid(unsafe_code)]

mod delivery;
mod destination;
#[cfg(any(test, feature = "test-hooks"))]
mod hooks;
mod polling;
mod webhook;
pub use webhook::{WebhookSender, WebhookSetupError, WebhookUrl};

pub use delivery::{DeliveryOutcome, Worker, encode_delivery, encode_order_event};
pub use destination::{Credentials, Destination, Limits, SecretsSource};
pub use polling::Poller;

use std::time::{Duration, Instant};
use tokio::sync::watch;

mod errors;
pub use errors::{DeliveryError, DestinationError, PollError};

fn deadline<E>(duration: Duration, expired: E) -> Result<Instant, E> {
    Instant::now().checked_add(duration).ok_or(expired)
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

async fn body<E: From<reqwest::Error>>(
    mut response: reqwest::Response,
    too_large: E,
) -> Result<Vec<u8>, E> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > analyte_contract::MAX_DOCUMENT_BYTES.saturating_sub(bytes.len()) {
            return Err(too_large);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;

fn http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .timeout(timeout)
        .build()
}
