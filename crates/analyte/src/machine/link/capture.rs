//! Captures optional protected wire recordings independently of clinical custody.
use crate::{installation::config::Machine, runtime::service::create_data_directory};
use analyte_transport::{
    IoSide,
    recording::{self, Evidence, Header},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

struct State {
    threads: Vec<(PathBuf, thread::JoinHandle<()>)>,
}

/// Owns every recording worker until its tap closes. Shutdown's watchdog bounds stuck I/O.
pub(crate) struct Capture {
    state: Arc<Mutex<State>>,
    directory: PathBuf,
    reserve: u64,
    header: Header,
    machine: String,
    failures: Option<std::sync::mpsc::SyncSender<(PathBuf, String)>>,
}

impl Capture {
    pub(crate) fn new(machine: &Machine, root: &Path, reserve: u64) -> Option<Self> {
        if !machine.capture {
            return None;
        }
        let stack = machine.snapshot.effective().stack();
        let directory = root.join("captures").join(machine.id.as_str());
        let (failures, reports) = std::sync::mpsc::sync_channel::<(PathBuf, String)>(16);
        let failure_root = directory.clone();
        let failure_machine = machine.id.as_str().to_owned();
        let marker_worker = match thread::Builder::new().name("capture-markers".into()).spawn(
            move || {
                while let Ok((path, reason)) = reports.recv() {
                    failure(&failure_root, &failure_machine, &path, &reason);
                }
            },
        ) {
            Ok(worker) => worker,
            Err(error) => {
                tracing::error!(error_kind = ?error.kind(), "capture marker worker unavailable; capture disabled");
                return None;
            }
        };
        Some(Self {
            failures: Some(failures),
            state: Arc::new(Mutex::new(State {
                threads: vec![(directory.clone(), marker_worker)],
            })),
            directory: root.join("captures").join(machine.id.as_str()),
            reserve,
            header: Header {
                version: 1,
                stack: stack.layer_names().map(str::to_owned),
                machine: Some(machine.id.as_str().to_owned()),
                software: None,
                evidence: Evidence::Machine,
                de_identification: "NOT de-identified".into(),
                source: "Host transport wire tap".into(),
                exceptions: Vec::new(),
            },
            machine: machine.id.as_str().to_owned(),
        })
    }

    pub(crate) fn attach(&self, side: IoSide) -> IoSide {
        side.with_recording_factory(self.factory())
    }

    #[expect(
        clippy::expect_used,
        reason = "only capture thread bookkeeping is locked; no fallible operation panics while holding it"
    )]
    pub(crate) fn factory(&self) -> impl FnMut() -> Option<recording::Tap> + Send + Sync + 'static {
        let failures = self.failures.clone();
        let state = self.state.clone();
        let directory = self.directory.clone();
        let reserve = self.reserve;
        let header = self.header.clone();
        let machine = self.machine.clone();
        move || {
            let path = directory.join(format!("{}.rec.jsonl", uuid::Uuid::now_v7()));
            let (tap, worker) = match recording::with_header(path.clone(), 64, header.clone()) {
                Ok(pair) => pair,
                Err(error) => {
                    report(&failures, &machine, &path, &error.to_string());
                    return None;
                }
            };
            let mut state = state
                .lock()
                .expect("capture worker ownership mutex is never poisoned");
            let mut pending = Vec::new();
            for (path, worker) in state.threads.drain(..) {
                if worker.is_finished() {
                    if worker.join().is_err() {
                        report(&failures, &machine, &path, "capture worker panicked");
                    }
                } else {
                    pending.push((path, worker));
                }
            }
            state.threads = pending;
            if state.threads.len() >= 16 {
                report(
                    &failures,
                    &machine,
                    &path,
                    "capture worker capacity exceeded",
                );
                return None;
            }
            let root = directory.clone();
            let id = machine.clone();
            let worker_path = path.clone();
            match thread::Builder::new()
                .name("analyte-capture".into())
                .spawn(move || {
                    let result = prepare(&root).and_then(|()| {
                        worker
                            .run_guarded(|| {
                                let margin = u64::try_from(recording::Limits::default().file_bytes)
                                    .map_err(io::Error::other)?;
                                let needed = reserve
                                    .checked_add(margin)
                                    .ok_or_else(|| io::Error::other("capture reserve overflow"))?;
                                if fs4::available_space(&root)? < needed {
                                    return Err(io::Error::other(
                                        "capture stopped at clinical storage reserve",
                                    ));
                                }
                                Ok(())
                            })
                            .map_err(io::Error::other)
                    });
                    if let Err(error) = result {
                        failure(&root, &id, &path, &error.to_string());
                    }
                }) {
                Ok(worker) => {
                    state.threads.push((worker_path, worker));
                    Some(tap)
                }
                Err(error) => {
                    report(
                        &failures,
                        &machine,
                        &directory,
                        &format!("capture worker start: {error}"),
                    );
                    None
                }
            }
        }
    }
}

fn prepare(directory: &Path) -> io::Result<()> {
    let parent = directory
        .parent()
        .ok_or_else(|| io::Error::other("capture parent absent"))?;
    for path in [parent, directory] {
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(io::Error::other(
                "capture directory must not be a symbolic link",
            ));
        }
    }
    create_data_directory(parent).map_err(io::Error::other)?;
    create_data_directory(directory).map_err(io::Error::other)
}

fn failure(directory: &Path, machine: &str, path: &Path, reason: &str) {
    // Rule 10: only recorder diagnostics and opaque filenames, never bytes or settings.
    tracing::error!(machine, capture = %path.display(), reason, "conversation capture failed; clinical traffic continues");
    let text = format!(
        "capture={} failed: {reason}\n",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    // A marker per failed connection keeps an incomplete recording visibly failed.
    if let Some(root) = directory.parent().and_then(Path::parent) {
        let marker = root.join(format!("capture-{machine}.failure"));
        if let Err(error) = private_write(&marker, text.as_bytes()) {
            tracing::error!(machine, error_kind = ?error.kind(), "capture failure status could not be persisted");
        }
        let marker = root.join(format!(
            "{}.failed",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if let Err(error) = private_write(&marker, text.as_bytes()) {
            tracing::error!(machine, error_kind = ?error.kind(), "capture connection failure marker could not be persisted");
        }
    }
}

fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_data()
}

fn report(
    sender: &Option<std::sync::mpsc::SyncSender<(PathBuf, String)>>,
    machine: &str,
    path: &Path,
    reason: &str,
) {
    tracing::error!(
        machine,
        reason,
        "capture failed; clinical traffic continues"
    );
    if sender.as_ref().is_none_or(|sender| {
        sender
            .try_send((path.to_owned(), reason.to_owned()))
            .is_err()
    }) {
        tracing::error!(machine, "capture failure marker queue unavailable");
    }
}

pub(crate) const SHUTDOWN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

impl Capture {
    /// Called by the host after all transport owners exit. Cancellation retains ownership.
    /// A stuck OS write cannot be cancelled: terminate visibly at the deadline,
    /// as with the service watchdog, rather than detach it or block a runtime worker.
    pub(crate) async fn close(&mut self, budget: std::time::Duration) {
        self.failures.take();
        let until = tokio::time::Instant::now()
            .checked_add(budget)
            .unwrap_or_else(tokio::time::Instant::now);
        loop {
            let finished = self
                .state
                .lock()
                .is_ok_and(|state| state.threads.iter().all(|(_, worker)| worker.is_finished()));
            if finished {
                break;
            }
            if tokio::time::Instant::now() >= until {
                tracing::error!(
                    machine = self.machine,
                    ?budget,
                    "capture shutdown exceeded its budget; terminating host"
                );
                std::process::exit(3);
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        if let Ok(mut state) = self.state.lock() {
            for (_, worker) in state.threads.drain(..) {
                if worker.join().is_err() {
                    tracing::error!(machine = self.machine, "capture worker panicked");
                }
            }
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        // Host ownership must survive machine-task cancellation. An unclosed owner
        // is a host lifecycle defect; never silently detach workers.
        if self
            .state
            .lock()
            .is_ok_and(|state| !state.threads.is_empty())
        {
            tracing::error!(
                machine = self.machine,
                "capture owner dropped without async close"
            );
            std::process::exit(3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_transport::{Backoff, Event, Limits, connection, serial};
    use std::{
        collections::VecDeque,
        io::{Read, Write},
        time::Duration,
    };

    // ADR 0009: optional capture admission and machine exit never wait for disk.
    // ADR 0009
    #[tokio::test]
    async fn stalled_capture_never_blocks_admission_or_exit() -> Result<(), crate::Error> {
        let (release, gate) = std::sync::mpsc::channel();
        let (started, entered) = tokio::sync::oneshot::channel();
        let worker = thread::spawn(move || {
            let _ = started.send(());
            let _ = gate.recv();
        });
        entered.await?;
        let (sender, _receiver) = std::sync::mpsc::sync_channel(1);
        sender.try_send((PathBuf::new(), "occupied".to_owned()))?;
        let header = Header {
            version: 1,
            stack: std::array::from_fn(|_| "synthetic".into()),
            machine: None,
            software: None,
            evidence: Evidence::Synthetic,
            de_identification: "synthetic".into(),
            source: "test".into(),
            exceptions: Vec::new(),
        };
        let capture = Arc::new(Capture {
            state: Arc::new(Mutex::new(State {
                threads: vec![(PathBuf::new(), worker)],
            })),
            directory: PathBuf::new(),
            reserve: 0,
            header,
            machine: "synthetic".into(),
            failures: Some(sender),
        });
        let machine_capture = capture.clone();
        let task = tokio::spawn(async move {
            let mut factory = machine_capture.factory();
            assert!(factory().is_none());
        });
        // Current-thread runtime: this cannot finish if admission or Drop joins the gate.
        tokio::time::timeout(Duration::from_secs(1), task).await??;
        assert!(
            !capture.state.lock().map_err(|_| "mutex")?.threads[0]
                .1
                .is_finished()
        );
        release.send(())?;
        let mut capture = Arc::try_unwrap(capture).map_err(|_| "capture owner")?;
        capture.close(SHUTDOWN_BUDGET).await;
        Ok(())
    }

    struct Echo(VecDeque<u8>);
    impl Read for Echo {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                // Synthetic serial Read models a timed-out port; the transport owns cancellation.
                thread::sleep(Duration::from_millis(1));
                return Err(io::ErrorKind::TimedOut.into());
            }
            let count = output.len().min(self.0.len());
            for byte in output.iter_mut().take(count) {
                *byte = self.0.pop_front().expect("count bounded by queue");
            }
            Ok(count)
        }
    }
    impl Write for Echo {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            self.0.extend(input);
            Ok(input.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // Plan §10 / ADR 0009 / rule 10: program-owned capture using the serial test seam.
    // Plan §10; ADR 0009
    #[test]
    fn serial_capture_equals_wire() -> Result<(), crate::Error> {
        let root = tempfile::tempdir()?;
        create_data_directory(&root.path().join("data"))?;
        analyte_fixtures::write_definition(
            root.path(),
            "synthetic",
            include_str!("../../../../core/config/examples/astm.toml"),
        )?;
        std::fs::write(
            root.path().join("site.toml"),
            include_str!("../../../../core/config/examples/site.toml"),
        )?;
        let program = root.path().join("program.toml");
        std::fs::write(
            &program,
            analyte_fixtures::program(
                "[[machines]]\nid='serial'\ncapture=true\ndefinition='test/synthetic/machine.toml'\nsite='site.toml'\n",
            ),
        )?;
        let program = crate::installation::config::Program::load(&program)?;
        let mut capture = Capture::new(
            program.machines().first().ok_or("machine")?,
            &root.path().join("data"),
            1024,
        )
        .ok_or("capture")?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let settings = serial::Settings::new(
            serial::Device::Named("synthetic".into()),
            9600,
            serial::DataBits::Eight,
            serial::Parity::None,
            serial::StopBits::One,
            serial::FlowControl::None,
            Duration::from_millis(5),
        )?;
        let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(2))?);
        let side = capture.attach(side);
        let owner = thread::spawn(move || {
            serial::run(
                &settings,
                side,
                Backoff::new(Duration::from_millis(5), Duration::from_millis(20), 1)
                    .expect("valid backoff"),
                |device| serial::locate(device, || Err(io::ErrorKind::Unsupported.into())),
                |_, _| Ok(Box::new(Echo(VecDeque::new()))),
            )
        });
        let result = runtime.block_on(async {
            assert!(matches!(dialog.receive().await?, Some(Event::Opened { .. })));
            dialog.write(b"SYN-0001\0\xff".to_vec()).await?;
            assert!(matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"SYN-0001\0\xff"));
            Ok::<_, crate::Error>(())
        });
        dialog.cancellation().cancel();
        runtime.block_on(async {
            assert!(matches!(dialog.receive().await?, Some(Event::Closed(_))));
            Ok::<_, crate::Error>(())
        })?;
        drop(dialog);
        owner.join().map_err(|_| "serial test owner panic")??;
        result?;
        runtime.block_on(capture.close(SHUTDOWN_BUDGET));
        drop(capture);
        let file = std::fs::read_dir(root.path().join("data/captures/serial"))?
            .next()
            .ok_or("recording")??
            .path();
        let (_, events) = recording::read(&std::fs::read(file)?[..], recording::Limits::default())?;
        for direction in [
            recording::Direction::HostToMachine,
            recording::Direction::MachineToHost,
        ] {
            let bytes: Vec<_> = events
                .iter()
                .filter(|e| e.link.is_none() && e.direction == direction)
                .flat_map(|e| e.bytes.iter().copied())
                .collect();
            assert_eq!(bytes, b"SYN-0001\0\xff");
        }
        Ok(())
    }
}
