//! Supervision, failure and recording behavior using synthetic localhost peers.

use analyte_transport::{Backoff, Cancellation, Event, Limits, Reason, connection, tcp};
use std::{error::Error, io, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

mod support;
use support::decode;

type TestResult = Result<(), Box<dyn Error>>;

fn limits() -> Result<Limits, analyte_transport::Error> {
    Limits::new(16, Duration::from_secs(1))
}
fn retry() -> Result<Backoff, analyte_transport::Error> {
    Backoff::new(Duration::from_millis(5), Duration::from_millis(20), 17)
}

#[tokio::test]
async fn client_reconnects_and_cancels_with_ordered_sessions() -> TestResult {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let address = listener.local_addr()?;
    let (mut dialog, side) = connection(limits()?);
    assert!(matches!(
        dialog.write(vec![1]).await,
        Err(analyte_transport::Error::Offline)
    ));
    let cancel = dialog.cancellation();
    let owner = tcp::client(address, side, retry()?);
    let exercise = async {
        let (first, _) = listener.accept().await?;
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        drop(first);
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(Reason::Disconnect))
        ));
        let (mut second, _) = listener.accept().await?;
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        dialog.write(b"new-session".to_vec()).await?;
        let mut bytes = [0; 11];
        second.read_exact(&mut bytes).await?;
        assert_eq!(&bytes, b"new-session");
        cancel.cancel();
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(Reason::Cancelled))
        ));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    Ok(())
}

#[tokio::test]
async fn failed_connect_reports_kind_and_cancels_during_backoff() -> TestResult {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let address = listener.local_addr()?;
    drop(listener);
    let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(4))?);
    let cancel = dialog.cancellation();
    let owner = tcp::client(
        address,
        side,
        Backoff::new(Duration::from_secs(30), Duration::from_secs(60), 42)?,
    );
    let exercise = async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Failed(Reason::Io {
                operation: "connect",
                kind: io::ErrorKind::ConnectionRefused,
                ..
            }))
        ));
        let started = std::time::Instant::now();
        cancel.cancel();
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(Reason::Cancelled))
        ));
        assert!(started.elapsed() < Duration::from_millis(500));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(6), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    Ok(())
}

#[tokio::test]
async fn server_refuses_limit_and_joins_active_connection_on_shutdown() -> TestResult {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let address = listener.local_addr()?;
    let (sender, mut events) = tokio::sync::mpsc::channel(4);
    let cancel = Cancellation::default();
    let owner = tcp::server(listener, 1, limits()?, sender, cancel.clone());
    let exercise = async {
        let _first = TcpStream::connect(address).await?;
        let Some(tcp::ServerEvent::Accepted { mut connection, .. }) = events.recv().await else {
            return Err("missing accepted connection".into());
        };
        assert!(matches!(
            connection.receive().await?,
            Some(Event::Opened { .. })
        ));
        let mut excess = TcpStream::connect(address).await?;
        assert!(matches!(
            events.recv().await,
            Some(tcp::ServerEvent::Refused {
                event: Event::Failed(Reason::LimitReached),
                ..
            })
        ));
        let mut byte = [0];
        assert_eq!(excess.read(&mut byte).await?, 0);
        let started = std::time::Instant::now();
        cancel.cancel();
        assert!(matches!(
            connection.receive().await?,
            Some(Event::Closed(Reason::Cancelled))
        ));
        assert!(started.elapsed() < Duration::from_millis(500));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    Ok(())
}

#[tokio::test]
async fn tcp_recording_is_integrated_and_exact() -> TestResult {
    use analyte_transport::recording::recording;
    let root = tempfile::tempdir()?;
    let path = root.path().join("recording");
    let (tap, worker) = recording(path.clone(), 16)?;
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let mut peer = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut dialog, side) = connection(limits()?);
    let owner = tcp::pump(stream, side.with_recording(tap));
    let exercise = async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        peer.write_all(b"\0\xff001").await?;
        assert!(
            matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"\0\xff001")
        );
        dialog.write(b"reply".to_vec()).await?;
        let mut bytes = [0; 5];
        peer.read_exact(&mut bytes).await?;
        assert_eq!(&bytes, b"reply");
        peer.shutdown().await?;
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(Reason::Disconnect))
        ));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    std::thread::scope(|scope| -> TestResult {
        scope
            .spawn(|| worker.run())
            .join()
            .map_err(|_| "worker panic")??;
        Ok(())
    })?;
    let records = decode(&std::fs::read(path)?)?;
    assert_eq!(records.len(), 4);
    assert!(
        records
            .first()
            .is_some_and(|(kind, _, bytes)| *kind == 0 && bytes.starts_with(b"TCP peer="))
    );
    assert!(
        records
            .get(1)
            .is_some_and(|(kind, _, bytes)| *kind == 1 && bytes == b"\0\xff001")
    );
    assert!(
        records
            .get(2)
            .is_some_and(|(kind, _, bytes)| *kind == 2 && bytes == b"reply")
    );
    assert!(
        records
            .last()
            .is_some_and(|(kind, _, bytes)| *kind == 3 && bytes == b"Disconnect")
    );
    assert!(
        records
            .windows(2)
            .all(|pair| match (pair.first(), pair.last()) {
                (Some(a), Some(b)) => a.1 <= b.1,
                _ => false,
            })
    );
    Ok(())
}

#[tokio::test]
async fn recording_failure_is_visible_without_changing_received_bytes() -> TestResult {
    use analyte_transport::recording::{Failure, recording};
    let root = tempfile::tempdir()?;
    let (tap, worker) = recording(root.path().join("missing").join("recording"), 4)?;
    assert!(worker.run().is_err());
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let mut peer = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut dialog, side) = connection(limits()?);
    let owner = tcp::pump(stream, side.with_recording(tap));
    let exercise = async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::RecordingFailed(Failure::Io {
                kind: io::ErrorKind::NotFound,
                ..
            }))
        ));
        peer.write_all(b"unchanged").await?;
        assert!(
            matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"unchanged")
        );
        dialog.cancellation().cancel();
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(Reason::Cancelled))
        ));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    Ok(())
}

#[tokio::test]
async fn saturated_recorder_flags_failure_but_preserves_stream() -> TestResult {
    let root = tempfile::tempdir()?;
    let (tap, worker) = analyte_transport::recording::recording(root.path().join("capture"), 1)?;
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let mut peer = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut dialog, side) = connection(limits()?);
    let owner = tcp::pump(stream, side.with_recording(tap));
    let exercise = async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        peer.write_all(b"retained").await?;
        assert!(
            matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"retained")
        );
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::RecordingFailed(
                analyte_transport::recording::Failure::QueueFull
            ))
        ));
        peer.write_all(b"also retained").await?;
        assert!(
            matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"also retained")
        );
        dialog.cancellation().cancel();
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(Reason::Cancelled))
        ));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    std::thread::scope(|scope| -> TestResult {
        scope
            .spawn(|| worker.run())
            .join()
            .map_err(|_| "recorder panic")??;
        Ok(())
    })?;
    Ok(())
}

#[tokio::test]
async fn dropped_server_dialog_does_not_stop_listener() -> TestResult {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let address = listener.local_addr()?;
    let (sender, mut events) = tokio::sync::mpsc::channel(4);
    let cancel = Cancellation::default();
    let owner = tcp::server(listener, 2, limits()?, sender, cancel.clone());
    let exercise = async {
        let _first = TcpStream::connect(address).await?;
        let Some(tcp::ServerEvent::Accepted { connection, .. }) = events.recv().await else {
            return Err("missing first connection".into());
        };
        drop(connection);
        let _second = TcpStream::connect(address).await?;
        let Some(tcp::ServerEvent::Accepted { mut connection, .. }) = events.recv().await else {
            return Err("listener stopped".into());
        };
        assert!(matches!(
            connection.receive().await?,
            Some(Event::Opened { .. })
        ));
        cancel.cancel();
        assert!(matches!(
            connection.receive().await?,
            Some(Event::Closed(Reason::Cancelled))
        ));
        Ok::<_, Box<dyn Error>>(())
    };
    let (owner, exercise) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(owner, exercise)
    })
    .await?;
    owner?;
    exercise?;
    Ok(())
}
