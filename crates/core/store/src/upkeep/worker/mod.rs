//! Runs blocking storage requests on an owned worker thread.
use crate::{Error, FreeSpace, Limits, Store};
use std::{path::PathBuf, thread, time::Instant};
use tokio::sync::{mpsc, oneshot};

type Operation = Box<dyn FnOnce(&mut Store) + Send>;

struct Request {
    deadline: Instant,
    operation: Operation,
}

impl Request {
    fn run(self, store: &mut Store, now: Instant) {
        if now < self.deadline {
            (self.operation)(store);
        }
    }
}

// This guard is declared after Handle so cancellation precedes closing its queue.
struct CancelStartup(crate::startup::Startup);

impl Drop for CancelStartup {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

// Completion is a notification, not a periodic probe of the thread state.
struct Finished(Option<oneshot::Sender<()>>);

impl Drop for Finished {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

/// Owns the dedicated storage thread and a bounded request queue.
/// Async owners must call `shutdown` before dropping, including on errors.
/// Drop signals stop; it joins only outside a Tokio runtime.
pub struct Handle {
    sender: Option<mpsc::Sender<Request>>,
    thread: Option<thread::JoinHandle<()>>,
    finished: oneshot::Receiver<()>,
}

impl Handle {
    /// Starts a worker; opening and migration happen on that thread.
    /// # Errors
    /// Refuses invalid queue capacity, thread creation and opening failures. A startup
    /// timeout names its active step; cancellation interrupts SQLite so the owner
    /// can roll back. Earlier committed migration versions are preserved. The
    /// caller must not abandon startup: dropping its future cannot await cleanup.
    pub async fn start(
        path: PathBuf,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        capacity: usize,
        deadline: Instant,
    ) -> Result<Self, Error> {
        if capacity == 0 || capacity > 1024 {
            return Err(Error::QueueCapacity { capacity });
        }
        let (sender, mut receiver) = mpsc::channel::<Request>(capacity);
        let (ready, opened) = oneshot::channel();
        let (finished, completion) = oneshot::channel();
        let startup = crate::startup::Startup::new(deadline);
        let opening = startup.clone();
        let thread = thread::Builder::new()
            .name("analyte-store".into())
            .spawn(move || {
                // Declared before the store so completion follows store destruction,
                // including early returns and unwinding.
                let _finished = Finished(Some(finished));
                let mut store = match Store::open_with_startup(&path, limits, space, &opening) {
                    Ok(store) => {
                        if ready.send(Ok(())).is_err() {
                            return;
                        }
                        store
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                while let Some(request) = receiver.blocking_recv() {
                    request.run(&mut store, Instant::now());
                }
            })?;
        let mut handle = Self {
            sender: Some(sender),
            thread: Some(thread),
            finished: completion,
        };
        let _cancel = CancelStartup(startup.clone());
        let result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), opened)
            .await
            .map_err(|_| {
                startup.cancel();
                Error::from(startup.error())
            })
            .and_then(|result| result.map_err(|_| Error::Closed))
            .and_then(|result| result.map_err(Error::from));
        if let Err(error) = result {
            startup.cancel();
            // Startup still owns cleanup on an opening error. Cancellation is
            // cooperative; preserve the original opening error if cleanup expires.
            let _ = handle.shutdown(deadline).await;
            return Err(error);
        }
        Ok(handle)
    }

    /// Executes one bounded storage operation on the owning thread.
    /// Cancellation before execution skips the request; once started a write may commit.
    /// Reconcile uncertain writes using their original IDs rather than inventing new ones.
    /// # Errors
    /// Reports queue/response timeout, worker closure or the operation's specific error.
    pub async fn request<T: Send + 'static>(
        &self,
        deadline: Instant,
        operation: impl FnOnce(&mut Store) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        let sender = self.sender.as_ref().ok_or(Error::Closed)?;
        let (reply, response) = oneshot::channel();
        let request = Request {
            deadline,
            operation: Box::new(move |store| {
                if !reply.is_closed() {
                    let _ = reply.send(operation(store));
                }
            }),
        };
        let until = tokio::time::Instant::from_std(deadline);
        tokio::time::timeout_at(until, sender.send(request))
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Closed)?;
        tokio::time::timeout_at(until, response)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Closed)?
    }

    /// Closes admission, drains the bounded queue and joins the owner thread.
    /// # Errors
    /// Reports expiration; the handle still owns the worker and can be shut down again.
    pub async fn shutdown(&mut self, deadline: Instant) -> Result<(), Error> {
        self.sender.take();
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut self.finished)
                .await
                .map_err(|_| Error::Timeout)?
                .map_err(|_| Error::Closed)?;
        }
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| Error::Closed)?;
        }
        Ok(())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let explicit_close_skipped = self.sender.take().is_some();
        if let Some(thread) = self.thread.take() {
            if tokio::runtime::Handle::try_current().is_ok() {
                tracing::error!(
                    explicit_close_skipped,
                    "store owner dropped inside Tokio before shutdown completed"
                );
                // Never block an async worker, even when reporting misuse.
                drop(thread);
                #[cfg(test)]
                debug_assert!(
                    !explicit_close_skipped,
                    "async store owner skipped explicit shutdown"
                );
            } else {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(test)]
mod tests;
