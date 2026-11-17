//! Receives claimed files and retains the claim until receipt and interpretation are durable.
use super::error::Error;
use crate::{deadline, installation::config::Machine, intake};
#[cfg(test)]
use analyte_fixtures::ConfigEdit as _;
use analyte_model::ReceiptId;
use analyte_store::Handle;
use analyte_transport::folder::{Disposition, Item, Watch};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use tokio::sync::watch;

struct Owner {
    stop: Arc<AtomicBool>,
    finished: tokio::sync::oneshot::Receiver<()>,
    thread: Option<thread::JoinHandle<Result<(), Error>>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            crate::machine::worker::report_join("folder", &thread.join());
        }
    }
}

impl Owner {
    fn start(
        incoming: PathBuf,
        root: PathBuf,
        limit: u64,
        recorder: Option<Box<dyn FnMut() -> Option<analyte_transport::recording::Tap> + Send>>,
    ) -> Result<(Self, tokio::sync::mpsc::Receiver<Item>), Error> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let (finished, completion) = tokio::sync::oneshot::channel();
        let thread = thread::Builder::new()
            .name("analyte-files".into())
            .spawn(move || {
                let result = Watch {
                    incoming,
                    root,
                    limit,
                }
                .run(&cancelled, &sender, recorder, || {
                    uuid::Uuid::now_v7().to_string()
                })
                .map_err(Error::from);
                let _ = finished.send(());
                result
            })?;
        Ok((
            Self {
                stop,
                finished: completion,
                thread: Some(thread),
            },
            receiver,
        ))
    }

    async fn close(mut self) -> Result<(), Error> {
        self.stop.store(true, Ordering::Release);
        // Completion also wakes on panic; the retained join below reports its cause.
        // On timeout or cancellation Drop still owns and joins the worker.
        let _ = tokio::time::timeout_at(deadline()?.into(), &mut self.finished)
            .await
            .map_err(Error::FileOwnerShutdownTimedOut)?;
        if let Some(thread) = self.thread.take() {
            // Panic payloads may contain protected bytes; retain only the typed panic classification.
            thread.join().map_err(|_| Error::FileOwnerPanicked)??;
        }
        Ok(())
    }
}

pub(in crate::machine) async fn session(
    machine: &Machine,
    store: &Handle,
    bytes: usize,
    root: PathBuf,
    mut stop: watch::Receiver<bool>,
    mut closing: crate::machine::Closing,
    capture: Option<&crate::machine::capture::Capture>,
) -> Result<(), Error> {
    let context = intake::ConnectionEvidence::new(machine)?;
    let machine = &context;
    let (owner, mut files) = Owner::start(
        PathBuf::from(&machine.machine().snapshot.effective().site().endpoint),
        root,
        u64::try_from(bytes)?,
        capture.map(|c| {
            Box::new(c.factory())
                as Box<dyn FnMut() -> Option<analyte_transport::recording::Tap> + Send>
        }),
    )?;
    let result = async {
        loop {
            let mut item = tokio::select! {
                biased;
                () = analyte_sink::stopped(&mut stop) => break,
                item = files.recv() => item.ok_or_else(|| Error::FileOwnerStopped)?,
            };
            let Some(raw) = receive(&mut item.connection, bytes, &mut stop).await? else {
                break;
            };
            let id = ReceiptId::new();
            intake::receive(store, machine, id, raw).await?;
            let accepted = intake::interpret(store, machine.machine(), id).await?;
            item.completed
                .send(if accepted {
                    Disposition::Archive
                } else {
                    Disposition::Quarantine
                })
                .map_err(Error::CompletionReplyLost)?;
        }
        Ok(())
    }
    .await;
    // Dropping queued connection endpoints unblocks a producer in deliver before joining it.
    drop(files);
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    owner.close().await?;
    result
}

// Closed(Disconnect) is the transport's complete-file boundary. A cancelled or
// truncated connection never authorizes archive/quarantine, even if it supplied a prefix.
async fn receive(
    connection: &mut analyte_transport::Connection,
    bound: usize,
    stop: &mut watch::Receiver<bool>,
) -> Result<Option<Vec<u8>>, Error> {
    use analyte_transport::{Event, Reason};
    let mut opened = false;
    let mut raw = Vec::new();
    loop {
        let event = tokio::select! {
            biased;
            () = analyte_sink::stopped(stop) => return Ok(None),
            event = connection.receive() => event?,
        }
        .ok_or_else(|| Error::CompletionInterrupted)?;
        match event {
            Event::Opened { .. } => {
                if opened {
                    return Err(Error::FileTransportOpenedTwice);
                }
                opened = true;
            }
            Event::Received { bytes, .. } => {
                if !opened
                    || raw
                        .len()
                        .checked_add(bytes.len())
                        .is_none_or(|size| size > bound)
                {
                    return Err(Error::FileTransportReceiptBoundExceeded);
                }
                raw.extend_from_slice(&bytes);
            }
            Event::Closed(Reason::Disconnect) if opened => return Ok(Some(raw)),
            Event::Closed(reason) | Event::Failed(reason) => {
                return Err(Error::FileInterrupted(reason));
            }
            Event::RecordingFailed(failure) => {
                tracing::warn!(error_kind = %failure, "optional recorder stopped; transport continues");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use std::time::Duration;

    // AGENTS.md: keep errors specific; clinical safety rule 10: no payloads in logs.
    #[test]
    fn safety_rule_10_drop_reports_worker_error_and_panic_without_payloads() {
        for panic in [false, true] {
            let worker = thread::spawn(move || {
                if panic {
                    std::panic::panic_any("protected-panic-token");
                }
                Err(super::Error::Io(std::io::Error::other(
                    "protected-error-token",
                )))
            });
            let output = crate::machine::worker::tests::capture(|| {
                drop(Owner {
                    stop: Arc::new(AtomicBool::new(false)),
                    finished: tokio::sync::oneshot::channel().1,
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
    #[tokio::test]
    async fn adr0003_folder_transport_waits_for_custody_and_lost_reply_keeps_claim()
    -> Result<(), Error> {
        for disposition in [
            Some(Disposition::Archive),
            Some(Disposition::Quarantine),
            None,
        ] {
            let temp = tempfile::tempdir()?;
            let incoming = temp.path().join("incoming");
            let root = temp.path().join("receipts");
            std::fs::create_dir(&incoming)?;
            let bytes = vec![0x7f; analyte_transport::CHUNK_LIMIT * 2 + 3];
            std::fs::write(incoming.join("synthetic"), &bytes)?;
            let (owner, mut files) = Owner::start(incoming.clone(), root.clone(), 16384, None)?;
            let mut item = tokio::time::timeout(Duration::from_secs(10), files.recv())
                .await?
                .ok_or("file connection absent")?;
            let (_stop, mut stop) = watch::channel(false);
            assert_eq!(
                receive(&mut item.connection, 16384, &mut stop)
                    .await?
                    .ok_or("file interrupted")?,
                bytes
            );
            assert_eq!(std::fs::read_dir(root.join("archive"))?.count(), 0);
            assert_eq!(std::fs::read_dir(root.join("quarantine"))?.count(), 0);
            let claim = std::fs::read_dir(root.join("claimed"))?
                .next()
                .ok_or("claim absent")??
                .path();
            assert_eq!(std::fs::read(claim.join("input"))?, bytes);
            if let Some(disposition) = disposition {
                item.completed
                    .send(disposition)
                    .map_err(|_| crate::invalid("custody reply lost"))?;
            }
            drop(item);
            drop(files);
            owner.close().await?;
            let area = match disposition {
                Some(Disposition::Archive) => "archive",
                Some(Disposition::Quarantine) => "quarantine",
                None => "claimed",
            };
            let retained = std::fs::read_dir(root.join(area))?
                .next()
                .ok_or("retained file absent")??
                .path();
            assert_eq!(std::fs::read(retained.join("input"))?, bytes);
        }
        Ok(())
    }

    #[tokio::test]
    async fn adr0003_file_astm_and_hl7_storage_refusal_never_release_claim() -> Result<(), Error> {
        for definition in [
            include_str!("../../../../core/config/examples/astm.toml"),
            include_str!("../../../../core/config/examples/hl7.toml"),
        ] {
            let temp = tempfile::tempdir()?;
            let incoming = temp.path().join("incoming");
            let root = temp.path().join("files");
            std::fs::create_dir(&incoming)?;
            let raw = b"synthetic complete file; storage must precede interpretation";
            std::fs::write(incoming.join("synthetic"), raw)?;
            let start = definition.find("[transport]").ok_or("transport section")?;
            let end = definition.find("[syntax]").ok_or("syntax section")?;
            let definition = format!(
                "{}[transport]\nkind='watched_folder'\noperation_timeout_ms=10000\n[framing]\nlimit=65536\n[dialog]\nreceive_timeout_ms=30000\nsend_timeout_ms=15000\n{}",
                &definition[..start],
                &definition[end..]
            );
            let site = include_str!("../../../../core/config/examples/site.toml").rewrite(
                "\"127.0.0.1:5600\"",
                &toml::Value::String(incoming.to_string_lossy().into_owned()).to_string(),
            );
            let snapshot = analyte_config::Draft::load(
                ("machine", &definition),
                ("site", &site),
                ("overrides", "schema_version=1"),
            )?
            .validate()?;
            let machine = Machine {
                id: analyte_model::Identifier::new("synthetic")?,
                snapshot,
                listener: None,
                capture: false,
                destination_limits: crate::runtime::delivery::limits(30),

                hooks: crate::installation::config::MachineHooks::default(),
            };
            let mut store = Handle::start(
                temp.path().join("store.db"),
                {
                    let mut limits = analyte_fixtures::store_limits();
                    limits.busy_timeout = Duration::from_millis(20);
                    limits
                },
                Box::new(analyte_fixtures::Space(0)),
                4,
                crate::test_startup_deadline()?,
            )
            .await?;
            let (_stop, stop) = watch::channel(false);
            let (ready, _ready) = watch::channel(false);
            let (_close, close) = watch::channel(false);
            let closing = crate::machine::Closing { ready, close };
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                session(&machine, &store, 65536, root.clone(), stop, closing, None),
            )
            .await?;
            assert!(matches!(
                result
                    .as_ref()
                    .err()
                    .and_then(|error| error.downcast_ref::<analyte_store::IntakeError>()),
                Some(analyte_store::IntakeError::Storage(
                    analyte_store::StorageError::Capacity
                ))
            ));
            assert_eq!(std::fs::read_dir(root.join("archive"))?.count(), 0);
            assert_eq!(std::fs::read_dir(root.join("quarantine"))?.count(), 0);
            let watcher = analyte_transport::folder::Watcher::new(
                incoming,
                root.join("claimed"),
                None,
                65536,
            )?;
            let recovered = watcher.recover()?;
            assert_eq!(recovered.len(), 1);
            assert_eq!(
                recovered.first().ok_or("claimed input absent")?.read()?,
                raw
            );
            assert!(
                store
                    .request(deadline()?, |s| s.receipts(""))
                    .await?
                    .is_empty()
            );
            store.shutdown(deadline()?).await?;
        }
        Ok(())
    }
}
