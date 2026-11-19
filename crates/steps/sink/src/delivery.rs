//! Delivers results while preserving delivery identities across retries.
use crate::{DeliveryError, Destination, SecretsSource, body, deadline};
use analyte_contract::{ResponseAction, classify_response};
use analyte_model::{DeliveryId, Identifier, IdentityStatus, ReceiptMetadata, ResultContent};
use analyte_store::{Attempt, DeliveryKind, Handle};
use std::time::Duration;
/// Bounds idle outbox query frequency; cancellation wakes immediately.
const IDLE_CLAIM_INTERVAL: Duration = Duration::from_millis(100);

use tokio::sync::watch;

/// Builds immutable production bytes and creates their delivery ID exactly once.
/// The caller commits this with its interpretation using `Store::interpret_reconciled`.
/// # Errors
/// Refuses non-production modes, parked evidence, receipt mismatch and wire errors.
pub fn encode_delivery(
    result: &ResultContent,
    receipt: &ReceiptMetadata,
    destination: Identifier,
    created_at: jiff::Timestamp,
) -> Result<analyte_store::Delivery, DeliveryError> {
    if !result.processing.may_deliver_to_production() {
        return Err(DeliveryError::ProcessingMode);
    }
    if result.requires_parking() {
        return Err(DeliveryError::RequiresParking);
    }
    let IdentityStatus::Resolved(identity) = &result.identity else {
        return Err(DeliveryError::RequiresParking);
    };
    let id = DeliveryId::try_from(uuid::Uuid::now_v7()).map_err(DeliveryError::Identity)?;
    let document = analyte_contract::Delivery::from_result(result, receipt, id, created_at)?;
    Ok(analyte_store::Delivery {
        id,
        destination,
        ordering_key: analyte_store::OrderingKey {
            machine: receipt.machine.clone(),
            observation: identity.observation.clone(),
        },
        payload: serde_json::to_vec(&document)?,
        contract_version: 1,
    })
}

/// Freezes a validated order event for atomic enqueue. Its supplied ID is retained.
/// # Errors
/// Refuses oversized bytes or an order identity outside the store's identifier bounds.
pub fn encode_order_event(
    event: &analyte_contract::OrderEvent,
    destination: Identifier,
) -> Result<analyte_store::OrderEventDelivery, DeliveryError> {
    let payload = serde_json::to_vec(event)?;
    if payload.len() > analyte_contract::MAX_DOCUMENT_BYTES {
        return Err(DeliveryError::PayloadTooLarge);
    }
    let fields = event.fields();
    let id = DeliveryId::try_from(uuid::Uuid::parse_str(fields.delivery_id.as_str())?)?;
    let order_id = Identifier::new(fields.order_id.as_str())?;
    Ok(analyte_store::OrderEventDelivery {
        id,
        destination,
        order_id,
        payload,
        contract_version: 1,
    })
}

/// Outcome durably recorded before returning to the host.
#[derive(Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// No eligible delivery is currently due.
    Idle,
    /// Accepted or deduplicated by the app.
    Done(analyte_contract::DeliveryStatus),
    /// Preserved for operator reconciliation.
    Parked,
    /// Destination paused until an audited operator release.
    Paused,
    /// Same immutable delivery scheduled for a later attempt.
    RetryAt(i64),
}

/// Claims and delivers in order; the host owns scheduling and cancellation.
/// Use one `for_destination` worker per destination to isolate slow HTTP peers.
pub struct Worker<'a> {
    store: &'a Handle,
    destination: &'a Destination,
    secrets: &'a dyn SecretsSource,
}

impl<'a> Worker<'a> {
    /// Restricts claims to one destination; other owners cannot steal its deliveries.
    #[must_use]
    pub fn for_destination(
        store: &'a Handle,
        destination: &'a Destination,
        secrets: &'a dyn SecretsSource,
    ) -> Self {
        Self {
            store,
            destination,
            secrets,
        }
    }

    /// Claims, sends and records one attempt. Cancellation can leave it in flight;
    /// the host must reconcile and explicitly release that same ID after restart.
    /// # Errors
    /// Returns store/configuration failures; an uncertain write is never assumed undone.
    pub async fn deliver_once(&self) -> Result<DeliveryOutcome, DeliveryError> {
        self.deliver_once_with_clock(&|| jiff::Timestamp::now().as_second())
            .await
    }

    // Scheduling time is independent of HTTP signing time. Tests move this clock
    // across exact persisted retry boundaries without racing whole-second ticks.
    pub(super) async fn deliver_once_with_clock(
        &self,
        now: &(dyn Fn() -> i64 + Sync),
    ) -> Result<DeliveryOutcome, DeliveryError> {
        let claim_time = now();
        let destination = self.destination.id().clone();
        let claim = self
            .store
            .request(
                deadline(
                    self.destination.limits.store_timeout,
                    DeliveryError::Timeout,
                )?,
                move |s| s.claim_destination(claim_time, &destination),
            )
            .await?;
        let Some(claim) = claim else {
            return Ok(DeliveryOutcome::Idle);
        };
        let destination = self.destination;
        let (action, conflict_detail) = match tokio::time::timeout(
            destination.limits.request_timeout,
            self.send(destination, &claim),
        )
        .await
        {
            Ok(Ok(action)) => action,
            Ok(Err(
                DeliveryError::Destination(
                    crate::DestinationError::Secrets | crate::DestinationError::Signature(_),
                )
                | DeliveryError::Signature(_),
            )) => (
                ResponseAction::PauseForSetup {
                    reason: "destination credentials require operator repair",
                },
                None,
            ),
            Ok(Err(
                DeliveryError::Http(_) | DeliveryError::ResponseTooLarge | DeliveryError::Timeout,
            ))
            | Err(_) => (ResponseAction::Retry { after: None }, None),
            Ok(Err(error)) => return Err(error),
        };
        let now = now();
        let (attempt, outcome) = match action {
            ResponseAction::Done(status) => (Attempt::Done, DeliveryOutcome::Done(status)),
            ResponseAction::Conflict => (
                Attempt::Conflict(reason(
                    conflict_detail
                        .as_deref()
                        .unwrap_or("app reported delivery identity conflict"),
                )),
                DeliveryOutcome::Parked,
            ),
            ResponseAction::Parked { reason: detail } => {
                (Attempt::Parked(reason(&detail)), DeliveryOutcome::Parked)
            }
            ResponseAction::PauseForSetup { reason: detail } => {
                (Attempt::Paused(reason(detail)), DeliveryOutcome::Paused)
            }
            ResponseAction::Retry { after } => {
                let delay = i64::try_from(backoff(destination, claim.earlier_attempts, after))?;
                let next = now
                    .checked_add(delay)
                    .ok_or(DeliveryError::RetryOverflow { now, delay })?;
                (Attempt::RetryAt(next), DeliveryOutcome::RetryAt(next))
            }
        };
        let id = claim.id;
        self.store
            .request(
                deadline(destination.limits.store_timeout, DeliveryError::Timeout)?,
                move |s| s.attempted(id, &attempt, now),
            )
            .await?;
        tracing::debug!(delivery_id=%uuid::Uuid::from(id),destination=%destination.id.as_str(),"delivery attempt recorded");
        Ok(outcome)
    }

    async fn send(
        &self,
        destination: &Destination,
        claim: &analyte_store::Claim,
    ) -> Result<(ResponseAction, Option<String>), DeliveryError> {
        if claim.contract_version != 1 {
            return Ok((
                ResponseAction::Parked {
                    reason: "unsupported stored contract version".into(),
                },
                None,
            ));
        }
        let credentials = destination.credentials(self.secrets)?;
        let id = uuid::Uuid::from(claim.id).to_string();
        let timestamp = jiff::Timestamp::now().as_second().to_string();
        let signatures = credentials
            .signing_secrets
            .iter()
            .map(|key| key.sign(&id, &timestamp, &claim.payload))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        let path = match claim.kind {
            DeliveryKind::Result => "results",
            DeliveryKind::OrderEvent => "order-events",
        };
        let response = destination
            .client
            .post(destination.url(path)?)
            .bearer_auth(&credentials.api_key)
            .header("content-type", "application/json")
            .header("webhook-id", id)
            .header("webhook-timestamp", timestamp)
            .header("webhook-signature", signatures)
            .body(claim.payload.clone())
            .send()
            .await?;
        let status = response.status().as_u16();
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        // These actions need no body. In particular, a stalled error body must
        // not discard Retry-After or delay a setup pause.
        if matches!(status, 300..=399 | 401 | 403 | 404 | 408 | 413 | 429 | 500..=599) {
            return Ok((classify_response(status, &[], retry.as_deref()), None));
        }
        let bytes = match body(response, DeliveryError::ResponseTooLarge).await {
            Ok(bytes) => bytes,
            Err(DeliveryError::ResponseTooLarge) => {
                return Ok((classify_response(status, &[], retry.as_deref()), None));
            }
            Err(error) => return Err(error),
        };
        let detail = if status == 409 {
            analyte_contract::decode::<analyte_contract::ErrorBody>(&bytes)
                .ok()
                .filter(|body| body.contract_version == 1)
                .map(|body| body.detail.as_str().to_owned())
        } else {
            None
        };
        Ok((classify_response(status, &bytes, retry.as_deref()), detail))
    }

    /// Stops claiming on cancellation and drains the current attempt within `budget`.
    /// Its HTTP deadline still applies. Expiry leaves an interrupted claim for
    /// startup recovery with the same immutable ID and bytes (ADR 0008).
    /// # Errors
    /// Reports storage/configuration failures or expiry of the shutdown budget.
    pub async fn run_draining(
        &self,
        stop: watch::Receiver<bool>,
        budget: Duration,
    ) -> Result<(), DeliveryError> {
        self.run_bounded(stop, budget, false).await
    }

    /// Like [`Self::run_draining`], and also returns once the destination holds
    /// nothing the app has not accepted (`Store::undelivered` is zero). Items held
    /// for an operator (paused, parked, conflicting) keep it running, so a release
    /// or resend is still sent. For a destination no machine uses any more.
    /// # Errors
    /// Reports storage/configuration failures or expiry of the shutdown budget.
    pub async fn run_until_delivered(
        &self,
        stop: watch::Receiver<bool>,
        budget: Duration,
    ) -> Result<(), DeliveryError> {
        self.run_bounded(stop, budget, true).await
    }

    async fn run_bounded(
        &self,
        mut stop: watch::Receiver<bool>,
        budget: Duration,
        until_delivered: bool,
    ) -> Result<(), DeliveryError> {
        loop {
            if *stop.borrow() || stop.has_changed().is_err() {
                return Ok(());
            }
            let started = std::sync::atomic::AtomicBool::new(false);
            let attempt = async {
                started.store(true, std::sync::atomic::Ordering::Relaxed);
                self.deliver_once().await
            };
            tokio::pin!(attempt);
            let outcome = tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => {
                    if !started.load(std::sync::atomic::Ordering::Relaxed) { return Ok(()); }
                    tokio::time::timeout(budget, &mut attempt).await.map_err(|_| DeliveryError::Timeout)??;
                    return Ok(());
                }
                result = &mut attempt => result?,
            };
            if outcome == DeliveryOutcome::Idle {
                if until_delivered && self.undelivered().await? == 0 {
                    return Ok(());
                }
                tokio::select! {
                    biased;
                    _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(()),
                    () = tokio::time::sleep(IDLE_CLAIM_INTERVAL) => {}
                }
            }
        }
    }

    async fn undelivered(&self) -> Result<u64, DeliveryError> {
        let destination = self.destination.id().clone();
        Ok(self
            .store
            .request(
                deadline(
                    self.destination.limits.store_timeout,
                    DeliveryError::Timeout,
                )?,
                move |s| s.undelivered(&destination),
            )
            .await?)
    }

    /// Runs until cancellation or a specific storage/configuration error.
    /// Dropping this future has the same uncertain-claim semantics as `deliver_once`.
    /// # Errors
    /// Stops visibly on errors requiring host intervention.
    pub async fn run(&self, mut stop: watch::Receiver<bool>) -> Result<(), DeliveryError> {
        loop {
            let outcome = tokio::select! {
                biased;
                _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(()),
                result = self.deliver_once() => result?,
            };
            if outcome == DeliveryOutcome::Idle {
                tokio::select! {
                    biased;
                    _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => return Ok(()),
                    () = tokio::time::sleep(IDLE_CLAIM_INTERVAL) => {}
                }
            }
        }
    }
}

// App diagnostics must never terminate the delivery owner.
#[expect(
    clippy::expect_used,
    reason = "the bounded, nonempty, control-free text proves the model invariant"
)]
pub(crate) fn reason(text: &str) -> analyte_model::Reason {
    const LIMIT: usize = 1024;
    const MARKER: &str = "\u{2026} [truncated]";
    let mut safe = String::new();
    for ch in text.chars() {
        let token = if ch.is_control() {
            ch.escape_default().to_string()
        } else {
            ch.to_string()
        };
        if safe.len().saturating_add(token.len()) > LIMIT {
            while safe.len() > LIMIT.saturating_sub(MARKER.len()) {
                safe.pop();
            }
            safe.push_str(MARKER);
            break;
        }
        safe.push_str(&token);
    }
    if safe.is_empty() {
        safe.push_str("app supplied an empty diagnostic");
    }
    analyte_model::Reason::new(&safe)
        .expect("bounded nonempty diagnostic contains no control characters")
}

fn backoff(destination: &Destination, attempts: u64, after: Option<u64>) -> u64 {
    let min = destination.limits.backoff_min.as_secs();
    let cap = destination.limits.backoff_max.as_secs();
    let exponent = u32::try_from(attempts.min(63)).unwrap_or(63);
    let base = min
        .saturating_mul(1_u64.checked_shl(exponent).unwrap_or(u64::MAX))
        .min(cap);
    // UUID randomness provides independent scheduling jitter without a global RNG.
    let random = uuid::Uuid::now_v7().as_bytes().iter().fold(0_u64, |n, b| {
        n.wrapping_mul(257).wrapping_add(u64::from(*b))
    });
    base.saturating_add(random.checked_rem(base.max(1)).unwrap_or(0))
        .max(after.unwrap_or(0))
        .min(cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // ADR 0008
        #[test]
        fn backoff_is_capped_and_honours_retry_after(attempts in any::<u64>(), after in any::<u64>()) {
            let limits=crate::Limits {request_timeout:Duration::from_secs(2),store_timeout:Duration::from_secs(30),backoff_min:Duration::from_secs(2),backoff_max:Duration::from_secs(60),freshness:Duration::from_secs(30),poll_interval:Duration::from_secs(1),page_size:100,max_pages:100};
            let destination=Destination::new(Identifier::new("app")?,"https://example.invalid/",Identifier::new("secret")?,limits)?;
            let delay=backoff(&destination,attempts,Some(after));
            prop_assert!((2..=60).contains(&delay));
            prop_assert!(delay>=after.min(60));
            if attempts>=5 { prop_assert_eq!(delay,60); }
        }
    }

    // Contract v1 "Delivery responses and retries", published vector retry-after.json:
    // the requested delay is honoured up to the 60-second cap, never beyond it.
    #[test]
    fn retry_after_vector_is_honoured_up_to_cap() -> Result<(), Box<dyn std::error::Error>> {
        let vector: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../docs/contract/vectors/retry-after.json"
        ))?;
        let cap = vector["cap_seconds"].as_u64().ok_or("cap")?;
        let limits = crate::Limits {
            request_timeout: Duration::from_secs(2),
            store_timeout: Duration::from_secs(30),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(cap),
            freshness: Duration::from_secs(30),
            poll_interval: Duration::from_secs(1),
            page_size: 100,
            max_pages: 100,
        };
        let destination = Destination::new(
            Identifier::new("app")?,
            "https://example.invalid/",
            Identifier::new("secret")?,
            limits,
        )?;
        for case in vector["cases"].as_array().ok_or("cases")? {
            let Some(requested) = case["requested_seconds"].as_u64() else {
                continue;
            };
            let least = case["wait_at_least"].as_u64().ok_or("least")?;
            let most = case["wait_at_most"].as_u64().ok_or("most")?;
            for attempts in 0..8 {
                let wait = backoff(&destination, attempts, Some(requested));
                assert!(
                    (least..=most).contains(&wait),
                    "{requested} s gave {wait} s"
                );
            }
        }
        Ok(())
    }
}
