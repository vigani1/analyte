//! Owns the serial session loop and its blocking port thread, joined on cancellation.
use super::error::Error as LinkError;
use super::{Kind, Next, recorded_connection, retry, session_ended};
use crate::machine::{Error, Handle, Limits, Machine};
use analyte_transport::{Backoff, Cancellation, IoSide, serial::Settings};
use std::{sync::Arc, thread, time::Duration};
use tokio::sync::watch;

struct Owner {
    cancellation: Cancellation,
    finished: tokio::sync::oneshot::Receiver<()>,
    thread: Option<thread::JoinHandle<Result<(), analyte_transport::Error>>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(thread) = self.thread.take() {
            crate::machine::worker::report_join("serial", &thread.join());
        }
    }
}

impl Owner {
    fn start(
        worker: &Worker,
        settings: &Settings,
        side: IoSide,
        retry: &Backoff,
        cancellation: Cancellation,
    ) -> Result<Self, std::io::Error> {
        let (worker, settings, retry) = (worker.clone(), settings.clone(), retry.clone());
        let (finished, completion) = tokio::sync::oneshot::channel();
        let thread = thread::Builder::new()
            .name("analyte-serial".into())
            .spawn(move || {
                let result = worker(settings, side, retry);
                let _ = finished.send(());
                result
            })?;
        Ok(Self {
            cancellation,
            finished: completion,
            thread: Some(thread),
        })
    }

    /// The port thread ends only when its event consumer is gone or backpressure
    /// exceeded its deadline; port faults themselves are events and reopen the port.
    fn worker_ended(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(thread::JoinHandle::is_finished)
    }

    async fn close(mut self) -> Result<(), LinkError> {
        self.cancellation.cancel();
        // Completion also wakes on panic; the retained join below reports its cause.
        // On timeout or cancellation Drop still owns and joins the worker.
        let _ = tokio::time::timeout_at(crate::deadline()?.into(), &mut self.finished)
            .await
            .map_err(LinkError::SerialOwnerShutdownTimedOut)?;
        if let Some(thread) = self.thread.take() {
            // Panic payloads may contain protected bytes; retain only the typed panic classification.
            thread
                .join()
                .map_err(|_| LinkError::SerialOwnerPanicked)??;
        }
        Ok(())
    }
}

/// Port thread body: production opens the configured OS device.
type Worker =
    Arc<dyn Fn(Settings, IoSide, Backoff) -> Result<(), analyte_transport::Error> + Send + Sync>;

pub(in crate::machine) async fn run(
    machine: &Machine,
    store: &Handle,
    link: crate::machine::settings::SerialLink<'_>,
    limits: Limits,
    stop: watch::Receiver<bool>,
    closing: crate::machine::Closing,
    capture: Option<&crate::machine::capture::Capture>,
) -> Result<(), Error> {
    let worker: Worker = Arc::new(|settings, side, retry| {
        analyte_transport::serial::run_system(&settings, side, retry)
    });
    serve(
        machine, store, link, limits, stop, closing, capture, &worker,
    )
    .await
}

// The worker is a test seam: tests supply scripted ports through the transport's
// own `serial::run`, never an operating system device.
#[expect(
    clippy::too_many_arguments,
    reason = "the production entry point adds only the worker seam to the owner inputs"
)]
async fn serve(
    machine: &Machine,
    store: &Handle,
    link: crate::machine::settings::SerialLink<'_>,
    limits: Limits,
    mut stop: watch::Receiver<bool>,
    mut closing: crate::machine::Closing,
    capture: Option<&crate::machine::capture::Capture>,
    worker: &Worker,
) -> Result<(), Error> {
    // Invalid port settings mean no port can ever be opened.
    let settings = settings(machine, link.settings)?;
    // Fixed bounds; invalid only if the constants are wrong.
    let retry = retry()?;
    loop {
        let (mut conn, side) = recorded_connection(limits, capture);
        // A refused thread spawn leaves no port owner to serve any session.
        let owner = Owner::start(worker, &settings, side, &retry, conn.cancellation())?;
        // Sessions share this port owner while they end cleanly: a clean close lets the
        // port thread reopen the device and the next session sees its Opened event. A
        // failed session ends the owner instead, because hosts such as the Autolyser
        // start polling only on Opened and would wait silently on a reused connection.
        let result = loop {
            let result =
                crate::machine::session(machine, link.session, store, &mut conn, stop.clone())
                    .await;
            // A finished port thread can neither receive nor write again.
            if result.is_err() || owner.worker_ended() || *stop.borrow() {
                break result;
            }
        };
        if *stop.borrow() {
            closing.after_commits().await?;
        }
        drop(conn);
        let closed = match owner.close().await {
            // The thread reports the dropped connection while it stops; that is the clean end.
            Err(LinkError::Transport(analyte_transport::Error::ConsumerClosed)) => Ok(()),
            closed @ (Ok(()) | Err(LinkError::Transport(_))) => closed,
            // A panicked or unjoined port thread may still hold the device, so a new
            // owner could share it; the machine owner stops and the service reports it.
            Err(error) => {
                super::report(
                    machine.id.as_str(),
                    Kind::Serial,
                    result,
                    Ok::<(), LinkError>(()),
                );
                return Err(error.into());
            }
        };
        match session_ended(machine.id.as_str(), Kind::Serial, result, closed, &mut stop).await {
            Next::Serve => {}
            Next::Stop => return Ok(()),
            Next::StopIdle => {
                closing.after_commits().await?;
                return Ok(());
            }
        }
    }
}

fn settings(machine: &Machine, serial: &analyte_config::Serial) -> Result<Settings, Error> {
    let parity = match serial.parity {
        analyte_config::Parity::None => serialport::Parity::None,
        analyte_config::Parity::Odd => serialport::Parity::Odd,
        analyte_config::Parity::Even => serialport::Parity::Even,
    };
    let flow = match serial.flow_control {
        analyte_config::FlowControl::None => serialport::FlowControl::None,
        analyte_config::FlowControl::Hardware => serialport::FlowControl::Hardware,
        analyte_config::FlowControl::Software => serialport::FlowControl::Software,
    };
    Ok(Settings::new(
        machine.snapshot.effective().site().endpoint.clone(),
        serial.baud,
        serial
            .data_bits
            .try_into()
            .map_err(|()| LinkError::InvalidSerialDataBits)?,
        parity,
        serial
            .stop_bits
            .try_into()
            .map_err(|()| LinkError::InvalidSerialStopBits)?,
        flow,
        Duration::from_millis(100),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // AGENTS.md: keep errors specific; clinical safety rule 10: no payloads in logs.
    #[test]
    fn safety_rule_10_drop_reports_worker_error_and_panic_without_payloads() {
        for panic in [false, true] {
            let worker = thread::spawn(move || {
                if panic {
                    std::panic::panic_any("protected-panic-token");
                }
                Err(analyte_transport::Error::Io {
                    operation: "test",
                    endpoint: "protected-error-token".into(),
                    source: std::io::Error::other("protected-error-token"),
                })
            });
            let output = crate::machine::worker::tests::capture(|| {
                drop(Owner {
                    cancellation: Cancellation::default(),
                    finished: tokio::sync::oneshot::channel().1,
                    thread: Some(worker),
                });
            });
            assert!(output.contains("worker=\"serial\""), "{output}");
            assert!(output.contains(if panic {
                "worker_panic"
            } else {
                "worker_error"
            }));
            assert!(!output.contains("protected-"));
        }
    }

    use analyte_fixtures::UnlimitedSpace as Space;
    struct Port {
        input: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
        output: tokio::sync::mpsc::Sender<Vec<u8>>,
    }
    impl std::io::Read for Port {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let bytes = self
                .input
                .recv_timeout(Duration::from_millis(10))
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))??;
            if bytes.len() > buffer.len() {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
    }
    impl std::io::Write for Port {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.output
                .blocking_send(bytes.to_vec())
                .map_err(std::io::Error::other)?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn port_settings(name: &str) -> Result<Settings, Error> {
        Ok(Settings::new(
            name.into(),
            9600,
            serialport::DataBits::Eight,
            serialport::Parity::None,
            serialport::StopBits::One,
            serialport::FlowControl::None,
            Duration::from_millis(10),
        )?)
    }

    fn serial_program(
        root: &std::path::Path,
        machine: &str,
    ) -> Result<crate::installation::config::Program, Error> {
        std::fs::write(root.join("machine.toml"), machine)?;
        std::fs::write(
            root.join("site.toml"),
            include_str!("../../../../core/config/examples/site.toml"),
        )?;
        let path = root.join("program.toml");
        std::fs::write(
            &path,
            analyte_fixtures::program_with_limits(
                "[[machines]]\nid='synthetic'\ndefinition='machine.toml'\nsite='site.toml'\n",
                &analyte_store::Limits {
                    reserve_bytes: 1,
                    database_bytes: 100000000,
                    record_bytes: 2097152,
                    page_rows: 100,
                    busy_timeout: Duration::from_millis(100),
                },
            ),
        )?;
        crate::installation::config::Program::load(&path).map_err(Into::into)
    }

    // ADR 0003 / ADR 0010: run the actual ASTM host dialog over the existing
    // serial factory seam. Every byte and failure is synthetic, never an OS port.
    #[tokio::test(flavor = "multi_thread")]
    async fn adr0003_adr0010_serial_timeout_split_merged_frames_store_before_ack_and_disconnect()
    -> Result<(), Error> {
        let root = crate::admin::private_tempdir()?;
        let program = serial_program(
            root.path(),
            include_str!("../../../../core/config/examples/astm.toml"),
        )?;
        let until = || std::time::Instant::now() + Duration::from_secs(30);
        let mut store =
            crate::runtime::service::open_store_with_deadline(&program, Box::new(Space), until())
                .await?;
        let machine = program.machines().first().ok_or("missing machine")?;
        let crate::machine::settings::Link::Serial(link) =
            crate::machine::settings::transport(machine.snapshot.effective().stack())
        else {
            return Err("expected serial".into());
        };
        let settings = port_settings("synthetic-only")?;
        let (mut connection, side) =
            analyte_transport::connection(Limits::new(16, Duration::from_secs(2))?);
        let cancellation = connection.cancellation();
        let (input, received) = std::sync::mpsc::sync_channel(16);
        let (output, mut writes) = tokio::sync::mpsc::channel(16);
        let (opened, mut attempts) = tokio::sync::mpsc::channel(16);
        let worker = thread::spawn(move || {
            let mut port = Some(Port {
                input: received,
                output,
            });
            analyte_transport::serial::run(
                &settings,
                side,
                retry().expect("valid retry"),
                move |_| {
                    opened.blocking_send(()).map_err(std::io::Error::other)?;
                    port.take()
                        .map(|p| Box::new(p) as Box<dyn analyte_transport::serial::Port>)
                        .ok_or_else(|| std::io::ErrorKind::PermissionDenied.into())
                },
            )
        });
        let (stop, stopping) = watch::channel(false);
        let processing =
            crate::machine::session(machine, link.session, &store, &mut connection, stopping);
        let conversation = async {
            attempts.recv().await.ok_or("open absent")?;
            input.send(Err(std::io::ErrorKind::TimedOut.into()))?;
            input.send(Ok(vec![5]))?;
            assert_eq!(writes.recv().await.ok_or("ENQ ACK absent")?, vec![6]);
            let frame: Vec<u8> = analyte_framing::astm::encode(
                b"H|\\^&\r",
                analyte_framing::astm::FrameNumber::FIRST,
                analyte_framing::astm::Limit::new(64000)?,
            )?
            .flatten()
            .collect();
            let second: Vec<u8> = analyte_framing::astm::encode(
                b"L|1|N\r",
                analyte_framing::astm::FrameNumber::FIRST.next(),
                analyte_framing::astm::Limit::new(64000)?,
            )?
            .flatten()
            .collect();
            input.send(Ok(frame[..3].to_vec()))?;
            let mut merged = frame[3..].to_vec();
            merged.extend_from_slice(&second);
            input.send(Ok(merged))?;
            for expected in [&frame, &second] {
                assert_eq!(writes.recv().await.ok_or("frame ACK absent")?, vec![6]);
                let db = rusqlite::Connection::open_with_flags(
                    program.data_directory().join("analyte.db"),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                let count: i64 = db.query_row(
                    "SELECT count(*) FROM frames WHERE raw = ?1",
                    [expected],
                    |r| r.get(0),
                )?;
                assert_eq!(count, 1, "frame must be durable when ACK reaches the port");
            }
            input.send(Err(std::io::ErrorKind::BrokenPipe.into()))?;
            // A reopen attempt proves the disconnect was consumed. Fail the open
            // explicitly, without asking the OS to open a device.
            attempts.recv().await.ok_or("reopen absent")?;
            stop.send_replace(true);
            Ok::<_, Error>(())
        };
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(processing, conversation)
        })
        .await;
        cancellation.cancel();
        drop(connection);
        let worker_result = worker.join().map_err(|_| "serial worker panic")?;
        assert!(matches!(
            worker_result,
            Ok(()) | Err(analyte_transport::Error::ConsumerClosed)
        ));
        let (processed, conversation) = result?;
        processed?;
        conversation?;
        store.shutdown(until()).await?;
        Ok(())
    }
    #[tokio::test]
    async fn plan11_serial_open_failure_is_specific_and_owner_is_joined() -> Result<(), Error> {
        let settings = port_settings("synthetic-refused-port")?;
        let (mut conn, side) =
            analyte_transport::connection(Limits::new(16, Duration::from_secs(2))?);
        let cancellation = conn.cancellation();
        let worker = thread::spawn(move || {
            analyte_transport::serial::run(
                &settings,
                side,
                retry().expect("valid retry"),
                |settings| {
                    assert_eq!(settings.name(), "synthetic-refused-port");
                    Err(std::io::ErrorKind::PermissionDenied.into())
                },
            )
        });
        let event = conn.receive().await;
        cancellation.cancel();
        let joined = worker.join().map_err(|_| "serial owner panic")?;
        assert!(matches!(
            event?,
            Some(analyte_transport::Event::Failed(
                analyte_transport::Reason::Io {
                    operation: "open serial",
                    kind: std::io::ErrorKind::PermissionDenied,
                    ..
                }
            ))
        ));
        joined?;
        Ok(())
    }

    // Review Y1: one failed session must not end the serial owner. VIDAS rejects a
    // control byte without its configured suffix (bioMérieux VIDAS Literal protocol,
    // control suffix); the rejected byte is parked, the port owner is replaced, and
    // the next session answers ENQ on a freshly opened port.
    #[test]
    fn y1_serial_failed_session_then_next_session_is_served() -> Result<(), Error> {
        use analyte_fixtures::ConfigEdit as _;
        let root = crate::admin::private_tempdir()?;
        let machine = include_str!("../../../../core/config/examples/vidas_literal.toml")
            .rewrite(
                "kind = \"tcp_server\"
",
                "kind = \"serial\"
",
            )
            .rewrite(
                "operation_timeout_ms = 10000
",
                "operation_timeout_ms = 10000
[transport.serial]
baud = 9600
data_bits = 8
                 parity = \"none\"
stop_bits = 1
flow_control = \"none\"
",
            )
            .rewrite("suffix = []", "suffix = [13, 10]")
            .rewrite("transport = \"tcp_server\"", "transport = \"serial\"");
        let program = serial_program(root.path(), &machine)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let mut outcome = Ok(());
        let logs = crate::machine::worker::tests::capture(|| {
            outcome = runtime.block_on(failed_then_served(&program));
        });
        outcome?;
        assert!(logs.contains("link=\"serial\""), "{logs}");
        assert!(logs.contains("machine session stopped"), "{logs}");
        assert!(
            logs.contains("error_kind=VIDAS control suffix mismatch"),
            "{logs}"
        );
        Ok(())
    }

    async fn failed_then_served(
        program: &crate::installation::config::Program,
    ) -> Result<(), Error> {
        let mut store = crate::runtime::service::open_store_with_deadline(
            program,
            Box::new(Space),
            crate::deadline()?,
        )
        .await?;
        let machine = program.machines().first().ok_or("missing machine")?;
        let crate::machine::settings::Link::Serial(link) =
            crate::machine::settings::transport(machine.snapshot.effective().stack())
        else {
            return Err("expected serial".into());
        };
        let (opened, mut attempts) = tokio::sync::mpsc::channel(4);
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut ports = std::collections::VecDeque::new();
        for _ in 0..2 {
            let (input, received) = std::sync::mpsc::sync_channel(4);
            let (output, writes) = tokio::sync::mpsc::channel(4);
            inputs.push(input);
            outputs.push(writes);
            ports.push_back(Port {
                input: received,
                output,
            });
        }
        let ports = std::sync::Mutex::new(ports);
        let worker: Worker = Arc::new(move |settings, side, retry| {
            let opened = opened.clone();
            analyte_transport::serial::run(&settings, side, retry, |_| {
                let port = ports
                    .lock()
                    .map_err(|_| std::io::Error::other("port lock"))?
                    .pop_front();
                let port = port.ok_or(std::io::ErrorKind::PermissionDenied)?;
                opened.blocking_send(()).map_err(std::io::Error::other)?;
                Ok(Box::new(port) as Box<dyn analyte_transport::serial::Port>)
            })
        });
        let (stop, stopping) = watch::channel(false);
        let (ready, _ready) = watch::channel(false);
        let (_close, close) = watch::channel(true);
        let owner = serve(
            machine,
            &store,
            link,
            Limits::new(16, Duration::from_secs(2))?,
            stopping,
            crate::machine::Closing { ready, close },
            None,
            &worker,
        );
        let conversation = async {
            let [first, second] = inputs.as_slice() else {
                return Err("two ports".into());
            };
            attempts.recv().await.ok_or("first open absent")?;
            first.send(Ok(vec![5, b'X']))?;
            // The failed session closes the first port; only a new owner opens the second.
            attempts.recv().await.ok_or("second open absent")?;
            second.send(Ok(vec![5, 13, 10]))?;
            let reply = outputs[1].recv().await.ok_or("second session ACK absent")?;
            assert_eq!(reply, vec![6, 13, 10], "next session acknowledges ENQ");
            assert!(
                outputs[0].try_recv().is_err(),
                "failed session wrote nothing"
            );
            stop.send_replace(true);
            Ok::<_, Error>(())
        };
        let (served, conversation) = tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
            tokio::join!(owner, conversation)
        })
        .await?;
        conversation?;
        served?;
        let db = rusqlite::Connection::open_with_flags(
            program.data_directory().join("analyte.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let received: i64 = db.query_row(
            "SELECT count(*) FROM receipts WHERE raw = ?1",
            [&[5_u8, b'X'][..]],
            |r| r.get(0),
        )?;
        assert_eq!(received, 1, "the failed session's input stays stored");
        let parked: i64 = db.query_row("SELECT count(*) FROM parked", [], |r| r.get(0))?;
        assert!(parked >= 1, "the rejected byte is parked with its reason");
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }
}
