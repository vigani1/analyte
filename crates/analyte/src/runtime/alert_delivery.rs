//! Drives independent alert endpoints without gating any clinical worker.
use crate::{
    Error, deadline,
    installation::config::{AlertDelivery, AlertEndpoint},
};
use analyte_store::{AlertFailure, Handle};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// A cancelled cycle may have reserved an attempt or sent bytes; the durable
/// deadline, identity and bytes survive. Shutdown never waits for the HTTP timeout.
pub(crate) async fn run(
    store: Arc<Handle>,
    endpoint: AlertEndpoint,
    policy: AlertDelivery,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut ticks = tokio::time::interval(Duration::from_secs(1));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if *stop.borrow() {
            return Ok(());
        }
        tokio::select! { biased;
            _ = stop.changed() => return Ok(()),
            _ = ticks.tick() => {}
        }
        tokio::select! { biased;
            _ = stop.changed() => return Ok(()),
            result = cycle(&store,&endpoint,&policy,jiff::Timestamp::now().as_second()) => {
                if let Err(error) = result {
                    // Only host/store errors reach here; send() returns fixed categories.
                    tracing::error!(endpoint=endpoint.id().as_str(), %error, "alert endpoint storage cycle failed");
                }
            }
        }
    }
}

async fn cycle(
    store: &Handle,
    endpoint: &AlertEndpoint,
    policy: &AlertDelivery,
    now: i64,
) -> Result<(), Error> {
    let id = endpoint.id().clone();
    store
        .request(deadline(), move |s| s.register_alert_endpoint(&id))
        .await?;
    let id = endpoint.id().clone();
    let reminder = policy.reminder().as_secs();
    let capacity = policy.capacity();
    store
        .request(deadline(), move |s| {
            s.prepare_alert_event(&id, now, reminder, capacity)
        })
        .await?;
    let id = endpoint.id().clone();
    let attempts = policy.attempts();
    let min = policy.backoff_min().as_secs();
    let max = policy.backoff_max().as_secs();
    let Some(event) = store
        .request(deadline(), move |s| {
            s.claim_alert_attempt(&id, now, attempts, min, max)
        })
        .await?
    else {
        return Ok(());
    };
    let outcome = match std::env::var(crate::installation::secrets::variable(
        endpoint.secret().as_str(),
    )) {
        Ok(token) => match analyte_sink::WebhookSender::new(endpoint.url(), policy.timeout()) {
            Ok(sender) => sender.send(&event, &token).await,
            Err(_) => Err(AlertFailure::Network),
        },
        Err(_) => Err(AlertFailure::Credentials),
    };
    match outcome {
        Ok(()) => {
            store
                .request(deadline(), move |s| {
                    s.complete_alert_attempt(&event.id, now)
                })
                .await?;
        }
        Err(failure) => {
            store
                .request(deadline(), move |s| {
                    s.fail_alert_attempt(&event.id, failure)
                })
                .await?;
        }
    }
    Ok(())
}
