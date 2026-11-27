//! Starts service workers, coordinates bounded shutdown and waits for worker ticks.
pub(crate) mod delivery;
#[cfg(test)]
pub(crate) mod log_capture;
pub mod service;
pub(crate) mod shutdown;
pub(crate) mod space;
pub mod test_hooks;
#[cfg(windows)]
pub mod windows_service;

pub(crate) mod retention;

pub(crate) mod review_notices;

pub(crate) mod alert_delivery;

mod recovery;

mod error;
pub use error::{DeliveryCommandError, Error, ParkedCommandError};

pub(crate) mod hooks;

/// Returns false on shutdown or sender loss, including an already-requested stop.
/// Cancellation leaves the interval and watch receiver owned by the caller.
pub(crate) async fn until_stop(
    ticks: &mut tokio::time::Interval,
    stop: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    if *stop.borrow() {
        return false;
    }
    tokio::select! {
        biased;
        _ = async { let _ = stop.wait_for(|&stopped| stopped).await; } => false,
        _ = ticks.tick() => true,
    }
}
