//! Owns blocking serial I/O on a dedicated thread and joins it on cancellation.
use super::{Error, Handle, Limits, Machine, invalid};
use analyte_transport::{Cancellation, serial::Settings};
use std::{thread, time::Duration};
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
            super::worker::report_join("serial", &thread.join());
        }
    }
}

impl Owner {
    async fn close(mut self) -> Result<(), Error> {
        self.cancellation.cancel();
        // Completion also wakes on panic; the retained join below reports its cause.
        // On timeout or cancellation Drop still owns and joins the worker.
        if tokio::time::timeout_at(crate::deadline().into(), &mut self.finished)
            .await
            .is_err()
        {
            return Err(invalid("serial owner shutdown timed out"));
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| invalid("serial owner panicked"))??;
        }
        Ok(())
    }
}

pub(super) async fn run(
    machine: &Machine,
    store: &Handle,
    link: super::settings::SerialLink<'_>,
    limits: Limits,
    stop: watch::Receiver<bool>,
    mut closing: super::Closing,
    capture: Option<&crate::machine::capture::Capture>,
) -> Result<(), Error> {
    let serial = link.settings;
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
    let settings = Settings::new(
        machine.snapshot.effective().site().endpoint.clone(),
        serial.baud,
        serial
            .data_bits
            .try_into()
            .map_err(|()| invalid("invalid serial data bits"))?,
        parity,
        serial
            .stop_bits
            .try_into()
            .map_err(|()| invalid("invalid serial stop bits"))?,
        flow,
        Duration::from_millis(100),
    )?;
    let (mut conn, side) = analyte_transport::connection(limits);
    let side = match capture {
        Some(capture) => capture.attach(side),
        None => side,
    };
    let cancellation = conn.cancellation();
    let retry = super::retry()?;
    let (finished, completion) = tokio::sync::oneshot::channel();
    let thread = thread::Builder::new()
        .name("analyte-serial".into())
        .spawn(move || {
            let result = analyte_transport::serial::run_system(&settings, side, retry);
            let _ = finished.send(());
            result
        })?;
    let owner = Owner {
        cancellation,
        finished: completion,
        thread: Some(thread),
    };
    let result = loop {
        let result = super::session(machine, link.session, store, &mut conn, stop.clone()).await;
        if result.is_err() || *stop.borrow() {
            break result;
        }
    };
    if *stop.borrow() {
        closing.after_commits().await?;
    }
    drop(conn);
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
                Err(analyte_transport::Error::Deadline("protected-error-token"))
            });
            let output = super::super::worker::tests::capture(|| {
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

    struct Space;
    impl analyte_store::FreeSpace for Space {
        fn available(&self, _: &std::path::Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
    }
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
    ) -> Result<crate::installation::config::Program, Error> {
        std::fs::write(
            root.join("machine.toml"),
            include_str!("../../../core/config/examples/astm.toml"),
        )?;
        std::fs::write(
            root.join("site.toml"),
            include_str!("../../../core/config/examples/site.toml"),
        )?;
        let path = root.join("program.toml");
        std::fs::write(
            &path,
            "schema_version=1\ndata_directory='data'\n[[machines]]\nid='synthetic'\ndefinition='machine.toml'\nsite='site.toml'\n[store]\nreserve_bytes=1\ndatabase_bytes=100000000\nrecord_bytes=2097152\npage_rows=100\nbusy_timeout_ms=100\n",
        )?;
        crate::installation::config::Program::load(&path)
    }

    // ADR 0003 / ADR 0010: run the actual ASTM host dialog over the existing
    // serial factory seam. Every byte and failure is synthetic, never an OS port.
    #[tokio::test(flavor = "multi_thread")]
    async fn adr0003_adr0010_serial_timeout_split_merged_frames_store_before_ack_and_disconnect()
    -> Result<(), Error> {
        let root = crate::admin::private_tempdir()?;
        let program = serial_program(root.path())?;
        let until = || std::time::Instant::now() + Duration::from_secs(30);
        let mut store =
            crate::runtime::service::open_store_with_deadline(&program, Box::new(Space), until())
                .await?;
        let machine = program.machines().first().ok_or("missing machine")?;
        let super::super::settings::Link::Serial(link) =
            super::super::settings::transport(machine.snapshot.effective().stack())
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
                super::super::retry().expect("valid retry"),
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
            super::super::session(machine, link.session, &store, &mut connection, stopping);
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
                super::super::retry().expect("valid retry"),
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
}
