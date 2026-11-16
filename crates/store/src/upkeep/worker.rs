//! Worker storage operations owned by upkeep.
use crate::{Error, FreeSpace, Limits, Store};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

type Operation = Box<dyn FnOnce(&mut Store) + Send>;

struct Request {
    deadline: Instant,
    operation: Operation,
}

/// Owns the dedicated storage thread and a bounded request queue.
/// Dropping closes the queue and joins the worker; explicitly shut down before runtime exit.
pub struct Handle {
    sender: Option<mpsc::Sender<Request>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Handle {
    /// Starts a worker; opening and migration happen on that thread.
    /// # Errors
    /// Refuses invalid queue capacity, thread creation and opening failures. A startup
    /// timeout names its active step; cancellation interrupts SQLite and joins the owner
    /// after rollback. Earlier committed migration versions are preserved.
    pub async fn start(
        path: PathBuf,
        limits: Limits,
        space: Box<dyn FreeSpace>,
        capacity: usize,
        deadline: Instant,
    ) -> Result<Self, Error> {
        if capacity == 0 || capacity > 1024 {
            return Err(Error::InvalidInput);
        }
        let (sender, mut receiver) = mpsc::channel::<Request>(capacity);
        let (ready, opened) = oneshot::channel();
        let startup = crate::startup::Startup::new(deadline);
        let opening = startup.clone();
        let thread = thread::Builder::new()
            .name("analyte-store".into())
            .spawn(move || {
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
                    if Instant::now() < request.deadline {
                        (request.operation)(&mut store);
                    }
                }
            })?;
        let handle = Self {
            sender: Some(sender),
            thread: Some(thread),
        };
        // Cancellation must interrupt startup before Handle's Drop joins its worker.
        struct CancelStartup(crate::startup::Startup);
        impl Drop for CancelStartup {
            fn drop(&mut self) {
                self.0.cancel();
            }
        }
        let _cancel = CancelStartup(startup.clone());
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), opened)
            .await
            .map_err(|_| {
                startup.cancel();
                startup.error()
            })?
            .map_err(|_| Error::Closed)??;
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
        while self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| Error::Closed)?;
        }
        Ok(())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests;
