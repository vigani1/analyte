//! Delivers HTTP results and polls orders without analyzer I/O or clinical decisions.
#![forbid(unsafe_code)]

mod delivery;
mod destination;
#[cfg(any(test, feature = "test-hooks"))]
mod hooks;
mod polling;
mod webhook;
pub use webhook::{WebhookSender, WebhookSetupError, WebhookUrl};

pub use delivery::{
    CONFLICT_REASON, DeliveryOutcome, UNSUPPORTED_CONTRACT_REASON, Worker, encode_delivery,
    encode_order_event, encode_review_notice,
};
pub use destination::{Credentials, Destination, Limits, SecretsSource};
pub use polling::Poller;

use std::time::{Duration, Instant};
#[cfg(test)]
use tokio::sync::watch;

mod errors;
pub use errors::{DeliveryError, DestinationError, PollError};
#[cfg(test)]
mod tests;

fn deadline<E>(duration: Duration, expired: E) -> Result<Instant, E> {
    Instant::now().checked_add(duration).ok_or(expired)
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

fn http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .timeout(timeout)
        .build()
}

/// Whether a lab app answers over HTTPS at `url` within `timeout`: one GET
/// that carries no keys, signature or data, with the same client settings and
/// certificate checks deliveries use. Any HTTP status is an answer, so the
/// address resolves, the connection opens and the certificate is valid; the
/// status is returned. It sends nothing a lab app could store.
/// # Errors
/// The client's failure: name lookup, connection, certificate or time limit.
pub async fn answers(url: &reqwest::Url, timeout: Duration) -> Result<u16, reqwest::Error> {
    let response = http_client(timeout)?.get(url.clone()).send().await?;
    Ok(response.status().as_u16())
}

#[cfg(test)]
mod stop_tests {
    use super::*;
    use std::{future::Future, task::Poll};

    // Safety rule 7 (ADR 0003)
    #[tokio::test(start_paused = true)]
    async fn stop_wait_wakes_on_true_or_owner_loss() {
        for close in [false, true] {
            let (sender, mut receiver) = watch::channel(false);
            let wait = receiver.wait_for(|&stopped| stopped);
            tokio::pin!(wait);
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(wait.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            sender.send(false).expect("receiver is alive");
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(wait.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            if close {
                drop(sender);
            } else {
                sender.send(true).expect("receiver is alive");
            }
            let _ = tokio::time::timeout(Duration::from_millis(1), wait)
                .await
                .expect("stop wakes promptly");
        }
    }
}
