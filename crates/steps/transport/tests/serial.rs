//! Tests serial settings and bounded fake I/O.

use analyte_transport::{Backoff, Event, Limits, Reason, connection, serial};
use std::{
    collections::VecDeque,
    error::Error,
    io::{self, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

mod support;

/// A named port is opened as written; listing ports for it would be a bug.
fn never_listed(device: &serial::Device) -> Result<String, serial::Lookup> {
    serial::locate(device, || {
        Err(io::Error::other("a named port is never listed"))
    })
}

struct FakePort {
    reads: VecDeque<io::Result<Vec<u8>>>,
}

impl Read for FakePort {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(next) = self.reads.pop_front() {
            let bytes = next?;
            for (target, byte) in buffer.iter_mut().zip(&bytes) {
                *target = *byte;
            }
            return Ok(bytes.len());
        }
        std::thread::park_timeout(Duration::from_millis(1));
        Err(io::ErrorKind::TimedOut.into())
    }
}

impl Write for FakePort {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn vanished_port_reopens_same_name_and_cancels() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("serial-recording");
    let (tap, worker) = analyte_transport::recording::recording(path.clone(), 32)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let settings = serial::Settings::new(
        serial::Device::Named("FIXED-PORT".into()),
        9600,
        serial::DataBits::Eight,
        serial::Parity::None,
        serial::StopBits::One,
        serial::FlowControl::None,
        Duration::from_millis(5),
    )?;
    let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(1))?);
    let attempts = Arc::new(AtomicUsize::new(0));
    let observed = attempts.clone();
    let retry = Backoff::new(Duration::from_millis(5), Duration::from_millis(20), 1)?;
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let owner = scope.spawn(|| {
            serial::run(
                &settings,
                side.with_recording(tap),
                retry,
                never_listed,
                move |name, _| {
                    assert_eq!(name, "FIXED-PORT");
                    let attempt = observed.fetch_add(1, Ordering::SeqCst);
                    let reads = if attempt == 0 {
                        VecDeque::from([
                            Ok(b"before".to_vec()),
                            Err(io::ErrorKind::NotFound.into()),
                        ])
                    } else {
                        VecDeque::from([Ok(b"after".to_vec())])
                    };
                    Ok(Box::new(FakePort { reads }))
                },
            )
        });
        let result = runtime.block_on(async {
            assert!(matches!(dialog.receive().await?, Some(Event::Opened { .. })));
            assert!(matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"before"));
            assert!(matches!(dialog.receive().await?, Some(Event::Failed(Reason::PortVanished(name))) if name == "FIXED-PORT"));
            assert!(matches!(dialog.receive().await?, Some(Event::Closed(Reason::PortVanished(name))) if name == "FIXED-PORT"));
            assert!(matches!(dialog.receive().await?, Some(Event::Opened { .. })));
            assert!(matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"after"));
            let start = std::time::Instant::now();
            dialog.cancellation().cancel();
            assert!(matches!(dialog.receive().await?, Some(Event::Closed(Reason::Cancelled))));
            assert!(start.elapsed() < Duration::from_millis(200));
            Ok::<_, Box<dyn Error>>(())
        });
        dialog.cancellation().cancel();
        owner.join().map_err(|_| "serial owner panic")??;
        result
    })?;
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        scope
            .spawn(|| worker.run())
            .join()
            .map_err(|_| "recorder panic")??;
        Ok(())
    })?;
    let records = support::decode(&std::fs::read(path)?)?;
    let incoming: Vec<_> = records
        .iter()
        .filter(|(kind, _, _)| *kind == 1)
        .flat_map(|(_, _, bytes)| bytes.iter().copied())
        .collect();
    assert_eq!(incoming, b"beforeafter");
    assert_eq!(records.iter().filter(|(kind, _, _)| *kind == 0).count(), 2);
    Ok(())
}

struct EchoPort {
    bytes: VecDeque<u8>,
}

impl Read for EchoPort {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut count = 0_usize;
        for target in buffer {
            let Some(byte) = self.bytes.pop_front() else {
                break;
            };
            *target = byte;
            count = count.saturating_add(1);
        }
        if count == 0 {
            std::thread::park_timeout(Duration::from_millis(1));
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(count)
    }
}

impl Write for EchoPort {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn serial_write_and_read_are_recorded_without_modification() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("capture");
    let (tap, worker) = analyte_transport::recording::recording(path.clone(), 16)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let settings = serial::Settings::new(
        serial::Device::Named("ECHO".into()),
        9600,
        serial::DataBits::Eight,
        serial::Parity::None,
        serial::StopBits::One,
        serial::FlowControl::None,
        Duration::from_millis(5),
    )?;
    let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(1))?);
    let retry = Backoff::new(Duration::from_millis(5), Duration::from_millis(20), 2)?;
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let owner = scope.spawn(|| {
            serial::run(
                &settings,
                side.with_recording(tap),
                retry,
                never_listed,
                |_, _| {
                    Ok(Box::new(EchoPort {
                        bytes: VecDeque::new(),
                    }))
                },
            )
        });
        let result = runtime.block_on(async {
            assert!(matches!(dialog.receive().await?, Some(Event::Opened { .. })));
            dialog.write(b"0001\0\xff".to_vec()).await?;
            assert!(matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"0001\0\xff"));
            dialog.cancellation().cancel();
            assert!(matches!(dialog.receive().await?, Some(Event::Closed(Reason::Cancelled))));
            Ok::<_, Box<dyn Error>>(())
        });
        dialog.cancellation().cancel();
        owner.join().map_err(|_| "serial owner panic")??;
        result
    })?;
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        scope
            .spawn(|| worker.run())
            .join()
            .map_err(|_| "recorder panic")??;
        Ok(())
    })?;
    let records = support::decode(&std::fs::read(path)?)?;
    assert!(
        records
            .iter()
            .any(|(kind, _, bytes)| *kind == 2 && bytes == b"0001\0\xff")
    );
    assert!(
        records
            .iter()
            .any(|(kind, _, bytes)| *kind == 1 && bytes == b"0001\0\xff")
    );
    Ok(())
}

/// Keeps every byte written to it, so a test can see what reached the port.
struct SinkPort {
    written: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl Read for SinkPort {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        std::thread::park_timeout(Duration::from_millis(1));
        Err(io::ErrorKind::TimedOut.into())
    }
}

impl Write for SinkPort {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written
            .lock()
            .map_err(|_| io::Error::other("sink poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// ADR 0003 (refuse visibly): a flushed reply has reached the port before the owner
/// cancels, and a closed session reports offline instead of a confirmed flush.
#[test]
fn serial_flush_confirms_queued_reply_reached_the_port_before_cancel() -> Result<(), Box<dyn Error>>
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let settings = serial::Settings::new(
        serial::Device::Named("SINK".into()),
        9600,
        serial::DataBits::Eight,
        serial::Parity::None,
        serial::StopBits::One,
        serial::FlowControl::None,
        Duration::from_millis(5),
    )?;
    let written = Arc::new(std::sync::Mutex::new(Vec::new()));
    let port = written.clone();
    let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(1))?);
    let retry = Backoff::new(Duration::from_millis(5), Duration::from_millis(20), 3)?;
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let owner = scope.spawn(|| {
            serial::run(&settings, side, retry, never_listed, move |_, _| {
                Ok(Box::new(SinkPort {
                    written: port.clone(),
                }))
            })
        });
        let result = runtime.block_on(async {
            assert!(matches!(
                dialog.receive().await?,
                Some(Event::Opened { .. })
            ));
            dialog.write(b"".to_vec()).await?;
            dialog.flush().await?;
            assert_eq!(
                *written.lock().map_err(|_| "sink poisoned")?,
                b"",
                "the reply must reach the port before the flush returns"
            );
            dialog.cancellation().cancel();
            assert!(matches!(
                dialog.receive().await?,
                Some(Event::Closed(Reason::Cancelled))
            ));
            assert!(matches!(
                dialog.flush().await,
                Err(analyte_transport::Error::Offline)
            ));
            Ok::<_, Box<dyn Error>>(())
        });
        dialog.cancellation().cancel();
        owner.join().map_err(|_| "serial owner panic")??;
        result
    })?;
    Ok(())
}

fn usb(port: &str, serial: Option<&str>) -> serial::Listed {
    serial::Listed {
        name: port.into(),
        usb_serial: serial.map(Into::into),
    }
}

fn usb_settings() -> Result<serial::Settings, analyte_transport::Error> {
    serial::Settings::new(
        serial::Device::UsbSerial("A10K3XYZ".into()),
        9600,
        serial::DataBits::Eight,
        serial::Parity::None,
        serial::StopBits::One,
        serial::FlowControl::None,
        Duration::from_millis(5),
    )
}

// Plan §11
#[test]
fn usb_serial_lookup_names_one_port_or_refuses() {
    let device = serial::Device::UsbSerial("A10K3XYZ".into());
    let found = serial::locate(&device, || {
        Ok(vec![
            usb("COM3", None),
            usb("COM4", Some("OTHER1")),
            // Windows reports hardware-ID letters in upper case.
            usb("COM9", Some("a10k3xyz")),
        ])
    });
    assert_eq!(found.ok().as_deref(), Some("COM9"));

    let missing = serial::locate(&device, || Ok(vec![usb("COM4", Some("OTHER1"))]));
    let Err(missing) = missing else {
        panic!("no port reports the serial number")
    };
    assert_eq!(
        missing.to_string(),
        "no USB serial adapter with serial number A10K3XYZ is connected"
    );
    assert_eq!(
        missing.reason(),
        Reason::UsbSerialNotConnected("A10K3XYZ".into())
    );

    let twice = serial::locate(&device, || {
        Ok(vec![
            usb("COM9", Some("A10K3XYZ")),
            usb("COM12", Some("A10K3XYZ")),
        ])
    });
    let Err(twice) = twice else {
        panic!("two ports report the serial number")
    };
    assert_eq!(
        twice.to_string(),
        "2 USB serial ports report serial number A10K3XYZ (COM12, COM9); none was opened"
    );

    let unlisted = serial::locate(&device, || Err(io::ErrorKind::PermissionDenied.into()));
    assert!(matches!(
        unlisted.map_err(|lookup| lookup.reason()),
        Err(Reason::Io {
            operation: "list serial ports",
            kind: io::ErrorKind::PermissionDenied,
            ..
        })
    ));
}

/// Runs a USB-serial owner over scripted port lists until `script` returns,
/// then reports every port name it opened.
fn run_usb(
    lists: Vec<Vec<serial::Listed>>,
    ports: Vec<FakePort>,
    script: impl AsyncFnOnce(&mut analyte_transport::Connection) -> Result<(), Box<dyn Error>>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let settings = usb_settings()?;
    let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(1))?);
    let retry = Backoff::new(Duration::from_millis(5), Duration::from_millis(20), 4)?;
    let mut lists = VecDeque::from(lists);
    let mut ports = VecDeque::from(ports);
    let opened = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let owner = scope.spawn(|| {
            serial::run(
                &settings,
                side,
                retry,
                |device| {
                    // The last scripted list stays connected until the test ends.
                    let listed = if lists.len() > 1 {
                        lists.pop_front()
                    } else {
                        lists.front().cloned()
                    };
                    serial::locate(device, || Ok(listed.unwrap_or_default()))
                },
                |name, _| {
                    opened
                        .lock()
                        .map_err(|_| io::Error::other("opened poisoned"))?
                        .push(name.to_owned());
                    let port = ports.pop_front().ok_or(io::ErrorKind::PermissionDenied)?;
                    Ok(Box::new(port) as Box<dyn serial::Port>)
                },
            )
        });
        let result = runtime.block_on(script(&mut dialog));
        dialog.cancellation().cancel();
        owner.join().map_err(|_| "serial owner panic")??;
        result
    })?;
    Ok(opened.into_inner().map_err(|_| "opened poisoned")?)
}

fn reading(bytes: &[u8]) -> FakePort {
    FakePort {
        reads: VecDeque::from([Ok(bytes.to_vec())]),
    }
}

// Plan §11
#[test]
fn usb_serial_opens_the_matching_port() -> Result<(), Box<dyn Error>> {
    let opened = run_usb(
        vec![vec![
            usb("COM3", None),
            usb("COM9", Some("A10K3XYZ")),
            usb("COM4", Some("OTHER1")),
        ]],
        vec![reading(b"data")],
        async |dialog| {
            assert!(matches!(
                dialog.receive().await?,
                Some(Event::Opened { .. })
            ));
            assert!(
                matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"data")
            );
            Ok(())
        },
    )?;
    assert_eq!(opened, ["COM9"]);
    Ok(())
}

// Plan §11
#[test]
fn missing_usb_serial_fails_and_retries() -> Result<(), Box<dyn Error>> {
    let opened = run_usb(
        vec![
            vec![usb("COM4", Some("OTHER1"))],
            vec![],
            vec![usb("COM9", Some("A10K3XYZ"))],
        ],
        vec![reading(b"data")],
        async |dialog| {
            for _ in 0..2 {
                let event = dialog.receive().await?;
                assert!(
                    matches!(
                        &event,
                        Some(Event::Failed(Reason::UsbSerialNotConnected(serial))) if serial == "A10K3XYZ"
                    ),
                    "{event:?}"
                );
            }
            assert!(matches!(
                dialog.receive().await?,
                Some(Event::Opened { .. })
            ));
            Ok(())
        },
    )?;
    assert_eq!(opened, ["COM9"]);
    Ok(())
}

// Plan §11
#[test]
fn ambiguous_usb_serial_opens_nothing() -> Result<(), Box<dyn Error>> {
    let opened = run_usb(
        vec![vec![
            usb("COM9", Some("A10K3XYZ")),
            usb("COM12", Some("A10K3XYZ")),
        ]],
        vec![reading(b"data")],
        async |dialog| {
            for _ in 0..3 {
                let event = dialog.receive().await?;
                assert!(
                    matches!(
                        &event,
                        Some(Event::Failed(Reason::UsbSerialAmbiguous { serial, ports }))
                            if serial == "A10K3XYZ" && ports == &["COM12", "COM9"]
                    ),
                    "{event:?}"
                );
            }
            Ok(())
        },
    )?;
    assert!(opened.is_empty(), "no port may open: {opened:?}");
    Ok(())
}

// Plan §11
#[test]
fn reconnect_follows_adapter_to_new_port() -> Result<(), Box<dyn Error>> {
    let opened = run_usb(
        vec![
            vec![usb("COM9", Some("A10K3XYZ"))],
            vec![usb("COM12", Some("A10K3XYZ"))],
        ],
        vec![
            FakePort {
                reads: VecDeque::from([
                    Ok(b"before".to_vec()),
                    Err(io::ErrorKind::NotFound.into()),
                ]),
            },
            reading(b"after"),
        ],
        async |dialog| {
            assert!(matches!(
                dialog.receive().await?,
                Some(Event::Opened { .. })
            ));
            assert!(
                matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"before")
            );
            assert!(
                matches!(dialog.receive().await?, Some(Event::Failed(Reason::PortVanished(name))) if name == "COM9")
            );
            assert!(
                matches!(dialog.receive().await?, Some(Event::Closed(Reason::PortVanished(name))) if name == "COM9")
            );
            assert!(matches!(
                dialog.receive().await?,
                Some(Event::Opened { .. })
            ));
            assert!(
                matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"after")
            );
            Ok(())
        },
    )?;
    assert_eq!(opened, ["COM9", "COM12"]);
    Ok(())
}
