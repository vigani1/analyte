//! Drives independent alert endpoints without gating any clinical worker.
use crate::runtime::Error;
use crate::{
    deadline,
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
    let sender = analyte_sink::WebhookSender::new(endpoint.url(), policy.timeout()).ok();
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
        let token = std::env::var(crate::installation::secrets::variable(
            endpoint.secret().as_str(),
        ))
        .ok();
        tokio::select! { biased;
            _ = stop.changed() => return Ok(()),
            result = cycle(&store,endpoint.id(),&policy,jiff::Timestamp::now().as_second(),sender.as_ref(),token.as_deref()) => {
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
    endpoint: &analyte_model::Identifier,
    policy: &AlertDelivery,
    now: i64,
    sender: Option<&analyte_sink::WebhookSender>,
    token: Option<&str>,
) -> Result<(), Error> {
    let id = endpoint.clone();
    store
        .request(deadline()?, move |s| s.register_alert_endpoint(&id))
        .await?;
    let id = endpoint.clone();
    let reminder = policy.reminder().as_secs();
    let capacity = policy.capacity();
    store
        .request(deadline()?, move |s| {
            s.prepare_alert_event(&id, now, reminder, capacity)
        })
        .await?;
    let id = endpoint.clone();
    let attempts = policy.attempts();
    let min = policy.backoff_min().as_secs();
    let max = policy.backoff_max().as_secs();
    let Some(event) = store
        .request(deadline()?, move |s| {
            s.claim_alert_attempt(&id, now, attempts, min, max)
        })
        .await?
    else {
        return Ok(());
    };
    let outcome = match (token, sender) {
        (Some(token), Some(sender)) => sender.send(&event, token).await,
        (None, _) => Err(AlertFailure::Credentials),
        (Some(_), None) => Err(AlertFailure::Network),
    };
    match outcome {
        Ok(()) => {
            store
                .request(deadline()?, move |s| {
                    s.complete_alert_attempt(event.id(), now)
                })
                .await?;
        }
        Err(failure) => {
            store
                .request(deadline()?, move |s| {
                    s.fail_alert_attempt(event.id(), failure)
                })
                .await?;
        }
    }
    Ok(())
}

#[cfg(all(test, feature = "test-hooks"))]
mod tests {
    use super::*;
    use crate::Error;
    use std::sync::Mutex;
    use tracing::instrument::WithSubscriber as _;
    #[derive(Clone)]
    struct Log(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Log {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| std::io::Error::other("test log poisoned"))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Log {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }
    async fn open(path: std::path::PathBuf) -> Result<Handle, Error> {
        Ok(Handle::start(
            path,
            analyte_store::Limits {
                reserve_bytes: 0,
                database_bytes: 100_000_000,
                record_bytes: 2_097_152,
                page_rows: 100,
                busy_timeout: Duration::from_millis(100),
            },
            Box::new(crate::runtime::space::Volume),
            8,
            crate::test_startup_deadline()?,
        )
        .await?)
    }
    #[tokio::test]
    async fn adr0014_worker_credential_failure_is_visible_and_never_logged() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let mut store = open(temp.path().join("store.db")).await?;
        store
            .request(deadline()?, |s| {
                s.evaluate_alert(
                    &analyte_store::AlertKey::ParkedItems,
                    "Duty officer",
                    true,
                    Some(1),
                    100,
                )
            })
            .await?;
        let policy = AlertDelivery::for_test("queue_capacity=1\nmax_attempts=1")?;
        let endpoint = analyte_model::Identifier::new("relay")?;
        let sender = analyte_sink::WebhookSender::new(
            &analyte_sink::WebhookUrl::parse("https://private.invalid/hook")?,
            Duration::from_secs(1),
        )?;
        let log = Log(Arc::new(Mutex::new(Vec::new())));
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(log.clone())
            .finish();
        cycle(
            &store,
            &endpoint,
            &policy,
            100,
            Some(&sender),
            Some("SECRET-CREDENTIAL\nPATIENT-SENTINEL"),
        )
        .with_subscriber(subscriber)
        .await?;
        let status = store
            .request(deadline()?, |s| s.alert_endpoint_status())
            .await?;
        assert_eq!(status[0].failed, 1);
        assert_eq!(status[0].exhausted, 1);
        let output = String::from_utf8(
            log.0
                .lock()
                .map_err(|_| std::io::Error::other("test log poisoned"))?
                .clone(),
        )?;
        for forbidden in [
            "SECRET-CREDENTIAL",
            "PATIENT-SENTINEL",
            "private.invalid",
            "https://",
        ] {
            assert!(!output.contains(forbidden));
        }
        store.shutdown(deadline()?).await?;
        Ok(())
    }
    #[tokio::test]
    async fn adr0014_cancelling_worker_http_wait_keeps_reserved_event() -> Result<(), Error> {
        use tokio::io::AsyncReadExt as _;
        let temp = tempfile::tempdir()?;
        let mut store = open(temp.path().join("store.db")).await?;
        store
            .request(deadline()?, |s| {
                s.evaluate_alert(
                    &analyte_store::AlertKey::ParkedItems,
                    "Duty officer",
                    true,
                    Some(1),
                    100,
                )
            })
            .await?;
        let policy = AlertDelivery::for_test("")?;
        let endpoint = analyte_model::Identifier::new("relay")?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let sender = analyte_sink::WebhookSender::for_loopback_test(
            &format!("http://{}/alerts", listener.local_addr()?),
            Duration::from_secs(30),
        )?;
        let mut pending = Box::pin(cycle(
            &store,
            &endpoint,
            &policy,
            100,
            Some(&sender),
            Some("synthetic-token"),
        ));
        let mut peer = tokio::select! {
            result = &mut pending => { result?; return Err("HTTP wait unexpectedly finished".into()); },
            result = listener.accept() => result?.0,
        };
        tokio::select! {
            result = &mut pending => { result?; return Err("HTTP wait unexpectedly finished".into()); },
            result = peer.read_u8() => { result?; }
        }
        // Dropping the owned future models the service's cancellation select.
        drop(pending);
        let status = store
            .request(deadline()?, |s| s.alert_endpoint_status())
            .await?;
        assert_eq!(status[0].pending, 1);
        assert_eq!(status[0].next_attempt, Some(105));
        let retry = store
            .request(deadline()?, move |s| {
                s.claim_alert_attempt(&endpoint, 105, 10, 5, 300)
            })
            .await?
            .ok_or("cancelled event lost")?;
        assert_eq!(retry.attempts(), 2);
        drop(peer);
        store.shutdown(deadline()?).await?;
        Ok(())
    }
}
