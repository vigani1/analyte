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
    time::Duration,
};

use tokio::time::Instant;

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
    marker_worker: Option<thread::JoinHandle<()>>,
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
            marker_worker: Some(marker_worker),
            state: Arc::new(Mutex::new(State {
                threads: Vec::new(),
            })),
            directory,
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
            if state.threads.len() >= 15 {
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
    tracing::error!(machine, capture = %path.display(), reason, "conversation capture failed");
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

impl Capture {
    /// Drains all captures against one cutoff after the transport owners exit.
    /// Cancellation retains ownership. On timeout the armed process watchdog
    /// enforces the final deadline while the marker workers try to flag the files.
    pub(crate) async fn close_all(captures: &mut [Self], deadline: Instant) {
        loop {
            let mut finished = true;
            for capture in &mut *captures {
                finished &= capture.reap();
            }
            if finished {
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                for capture in captures {
                    capture.flag_unfinished();
                }
                // The watchdog performs the platform stop notification and exits
                // with code 3 even if a recorder or marker write is still blocked.
                std::future::pending::<()>().await;
                return;
            }
            let next_poll = now
                .checked_add(Duration::from_millis(1))
                .unwrap_or(deadline);
            tokio::time::sleep_until(deadline.min(next_poll)).await;
        }
    }

    fn reap(&mut self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        for (_, worker) in state
            .threads
            .extract_if(.., |(_, worker)| worker.is_finished())
        {
            if worker.join().is_err() {
                tracing::error!(machine = self.machine, "capture worker panicked");
            }
        }
        if !state.threads.is_empty() {
            return false;
        }
        drop(state);
        self.failures.take();
        if self
            .marker_worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return false;
        }
        if let Some(worker) = self.marker_worker.take()
            && worker.join().is_err()
        {
            tracing::error!(machine = self.machine, "capture marker worker panicked");
        }
        true
    }

    fn flag_unfinished(&mut self) {
        if let Ok(state) = self.state.lock() {
            for (path, _) in &state.threads {
                tracing::error!(
                    machine = self.machine,
                    capture = %path.display(),
                    "capture shutdown exceeded its budget; terminating host"
                );
                if self.failures.as_ref().is_none_or(|sender| {
                    sender
                        .try_send((path.clone(), "capture shutdown deadline reached".into()))
                        .is_err()
                }) {
                    tracing::error!(
                        machine = self.machine,
                        "capture failure marker queue unavailable"
                    );
                }
            }
        }
        if self
            .marker_worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            tracing::error!(
                machine = self.machine,
                "capture marker worker still active at shutdown cutoff"
            );
        }
        self.failures.take();
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        // Host ownership must survive machine-task cancellation. An unclosed owner
        // is a host lifecycle defect; never silently detach workers.
        if self.marker_worker.is_some()
            || self
                .state
                .lock()
                .map_or(true, |state| !state.threads.is_empty())
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
        let capture = Arc::new(capture_with_worker(PathBuf::new(), worker, Some(sender)));
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
        Capture::close_all(
            std::slice::from_mut(&mut capture),
            Instant::now() + Duration::from_secs(5),
        )
        .await;
        Ok(())
    }

    fn capture_with_worker(
        path: PathBuf,
        worker: thread::JoinHandle<()>,
        failures: Option<std::sync::mpsc::SyncSender<(PathBuf, String)>>,
    ) -> Capture {
        Capture {
            state: Arc::new(Mutex::new(State {
                threads: vec![(path, worker)],
            })),
            directory: PathBuf::new(),
            reserve: 0,
            header: Header {
                version: 1,
                stack: std::array::from_fn(|_| "synthetic".into()),
                machine: None,
                software: None,
                evidence: Evidence::Synthetic,
                de_identification: "synthetic".into(),
                source: "test".into(),
                exceptions: Vec::new(),
            },
            machine: "synthetic".into(),
            failures,
            marker_worker: None,
        }
    }

    // ADR 0009: the final event can be synchronized beyond the former five-second limit.
    #[tokio::test]
    async fn slow_capture_finishes_and_replays() -> Result<(), crate::Error> {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let root = tempfile::tempdir()?;
        let path = root.path().join("slow.rec.jsonl");
        let (tap, worker) = recording::recording(path.clone(), 4)?;
        let at = std::time::Instant::now();
        tap.record(recording::Kind::In, at, b"result")?;
        tap.record(recording::Kind::Out, at, b"ack")?;
        tap.record(recording::Kind::Close, at, b"shutdown")?;

        let (start, gate) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let mut writes = 0;
            worker
                .run_guarded(|| {
                    writes += 1;
                    if writes == 4 {
                        gate.recv_timeout(Duration::from_secs(10))
                            .map_err(io::Error::other)?;
                        thread::sleep(Duration::from_millis(5_200));
                    }
                    Ok(())
                })
                .unwrap_or_else(|error| panic!("synthetic recorder failed: {error}"));
        });
        let mut capture = [capture_with_worker(path.clone(), worker, None)];
        let watchdog = crate::runtime::shutdown::Watchdog::new(Duration::from_secs(30), None)?;
        let deadline = watchdog.arm();
        drop(tap);
        start.send(())?;
        Capture::close_all(&mut capture, (deadline - Duration::from_secs(5)).into()).await;
        let (header, events) =
            recording::read(&std::fs::read(path)?[..], recording::Limits::default())?;
        assert_eq!(events.len(), 3);
        assert_eq!(
            events.last().and_then(|event| event.link),
            Some(recording::Link::Disconnect)
        );

        let (mut machine, mut host) = tokio::io::duplex(32);
        let peer = async {
            let mut bytes = [0; 6];
            host.read_exact(&mut bytes).await?;
            assert_eq!(&bytes, b"result");
            host.write_all(b"ack").await?;
            assert_eq!(host.read(&mut bytes).await?, 0);
            Ok::<_, io::Error>(())
        };
        let replay = analyte_sim::replay(
            &mut machine,
            &events,
            &header.exceptions,
            Duration::from_secs(2),
        );
        let (replayed, replied) =
            tokio::time::timeout(Duration::from_secs(3), async { tokio::join!(replay, peer) })
                .await?;
        replayed?;
        replied?;
        Ok(())
    }

    // ADR 0009 and AGENTS.md concurrency: a blocked writer is flagged and cannot
    // keep the process alive. The child isolates the production code-3 exit.
    #[test]
    fn stuck_capture_is_marked_and_exits_by_watchdog() -> Result<(), crate::Error> {
        const CHILD: &str = "ANALYTE_STUCK_CAPTURE_ROOT";
        if let Some(root) = std::env::var_os(CHILD) {
            let directory = PathBuf::from(root).join("captures/synthetic");
            tracing_subscriber::fmt().with_writer(io::stderr).init();
            let worker = thread::spawn(|| {
                loop {
                    thread::park();
                }
            });
            let (sender, receiver) = std::sync::mpsc::sync_channel::<(PathBuf, String)>(16);
            let mut capture = [capture_with_worker(
                directory.join("stuck.rec.jsonl"),
                worker,
                Some(sender),
            )];
            capture[0].marker_worker = Some(thread::spawn(move || {
                for (path, reason) in receiver {
                    failure(&directory, "synthetic", &path, &reason);
                }
            }));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let watchdog = crate::runtime::shutdown::Watchdog::new(Duration::from_secs(2), None)?;
            let _component = watchdog.component("capture".into());
            let deadline = watchdog.arm();
            runtime.block_on(Capture::close_all(
                &mut capture,
                (deadline - Duration::from_millis(1500)).into(),
            ));
            return Err("stuck capture unexpectedly closed".into());
        }

        let root = tempfile::tempdir()?;
        let started = std::time::Instant::now();
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "machine::link::capture::tests::stuck_capture_is_marked_and_exits_by_watchdog",
                "--nocapture",
            ])
            .env(CHILD, root.path())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        while child.try_wait()?.is_none() {
            if started.elapsed() >= Duration::from_secs(8) {
                child.kill()?;
                child.wait()?;
                return Err("capture watchdog did not terminate the child".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output()?;
        assert_eq!(output.status.code(), Some(3));
        let log = String::from_utf8(output.stderr)?;
        assert!(log.contains("capture shutdown exceeded its budget"));
        assert!(log.contains("shutdown bound exceeded"));
        for marker in ["capture-synthetic.failure", "stuck.rec.jsonl.failed"] {
            assert!(
                std::fs::read_to_string(root.path().join(marker))?
                    .contains("capture shutdown deadline reached")
            );
        }
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
        runtime.block_on(async {
            Capture::close_all(
                std::slice::from_mut(&mut capture),
                Instant::now() + Duration::from_secs(5),
            )
            .await;
        });
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
