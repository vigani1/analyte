//! File handoff stays claimed until receipt and interpretation are durable.
use crate::{Error, config::Machine, deadline, evidence, invalid};
use analyte_model::ReceiptId;
use analyte_store::Handle;
use analyte_transport::folder::Watcher;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::watch;

struct Item {
    bytes: Vec<u8>,
    stored: mpsc::SyncSender<bool>,
}

struct Owner {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<Result<(), Error>>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            super::worker::report_join("folder", &thread.join());
        }
    }
}

impl Owner {
    fn start(
        incoming: PathBuf,
        root: PathBuf,
        limit: u64,
        mut recorder: Option<Box<dyn FnMut() -> Option<analyte_transport::recording::Tap> + Send>>,
    ) -> Result<(Self, tokio::sync::mpsc::Receiver<Item>), Error> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let thread = thread::Builder::new()
            .name("analyte-files".into())
            .spawn(move || {
                let claimed = root.join("claimed");
                std::fs::create_dir_all(&claimed)?;
                std::fs::create_dir_all(root.join("archive"))?;
                std::fs::create_dir_all(root.join("quarantine"))?;
                // Completion is producer rename into incoming, never a guessed stable size.
                let mut watcher = Watcher::new(incoming, claimed, None, limit)?;
                let mut claims = watcher.recover()?;
                while !cancelled.load(Ordering::Acquire) {
                    if claims.is_empty() {
                        claims = watcher.scan()?;
                    }
                    for claim in claims.drain(..) {
                        if cancelled.load(Ordering::Acquire) {
                            return Ok(());
                        }
                        let bytes = claim.read()?;
                        if let Some(tap) = recorder.as_mut().and_then(|factory| factory()) {
                            use analyte_transport::recording::Kind;
                            let result = tap
                                .record(Kind::Open, Instant::now(), b"watched folder receipt")
                                .and_then(|()| {
                                    for chunk in bytes.chunks(analyte_transport::CHUNK_LIMIT) {
                                        tap.record(Kind::In, Instant::now(), chunk)?;
                                    }
                                    tap.record(Kind::Close, Instant::now(), b"file read complete")
                                });
                            if let Err(error) = result {
                                // Tap::record publishes every failure to the shared worker state.
                                // The capture worker logs once and persists the failed markers.
                                tracing::debug!(error_kind = %error, "watched-folder capture failure handed to recorder worker");
                            }
                        }
                        let (reply, response) = mpsc::sync_channel(1);
                        let mut item = Item {
                            bytes,
                            stored: reply,
                        };
                        let until = deadline();
                        loop {
                            match sender.try_send(item) {
                                Ok(()) => break,
                                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                    return Ok(());
                                }
                                Err(tokio::sync::mpsc::error::TrySendError::Full(returned)) => {
                                    item = returned;
                                }
                            }
                            if cancelled.load(Ordering::Acquire) || Instant::now() >= until {
                                return Ok(());
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        // A lost reply leaves the source claimed; a restart reprocesses it safely.
                        let until = deadline();
                        let accepted = loop {
                            match response.recv_timeout(Duration::from_millis(10)) {
                                Ok(accepted) => break accepted,
                                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                                Err(mpsc::RecvTimeoutError::Timeout) => {
                                    if cancelled.load(Ordering::Acquire) || Instant::now() >= until
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                        };
                        let area = if accepted { "archive" } else { "quarantine" };
                        claim.finish(&root.join(area).join(uuid::Uuid::now_v7().to_string()))?;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            })?;
        Ok((
            Self {
                stop,
                thread: Some(thread),
            },
            receiver,
        ))
    }

    async fn close(mut self) -> Result<(), Error> {
        self.stop.store(true, Ordering::Release);
        let until = deadline();
        while self.thread.as_ref().is_some_and(|t| !t.is_finished()) {
            if Instant::now() >= until {
                return Err(invalid("file owner shutdown timed out"));
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| invalid("file owner panicked"))??;
        }
        Ok(())
    }
}

pub(super) async fn session(
    machine: &Machine,
    store: &Handle,
    root: PathBuf,
    mut stop: watch::Receiver<bool>,
    mut closing: super::Closing,
    capture: Option<&crate::capture::Capture>,
) -> Result<(), Error> {
    let context = evidence::Connection::new(machine)?;
    let machine = &context;
    let (owner, mut files) = Owner::start(
        PathBuf::from(&machine.snapshot.effective().site().endpoint),
        root,
        u64::try_from(machine.snapshot.effective().machine().syntax.bytes)?,
        capture.map(|c| {
            Box::new(c.factory())
                as Box<dyn FnMut() -> Option<analyte_transport::recording::Tap> + Send>
        }),
    )?;
    let result = async {
        loop {
            let item = tokio::select! {
                biased;
                () = super::stopping(&mut stop) => break,
                item = files.recv() => item.ok_or_else(|| invalid("file owner stopped"))?,
            };
            let id = ReceiptId::new();
            evidence::receive(store, machine, id, item.bytes.clone()).await?;
            let accepted = evidence::interpret(store, machine, id).await?;
            item.stored
                .send(accepted)
                .map_err(|_| invalid("file completion reply lost; source remains claimed"))?;
        }
        Ok(())
    }
    .await;
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    owner.close().await?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // AGENTS.md: keep errors specific; clinical safety rule 10: no payloads in logs.
    #[test]
    fn safety_rule_10_drop_reports_worker_error_and_panic_without_payloads() {
        for panic in [false, true] {
            let worker = thread::spawn(move || {
                if panic {
                    std::panic::panic_any("protected-panic-token");
                }
                Err(invalid("protected-error-token"))
            });
            let output = super::super::worker::tests::capture(|| {
                drop(Owner {
                    stop: Arc::new(AtomicBool::new(false)),
                    thread: Some(worker),
                });
            });
            assert!(output.contains("worker=\"folder\""), "{output}");
            assert!(output.contains(if panic {
                "worker_panic"
            } else {
                "worker_error"
            }));
            assert!(!output.contains("protected-"));
        }
    }
}
