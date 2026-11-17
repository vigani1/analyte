//! Waits for program shutdown without taking ownership of the signal.
use tokio::sync::watch;

/// Waits until shutdown is requested or its owner disappears, ignoring false updates.
/// Cancellation leaves the receiver owned by the caller.
pub(crate) async fn stopped(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}
