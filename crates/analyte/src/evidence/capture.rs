//! Plan §10 / ADR 0009: optional protected wire evidence, never clinical custody.
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
}

impl Capture {
    pub(crate) fn new(machine: &Machine, root: &Path, reserve: u64) -> Option<Self> {
        if !machine.capture {
            return None;
        }
        let stack = machine.snapshot.effective().stack();
        Some(Self {
            state: Arc::new(Mutex::new(State {
                threads: Vec::new(),
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
        let state = self.state.clone();
        let directory = self.directory.clone();
        let reserve = self.reserve;
        let header = self.header.clone();
        let machine = self.machine.clone();
        move || {
            let path = directory.join(format!("{}.rec.jsonl", uuid::Uuid::now_v7()));
            let (tap, worker) =
                match recording::recording_with_header(path.clone(), 64, header.clone()) {
                    Ok(pair) => pair,
                    Err(error) => {
                        failure(&directory, &machine, &path, &error.to_string());
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
                        failure(&directory, &machine, &path, "capture worker panicked");
                    }
                } else {
                    pending.push((path, worker));
                }
            }
            state.threads = pending;
            if state.threads.len() >= 16 {
                failure(
                    &directory,
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
                    failure(
                        &directory,
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

impl Drop for Capture {
    #[expect(
        clippy::expect_used,
        reason = "capture bookkeeping never panics while locked"
    )]
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .expect("capture ownership mutex is never poisoned");
        for (path, worker) in state.threads.drain(..) {
            if worker.join().is_err() {
                failure(
                    &self.directory,
                    &self.machine,
                    &path,
                    "capture worker panicked",
                );
            }
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
    #[test]
    fn plan10_adr0009_serial_capture_equals_wire() -> Result<(), crate::Error> {
        let root = tempfile::tempdir()?;
        create_data_directory(&root.path().join("data"))?;
        std::fs::write(
            root.path().join("machine.toml"),
            include_str!("../../../config/examples/astm.toml"),
        )?;
        std::fs::write(
            root.path().join("site.toml"),
            include_str!("../../../config/examples/site.toml"),
        )?;
        let program = root.path().join("program.toml");
        std::fs::write(
            &program,
            "schema_version=1\ndata_directory='data'\n[[machines]]\nid='serial'\ncapture=true\ndefinition='machine.toml'\nsite='site.toml'\n[store]\nreserve_bytes=1024\ndatabase_bytes=100000000\nrecord_bytes=2097152\npage_rows=100\nbusy_timeout_ms=1000\n",
        )?;
        let program = crate::installation::config::Program::load(&program)?;
        let capture = Capture::new(
            program.machines().first().ok_or("machine")?,
            &root.path().join("data"),
            1024,
        )
        .ok_or("capture")?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let settings = serial::Settings::new(
            "synthetic".into(),
            9600,
            serialport::DataBits::Eight,
            serialport::Parity::None,
            serialport::StopBits::One,
            serialport::FlowControl::None,
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
                |_| Ok(Box::new(Echo(VecDeque::new()))),
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
