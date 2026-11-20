//! Runs read-only requests that stop at their caller's deadline or once the caller has gone, and never anything else.
//!
//! Only [`super::Handle::read`] builds these requests, and its work gets a
//! shared `&Store`: every store method that writes takes `&mut Store`, so a
//! read cannot call one. On top of that the connection is switched to
//! `query_only` for the read, so any write SQLite is asked for fails before it
//! changes a page.
//!
//! The read is stopped by a progress handler, not by `sqlite3_interrupt`. The
//! handler runs on the worker thread inside this read's own statements and is
//! made inert before the next request starts, so it can never reach a write. An
//! interrupt sent from another thread could land after the read has finished
//! and stop the request behind it, which may be machine data.
use crate::{Store, WorkerError};
use std::{
    ffi::c_int,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

/// SQLite virtual machine steps between two checks of a read's stop condition.
/// A check reads the clock and one flag; at this spacing a stopped read ends
/// within milliseconds of its deadline (ADR 0020 measures it) at no
/// measurable cost.
const CHECK_EVERY: c_int = 1000;

/// When a read must stop: at `deadline`, or once its caller has gone.
#[derive(Clone)]
pub(super) struct Stop {
    deadline: Instant,
    gone: Arc<AtomicBool>,
}

/// Held by the caller waiting for the read. Dropping it, because the caller has
/// its answer, gave up or was itself dropped, tells the worker the answer is no
/// longer wanted.
pub(super) struct Caller(Arc<AtomicBool>);

impl Drop for Caller {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Stop {
    /// A stop condition for a read due by `deadline`, and its caller's half.
    pub(super) fn new(deadline: Instant) -> (Self, Caller) {
        let gone = Arc::new(AtomicBool::new(false));
        (
            Self {
                deadline,
                gone: gone.clone(),
            },
            Caller(gone),
        )
    }

    /// True once the deadline has passed or the caller has gone.
    pub(super) fn reached(&self) -> bool {
        self.gone.load(Ordering::Acquire) || Instant::now() >= self.deadline
    }
}

impl Store {
    /// Runs `operation` as a read that stops when `stop` is reached.
    ///
    /// The read starts only outside any transaction, runs with the connection in
    /// `query_only`, and leaves the connection as it found it: no progress
    /// handler that can still fire, `query_only` off and no transaction open.
    /// A read that was stopped returns [`WorkerError::ReadStopped`] in place of
    /// whatever its statements returned.
    pub(super) fn read_until<T, E: From<WorkerError>>(
        &mut self,
        stop: &Stop,
        operation: impl FnOnce(&Self) -> Result<T, E>,
    ) -> Result<T, E> {
        // A transaction left open by an earlier write belongs to that write:
        // stopping a statement inside it could cost its changes.
        if !self.db.is_autocommit() {
            return Err(WorkerError::ReadInsideTransaction.into());
        }
        self.db
            .pragma_update(None, "query_only", true)
            .map_err(WorkerError::ReadGuard)?;
        let armed = Arc::new(AtomicBool::new(true));
        let stopped = Arc::new(AtomicBool::new(false));
        let handler = {
            let (armed, stopped, stop) = (armed.clone(), stopped.clone(), stop.clone());
            move || {
                if armed.load(Ordering::Acquire) && stop.reached() {
                    stopped.store(true, Ordering::Release);
                    return true;
                }
                false
            }
        };
        let result = match self.db.progress_handler(CHECK_EVERY, Some(handler)) {
            Ok(()) => operation(self),
            Err(error) => Err(WorkerError::ReadGuard(error).into()),
        };
        // Disarm first: even if removing the handler fails, it can never stop
        // another statement.
        armed.store(false, Ordering::Release);
        let cleared = self.end_read();
        cleared.map_err(WorkerError::ReadGuard)?;
        if stopped.load(Ordering::Acquire) {
            return Err(WorkerError::ReadStopped.into());
        }
        result
    }

    /// Puts the connection back as a write expects it after a read.
    fn end_read(&mut self) -> rusqlite::Result<()> {
        let removed = self.db.progress_handler(0, None::<fn() -> bool>);
        // A read cannot begin a transaction through the store's methods; this
        // only guards the rule that a read never leaves one open.
        let closed = if self.db.is_autocommit() {
            Ok(())
        } else {
            self.db.execute_batch("ROLLBACK")
        };
        let writable = self.db.pragma_update(None, "query_only", false);
        removed.and(closed).and(writable)
    }
}
