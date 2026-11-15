//! Test-only commit/ACK boundary control. Never enable `test-hooks` in release builds.
use crate::{Error, invalid};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Notify, watch};
/// One-shot pause after machine bytes are durable, before interpretation or ACK.
/// It cannot inspect/modify bytes or bypass storage. Dropping control stops the connection.
pub struct Committed {
    armed: AtomicBool,
    reached: Notify,
    decision: watch::Receiver<Option<bool>>,
}
/// Owns the decision for one bounded pause; drop means stop, never acknowledge.
pub struct Control {
    hook: Arc<Committed>,
    decision: watch::Sender<Option<bool>>,
}
/// Compatibility name for the ASTM durable-frame boundary.
pub type AstmCommitted = Committed;

impl Committed {
    /// Creates an armed hook and its control without starting a task.
    #[must_use]
    pub fn new() -> (Arc<Self>, Control) {
        let (decision, receiver) = watch::channel(None);
        let hook = Arc::new(Self {
            armed: AtomicBool::new(true),
            reached: Notify::new(),
            decision: receiver,
        });
        (hook.clone(), Control { hook, decision })
    }
    pub(crate) async fn pause(&self, stop: &mut watch::Receiver<bool>) -> Result<(), Error> {
        if !self.armed.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        self.reached.notify_one();
        let mut decision = self.decision.clone();
        tokio::select! {
            () = crate::machine::stopping(stop) => Err(invalid("test commit hook stopped connection")),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    match *decision.borrow_and_update() {
                        Some(true) => return Ok(()),
                        Some(false) => return Err(invalid("test commit hook stopped connection")),
                        None => (),
                    }
                    decision.changed().await.map_err(|_| invalid("test commit hook control dropped"))?;
                }
            }) => result.map_err(|_| invalid("test commit hook deadline"))?,
        }
    }
}
impl Control {
    /// Waits until durable storage has completed and the connection is paused.
    /// # Errors
    /// Fails after five seconds if the named boundary was never reached.
    pub async fn reached(&self) -> Result<(), Error> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.hook.reached.notified(),
        )
        .await
        .map_err(|_| invalid("test commit hook not reached"))
    }
    /// Continues normal processing; this never directly writes an ACK.
    pub fn resume(&self) {
        self.decision.send_replace(Some(true));
    }
    /// Stops the connection without generating the pending ACK.
    pub fn stop(&self) {
        self.decision.send_replace(Some(false));
    }
}
