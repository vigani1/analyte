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
        "FIXED-PORT".into(),
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
                move |settings| {
                    assert_eq!(settings.name(), "FIXED-PORT");
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
        "ECHO".into(),
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
            serial::run(&settings, side.with_recording(tap), retry, |_| {
                Ok(Box::new(EchoPort {
                    bytes: VecDeque::new(),
                }))
            })
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
        "SINK".into(),
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
            serial::run(&settings, side, retry, move |_| {
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
