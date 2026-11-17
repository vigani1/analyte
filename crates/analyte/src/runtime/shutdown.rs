//! Enforces a process shutdown deadline independent of async scheduling.
use crate::runtime::Error;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub(crate) type Notice = Arc<dyn Fn() + Send + Sync>;
type Components = Arc<Mutex<BTreeSet<String>>>;
enum Event {
    Arm,
    Finished,
}

pub(crate) struct Watchdog {
    events: mpsc::SyncSender<Event>,
    owner: Option<thread::JoinHandle<()>>,
    active: Components,
    armed: AtomicBool,
}

pub(crate) struct Component {
    id: String,
    active: Components,
}
impl Drop for Component {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.id);
        }
    }
}
impl Watchdog {
    pub(crate) fn new(bound: Duration, notice: Option<Notice>) -> Result<Self, Error> {
        let (events, received) = mpsc::sync_channel(2);
        let active = Components::default();
        let running = active.clone();
        let owner = thread::Builder::new()
            .name("shutdown-deadline".into())
            .spawn(move || {
                if !matches!(received.recv(), Ok(Event::Arm)) {
                    return;
                }
                if matches!(
                    received.recv_timeout(bound),
                    Ok(Event::Finished) | Err(mpsc::RecvTimeoutError::Disconnected)
                ) {
                    return;
                }
                if let Some(notice) = notice {
                    notice();
                }
                let components = running
                    .lock()
                    .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_else(|_| vec!["component-registry".into()]);
                tracing::error!(components = ?components, exit_code = 3, "shutdown bound exceeded");
                // ADR 0003: durable-before-ACK and crash recovery preserve receipts,
                // delivery identities and uncertain orders. Bypass blocked owner destructors.
                std::process::exit(3);
            })?;
        Ok(Self {
            events,
            owner: Some(owner),
            active,
            armed: AtomicBool::new(false),
        })
    }
    pub(crate) fn component(&self, id: String) -> Component {
        if let Ok(mut active) = self.active.lock() {
            active.insert(id.clone());
        }
        Component {
            id,
            active: self.active.clone(),
        }
    }
    pub(crate) fn arm(&self) {
        if !self.armed.swap(true, Ordering::AcqRel) {
            let _ = self.events.send(Event::Arm);
        }
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Finished);
        if let Some(owner) = self.owner.take() {
            let _ = owner.join();
        }
    }
}

/// Waits until shutdown is requested or its owner disappears, ignoring false updates.
/// Cancellation leaves the receiver owned by the caller.
pub(crate) async fn stopped(stop: &mut tokio::sync::watch::Receiver<bool>) {
    loop {
        if *stop.borrow() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    struct Stuck;
    impl Drop for Stuck {
        fn drop(&mut self) {
            loop {
                thread::park();
            }
        }
    }
    #[test]
    fn plan11_agents_concurrency_stuck_component_exits_three() -> Result<(), Error> {
        const CHILD: &str = "ANALYTE_WATCHDOG_CHILD";
        if std::env::var_os(CHILD).is_some() {
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .init();
            let watchdog = Watchdog::new(Duration::from_millis(100), None)?;
            let _component = watchdog.component("synthetic-stuck-owner".into());
            watchdog.arm();
            watchdog.arm(); // Repeated stop requests must not restart or shorten the deadline.
            drop(Stuck);
            return Err("stuck destructor unexpectedly returned".into());
        }
        let started = std::time::Instant::now();
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "runtime::shutdown::tests::plan11_agents_concurrency_stuck_component_exits_three",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        while child.try_wait()?.is_none() {
            if started.elapsed() >= Duration::from_secs(5) {
                child.kill()?;
                child.wait()?;
                return Err("shutdown watchdog failed to exit within five seconds".into());
            }
            // std::process has no timed wait; keep the five-second kill bound for a broken child.
            thread::sleep(Duration::from_millis(10));
        }
        let child = child.wait_with_output()?;
        assert_eq!(child.status.code(), Some(3));
        assert!(started.elapsed() < Duration::from_secs(5));
        let log = String::from_utf8(child.stderr)?;
        assert!(log.contains("synthetic-stuck-owner"));
        assert_eq!(log.matches("shutdown bound exceeded").count(), 1);
        Ok(())
    }
    #[test]
    fn plan11_completed_shutdown_joins_watchdog() -> Result<(), Error> {
        let watchdog = Watchdog::new(Duration::from_millis(20), None)?;
        watchdog.arm();
        drop(watchdog);
        Ok(())
    }
}
