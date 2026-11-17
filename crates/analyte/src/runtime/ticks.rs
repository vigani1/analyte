//! Waits for periodic worker ticks while prioritizing shutdown.
use tokio::{sync::watch, time::Interval};

/// Returns false on shutdown or sender loss, including an already-requested stop.
/// Cancellation leaves the interval and watch receiver owned by the caller.
pub(crate) async fn until_stop(ticks: &mut Interval, stop: &mut watch::Receiver<bool>) -> bool {
    if *stop.borrow() {
        return false;
    }
    tokio::select! {
        biased;
        _ = analyte_sink::stopped(stop) => false,
        _ = ticks.tick() => true,
    }
}
