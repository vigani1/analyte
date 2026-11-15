//! Startup progress and cooperative SQLite cancellation.
use crate::Error;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::watch;

/// Default startup budget, including full integrity verification and migrations.
pub const DEFAULT_STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// Payload-free startup operations suitable for operator diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupStep {
    /// Opening the database and reading its schema version.
    Opening,
    /// Checking database and foreign-key integrity.
    Integrity,
    /// Applying one atomic schema migration.
    Migration {
        /// Previously committed schema version.
        from: i64,
        /// Version committed by this migration.
        to: i64,
    },
    /// Installing connection policy and immutable-evidence guards.
    Finalizing,
}
impl std::fmt::Display for StartupStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Opening => f.write_str("opening"),
            Self::Integrity => f.write_str("integrity check"),
            Self::Migration { from, to } => write!(f, "migration {from} to {to}"),
            Self::Finalizing => f.write_str("finalizing"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Startup {
    deadline: Instant,
    step: watch::Sender<StartupStep>,
    cancelled: Arc<AtomicBool>,
    interrupted: Arc<AtomicBool>,
    #[cfg(test)]
    pub(crate) clock: Option<Arc<dyn Fn() -> Instant + Send + Sync>>,
}
impl Startup {
    pub(crate) fn new(deadline: Instant) -> Self {
        let (step, _) = watch::channel(StartupStep::Opening);
        Self {
            deadline,
            step,
            cancelled: Arc::new(AtomicBool::new(false)),
            interrupted: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            clock: None,
        }
    }
    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return clock();
        }
        Instant::now()
    }
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn expired(&self) -> bool {
        if self.now() >= self.deadline {
            self.cancel();
        }
        self.cancelled.load(Ordering::Acquire)
    }
    pub(crate) fn error(&self) -> Error {
        Error::StartupTimeout {
            step: *self.step.borrow(),
        }
    }
    pub(crate) fn check(&self) -> Result<(), Error> {
        if self.expired() {
            Err(self.error())
        } else {
            Ok(())
        }
    }
    pub(crate) fn install(&self, db: &mut rusqlite::Connection) -> Result<(), Error> {
        let startup = self.clone();
        // Interrupt only once: rollback and connection cleanup must still execute.
        db.progress_handler(
            1,
            Some(move || startup.expired() && !startup.interrupted.swap(true, Ordering::AcqRel)),
        )?;
        Ok(())
    }
    pub(crate) fn run<T>(
        &self,
        step: StartupStep,
        operation: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.check()?;
        self.step.send_replace(step);
        let started = self.now();
        match step {
            StartupStep::Migration { from, to } => {
                tracing::info!(step=%step, from_version=from, to_version=to, "store startup step started");
            }
            StartupStep::Integrity => {}
            StartupStep::Opening | StartupStep::Finalizing => {
                tracing::info!(step=%step, "store startup step started");
            }
        }
        let result = operation().and_then(|value| {
            self.check()?;
            Ok(value)
        });
        let elapsed_ms = self.now().saturating_duration_since(started).as_millis();
        if step != StartupStep::Integrity {
            tracing::info!(step=%step, elapsed_ms, succeeded=result.is_ok(), "store startup step finished");
        }
        if self.expired() {
            Err(self.error())
        } else {
            result
        }
    }
    #[cfg(test)]
    pub(crate) fn progress(&self) -> watch::Receiver<StartupStep> {
        self.step.subscribe()
    }
}
