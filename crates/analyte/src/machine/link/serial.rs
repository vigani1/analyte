//! Owns the serial session loop and its blocking port thread, joined on cancellation.
use super::error::Error as LinkError;
use super::{Kind, Next, recorded_connection, retry, session_ended};
use crate::machine::protocols::{self, SerialLink};
use crate::machine::{Error, Handle, Limits, Machine};
use analyte_config::Endpoint;
use analyte_transport::serial::{self, Device, Settings};
use analyte_transport::{Backoff, Cancellation, IoSide};
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

/// Finds the port for each open attempt and logs the outcome when it changes.
///
/// A USB serial number is looked up again before every open, so an adapter that
/// moves from COM9 to COM12 is followed. The log names the serial number and the
/// port only, and repeats only when the outcome changes, so a missing adapter
/// retried every few seconds does not flood the log. Each failed lookup is
/// still a `Failed` transport event.
struct Locator {
    machine: String,
    last: Option<Result<String, String>>,
}

impl Locator {
    fn new(machine: &str) -> Self {
        Self {
            machine: machine.to_owned(),
            last: None,
        }
    }

    fn locate(
        &mut self,
        device: &Device,
        list: impl FnOnce() -> std::io::Result<Vec<serial::Listed>>,
    ) -> Result<String, serial::Lookup> {
        let found = serial::locate(device, list);
        let Device::UsbSerial(usb_serial) = device else {
            return found;
        };
        let outcome = found
            .as_ref()
            .map(Clone::clone)
            .map_err(ToString::to_string);
        if self.last.as_ref() != Some(&outcome) {
            let machine = self.machine.as_str();
            match &found {
                Ok(port) => tracing::info!(
                    machine,
                    usb_serial,
                    port = port.as_str(),
                    "serial port found by USB serial number"
                ),
                Err(error) => tracing::warn!(
                    machine,
                    usb_serial,
                    error_kind = %error,
                    "serial port not found by USB serial number; retrying"
                ),
            }
            self.last = Some(outcome);
        }
        found
    }
}

/// Builds the port thread body from a port lister and an opener. Production
/// passes the operating system's; tests pass scripted ones through the same path.
fn worker<L, O>(machine: &str, list: L, open: O) -> Worker
where
    L: Fn() -> std::io::Result<Vec<serial::Listed>> + Send + Sync + 'static,
    O: Fn(&str, &Settings) -> std::io::Result<Box<dyn serial::Port>> + Send + Sync + 'static,
{
    let machine = machine.to_owned();
    Arc::new(move |settings, side, retry| {
        let mut locator = Locator::new(&machine);
        serial::run(
            &settings,
            side,
            retry,
            |device| locator.locate(device, &list),
            &open,
        )
    })
}

pub(in crate::machine) async fn run(
    machine: &Machine,
    store: &Handle,
    link: SerialLink<'_>,
    limits: Limits,
    stop: watch::Receiver<bool>,
    closing: crate::machine::Closing,
    capture: Option<&crate::machine::capture::Capture>,
) -> Result<(), Error> {
    let worker = worker(machine.id.as_str(), serial::list, |name, settings| {
        settings.open(name)
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
    link: SerialLink<'_>,
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
        let (mut conn, side) = recorded_connection(limits, capture, &machine.status);
        // A refused thread spawn leaves no port owner to serve any session.
        let owner = Owner::start(worker, &settings, side, &retry, conn.cancellation())?;
        // Sessions share this port owner while they end cleanly: a clean close lets the
        // port thread reopen the device and the next session sees its Opened event. A
        // failed session ends the owner instead, because hosts such as the Autolyser
        // start polling only on Opened and would wait silently on a reused connection.
        let result = loop {
            let result =
                protocols::session(machine, link.session, store, &mut conn, stop.clone()).await;
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
    let device = match &machine.snapshot.effective().site().endpoint {
        Endpoint::Fixed(name) => Device::Named(name.clone()),
        Endpoint::UsbSerial(usb_serial) => Device::UsbSerial(usb_serial.as_str().to_owned()),
    };
    Ok(Settings::new(
        device,
        serial.baud,
        serial
            .data_bits
            .try_into()
            .map_err(|()| LinkError::InvalidSerialDataBits)?,
        serial.parity.into(),
        serial
            .stop_bits
            .try_into()
            .map_err(|()| LinkError::InvalidSerialStopBits)?,
        serial.flow_control.into(),
        Duration::from_millis(100),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // AGENTS.md: keep errors specific; clinical safety rule 10: no payloads in logs.
    // Safety rule 10 (ADR 0003)
    #[test]
    fn serial_drop_reports_worker_failure_without_payloads() {
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

    /// A named port is opened as written; listing ports for it would be a bug.
    fn never_listed(device: &Device) -> Result<String, serial::Lookup> {
        serial::locate(device, || {
            Err(std::io::Error::other("a named port is never listed"))
        })
    }

    fn port_settings(name: &str) -> Result<Settings, Error> {
        Ok(Settings::new(
            Device::Named(name.into()),
            9600,
            serial::DataBits::Eight,
            serial::Parity::None,
            serial::StopBits::One,
            serial::FlowControl::None,
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

    /// The synthetic VIDAS teaching config moved onto a serial port.
    fn vidas_serial() -> String {
        use analyte_fixtures::ConfigEdit as _;
        include_str!("../../../../core/config/examples/vidas_literal.toml")
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
            .rewrite("transport = \"tcp_server\"", "transport = \"serial\"")
    }

    // ADR 0003 / ADR 0010: run the actual ASTM host dialog over the existing
    // serial factory seam. Every byte and failure is synthetic, never an OS port.
    // ADR 0003, ADR 0010
    #[tokio::test(flavor = "multi_thread")]
    async fn serial_astm_stores_each_frame_before_ack() -> Result<(), Error> {
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
        let protocols::Link::Serial(link) =
            protocols::transport(machine.snapshot.effective().stack())
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
            serial::run(
                &settings,
                side,
                retry().expect("valid retry"),
                never_listed,
                move |_, _| {
                    opened.blocking_send(()).map_err(std::io::Error::other)?;
                    port.take()
                        .map(|p| Box::new(p) as Box<dyn serial::Port>)
                        .ok_or_else(|| std::io::ErrorKind::PermissionDenied.into())
                },
            )
        });
        let (stop, stopping) = watch::channel(false);
        let processing =
            protocols::session(machine, link.session, &store, &mut connection, stopping);
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
    // Plan §11
    #[tokio::test]
    async fn open_failure_is_specific_and_owner_joined() -> Result<(), Error> {
        let settings = port_settings("synthetic-refused-port")?;
        let (mut conn, side) =
            analyte_transport::connection(Limits::new(16, Duration::from_secs(2))?);
        let cancellation = conn.cancellation();
        let worker = thread::spawn(move || {
            serial::run(
                &settings,
                side,
                retry().expect("valid retry"),
                never_listed,
                |name, _| {
                    assert_eq!(name, "synthetic-refused-port");
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
    // Review finding Y1
    #[test]
    fn failed_session_then_next_session_is_served() -> Result<(), Error> {
        let root = crate::admin::private_tempdir()?;
        let machine = vidas_serial();
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
        let protocols::Link::Serial(link) =
            protocols::transport(machine.snapshot.effective().stack())
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
            serial::run(&settings, side, retry, never_listed, |_, _| {
                let port = ports
                    .lock()
                    .map_err(|_| std::io::Error::other("port lock"))?
                    .pop_front();
                let port = port.ok_or(std::io::ErrorKind::PermissionDenied)?;
                opened.blocking_send(()).map_err(std::io::Error::other)?;
                Ok(Box::new(port) as Box<dyn serial::Port>)
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

    const ASTM: &str = include_str!("../../../../core/config/examples/astm.toml");

    /// An ASTM serial program whose site file names its port with `location`.
    fn located_program(
        root: &std::path::Path,
        location: &str,
    ) -> Result<crate::installation::config::Program, Error> {
        use analyte_fixtures::ConfigEdit as _;
        serial_program(root, ASTM)?;
        std::fs::write(
            root.join("site.toml"),
            include_str!("../../../../core/config/examples/site.toml")
                .rewrite("endpoint = \"127.0.0.1:5600\"", location),
        )?;
        crate::installation::config::Program::load(&root.join("program.toml")).map_err(Into::into)
    }

    fn serial_link(machine: &Machine) -> Result<SerialLink<'_>, Error> {
        let protocols::Link::Serial(link) =
            protocols::transport(machine.snapshot.effective().stack())
        else {
            return Err("expected serial".into());
        };
        Ok(link)
    }

    fn usb(port: &str) -> serial::Listed {
        serial::Listed {
            name: port.into(),
            usb_serial: Some("A10K3XYZ".into()),
        }
    }

    // Plan §11
    #[test]
    fn site_port_form_reaches_the_serial_link() -> Result<(), Error> {
        for (location, device) in [
            ("endpoint = \"COM9\"", Device::Named("COM9".into())),
            (
                "usb_serial = \"A10K3XYZ\"",
                Device::UsbSerial("A10K3XYZ".into()),
            ),
        ] {
            let root = crate::admin::private_tempdir()?;
            let program = located_program(root.path(), location)?;
            let machine = program.machines().first().ok_or("missing machine")?;
            let link = serial_link(machine)?;
            assert_eq!(settings(machine, link.settings)?.device(), &device);
        }
        Ok(())
    }

    // Plan §11; safety rule 10: the log names the serial number and port, no payload.
    #[test]
    fn usb_lookup_logs_each_change_once() {
        let device = Device::UsbSerial("A10K3XYZ".into());
        let mut locator = Locator::new("synthetic");
        let logs = crate::machine::worker::tests::capture(|| {
            for listed in [
                vec![],
                vec![],
                vec![usb("COM9")],
                vec![usb("COM9")],
                vec![usb("COM9"), usb("COM12")],
                vec![usb("COM12")],
            ] {
                let _ = locator.locate(&device, || Ok(listed));
            }
        });
        let lines: Vec<&str> = logs.lines().collect();
        assert_eq!(lines.len(), 4, "{logs}");
        assert!(
            lines[0].contains("no USB serial adapter with serial number A10K3XYZ is connected")
        );
        assert!(lines[1].contains("port=\"COM9\""), "{logs}");
        assert!(
            lines[2].contains("(COM12, COM9); none was opened"),
            "{logs}"
        );
        assert!(lines[3].contains("port=\"COM12\""), "{logs}");
        assert!(
            lines
                .iter()
                .all(|line| line.contains("machine=\"synthetic\""))
        );
    }

    // Plan §11: the site names the adapter by serial number; the adapter moves
    // from COM9 to COM12 while the port is open, and the link reopens on COM12.
    #[tokio::test(flavor = "multi_thread")]
    async fn link_follows_adapter_to_new_port() -> Result<(), Error> {
        let root = crate::admin::private_tempdir()?;
        let program = located_program(root.path(), "usb_serial = \"A10K3XYZ\"")?;
        let mut store = crate::runtime::service::open_store_with_deadline(
            &program,
            Box::new(Space),
            crate::deadline()?,
        )
        .await?;
        let machine = program.machines().first().ok_or("missing machine")?;
        let link = serial_link(machine)?;
        let lists = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from([
            vec![usb("COM9")],
            vec![usb("COM12")],
        ])));
        let (vanish, vanished) = std::sync::mpsc::sync_channel(4);
        let (idle, quiet) = std::sync::mpsc::sync_channel(4);
        let (first, _first) = tokio::sync::mpsc::channel(4);
        let (second, _second) = tokio::sync::mpsc::channel(4);
        let ports = std::sync::Mutex::new(std::collections::VecDeque::from([
            (vanished, first),
            (quiet, second),
        ]));
        let (opened, mut names) = tokio::sync::mpsc::channel(4);
        let worker = worker(
            "synthetic",
            move || {
                let mut lists = lists.lock().map_err(|_| std::io::Error::other("lists"))?;
                // The last list stays connected until the test ends.
                Ok(if lists.len() > 1 {
                    lists.pop_front().unwrap_or_default()
                } else {
                    lists.front().cloned().unwrap_or_default()
                })
            },
            move |name, _| {
                let (input, output) = ports
                    .lock()
                    .map_err(|_| std::io::Error::other("ports"))?
                    .pop_front()
                    .ok_or(std::io::ErrorKind::PermissionDenied)?;
                opened
                    .blocking_send(name.to_owned())
                    .map_err(std::io::Error::other)?;
                Ok(Box::new(Port { input, output }) as Box<dyn serial::Port>)
            },
        );
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
            assert_eq!(names.recv().await.as_deref(), Some("COM9"));
            vanish.send(Err(std::io::ErrorKind::NotFound.into()))?;
            assert_eq!(names.recv().await.as_deref(), Some("COM12"));
            stop.send_replace(true);
            Ok::<_, Error>(())
        };
        let (served, conversation) = tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
            tokio::join!(owner, conversation)
        })
        .await?;
        conversation?;
        served?;
        drop(idle);
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }

    // Plan §11 admin page: an open port proves neither a cable nor a machine, so a
    // serial machine reports "Port open", never "Connected", and a read is last data.
    #[tokio::test(flavor = "multi_thread")]
    async fn serial_reports_port_open_never_connected() -> Result<(), Error> {
        let root = crate::admin::private_tempdir()?;
        let program = serial_program(root.path(), &vidas_serial())?;
        let mut store = crate::runtime::service::open_store_with_deadline(
            &program,
            Box::new(Space),
            crate::deadline()?,
        )
        .await?;
        let machine = program.machines().first().ok_or("missing machine")?;
        let link = serial_link(machine)?;
        let mut light = machine.status.subscribe();
        assert_eq!(light.borrow().state(), "Port not open");
        let (input, received) = std::sync::mpsc::sync_channel(4);
        let (output, mut writes) = tokio::sync::mpsc::channel(4);
        let port = std::sync::Mutex::new(Some(Port {
            input: received,
            output,
        }));
        let worker: Worker = Arc::new(move |settings, side, retry| {
            serial::run(&settings, side, retry, never_listed, |_, _| {
                let port = port
                    .lock()
                    .map_err(|_| std::io::Error::other("port lock"))?
                    .take();
                let port = port.ok_or(std::io::ErrorKind::PermissionDenied)?;
                Ok(Box::new(port) as Box<dyn serial::Port>)
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
            light.wait_for(|s| s.state() == "Port open").await?;
            assert_eq!(light.borrow().last_data(), None);
            input.send(Ok(vec![5, 13, 10]))?;
            assert_eq!(writes.recv().await.ok_or("ACK absent")?, vec![6, 13, 10]);
            assert!(light.borrow().last_data().is_some(), "a read is last data");
            input.send(Err(std::io::ErrorKind::BrokenPipe.into()))?;
            light.wait_for(|s| s.state() == "Port not open").await?;
            stop.send_replace(true);
            Ok::<_, Error>(())
        };
        let (served, conversation) = tokio::time::timeout(analyte_fixtures::HANG_GUARD, async {
            tokio::join!(owner, conversation)
        })
        .await?;
        conversation?;
        served?;
        store.shutdown(crate::deadline()?).await?;
        Ok(())
    }
}
