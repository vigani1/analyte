//! Controls bounded synthetic pause gates and store probes.
use super::Error;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Notify, watch};
/// One-shot pause after a configured host boundary has completed.
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
            () = crate::machine::stopping(stop) => Err(Error::Stopped),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    match *decision.borrow_and_update() {
                        Some(true) => return Ok(()),
                        Some(false) => return Err(Error::Stopped),
                        None => (),
                    }
                    decision.changed().await.map_err(|_| Error::ControlDropped)?;
                }
            }) => result.map_err(|_| Error::Deadline)?,
        }
    }
}
impl Control {
    /// Waits until the configured host boundary has completed and its owner is paused.
    /// # Errors
    /// Fails after five seconds if the named boundary was never reached.
    pub async fn reached(&self) -> Result<(), Error> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.hook.reached.notified(),
        )
        .await
        .map_err(|_| Error::NotReached)
    }
    /// Continues normal processing; this never directly writes an ACK.
    pub fn resume(&self) {
        self.decision.send_replace(Some(true));
    }
    /// Arms the next boundary after the previous pause has resumed and completed.
    pub fn arm_next(&self) {
        self.decision.send_replace(None);
        self.hook.armed.store(true, Ordering::Release);
    }
    /// Stops the connection without generating the pending ACK.
    pub fn stop(&self) {
        self.decision.send_replace(Some(false));
    }
}

/// LAW host boundaries; absent gates have no effect, and this type is test-only.
#[derive(Clone, Default)]
pub struct LawGates {
    /// Entry to the actual cache lookup, after QBP custody.
    pub lookup: Option<Arc<Committed>>,
    /// All reservations and retained commands have committed.
    pub reserved: Option<Arc<Committed>>,
    /// Candidate chosen, immediately before transactional cancellation admission.
    pub cancel: Option<Arc<Committed>>,
    /// Transactional cancellation admission has returned.
    pub cancel_result: Option<Arc<Committed>>,
    /// Queued followup owns reservations, before any RSP write.
    pub handover: Option<Arc<Committed>>,
    /// RSP write completed, before releasing outbound readiness.
    pub rsp: Option<Arc<Committed>>,
    /// Worker has received a queued followup but has not awaited RSP readiness.
    pub queued: Option<Arc<Committed>>,
    /// Injects an already-expired queued deadline after the worker gate is released.
    pub expire_queued: bool,
    /// First pending page collected on the store thread.
    pub pages: Option<Arc<StoreGate>>,
    /// Weak access to the running store for competing feed requests.
    pub store: Option<Arc<StoreProbe>>,
}
impl Committed {
    pub(crate) async fn operation(&self) -> Result<(), Error> {
        let (_owner, mut stop) = watch::channel(false);
        self.pause(&mut stop).await
    }
}
/// Publishes a weak test reference; never keeps the service's store alive.
#[derive(Default)]
pub struct StoreProbe {
    store: std::sync::Mutex<std::sync::Weak<analyte_store::Handle>>,
    ready: Notify,
}
impl StoreProbe {
    pub(crate) fn publish(&self, store: &Arc<analyte_store::Handle>) -> Result<(), Error> {
        *self.store.lock().map_err(|_| Error::ProbePoisoned)? = Arc::downgrade(store);
        self.ready.notify_one();
        Ok(())
    }
    /// Waits for the service store; caller must drop its reference before shutdown.
    /// # Errors
    /// Refuses missing startup within five seconds.
    pub async fn get(&self) -> Result<Arc<analyte_store::Handle>, Error> {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.ready.notified()).await?;
        self.store
            .lock()
            .map_err(|_| Error::ProbePoisoned)?
            .upgrade()
            .ok_or(Error::StoreClosed)
    }
}
/// One-shot bounded pause on the dedicated store thread, never a runtime thread.
pub struct StoreGate {
    armed: AtomicBool,
    reached: Notify,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
/// Controls the store-thread pause without holding its receiver mutex.
pub struct StoreControl {
    gate: Arc<StoreGate>,
    release: std::sync::mpsc::SyncSender<()>,
}
impl StoreGate {
    /// Creates a store-thread gate and its owner.
    #[must_use]
    pub fn new() -> (Arc<Self>, StoreControl) {
        let (release, receiver) = std::sync::mpsc::sync_channel(1);
        let gate = Arc::new(Self {
            armed: AtomicBool::new(true),
            reached: Notify::new(),
            release: std::sync::Mutex::new(receiver),
        });
        (gate.clone(), StoreControl { gate, release })
    }
    pub(crate) fn pause(&self) -> Result<(), Error> {
        if self.armed.swap(false, Ordering::AcqRel) {
            self.reached.notify_one();
            self.release
                .lock()
                .map_err(|_| Error::ProbePoisoned)?
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| Error::Deadline)?;
        }
        Ok(())
    }
}
impl StoreControl {
    /// Waits for the first collected page.
    /// # Errors
    /// Refuses an unreached boundary within five seconds.
    pub async fn reached(&self) -> Result<(), Error> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.gate.reached.notified(),
        )
        .await?;
        Ok(())
    }
    /// Releases the paused collection.
    pub fn resume(&self) {
        let _ = self.release.try_send(());
    }
}
