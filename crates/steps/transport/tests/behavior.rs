//! Tests bounded transport I/O and recordings.
use analyte_transport::{Event, Limits, connection, tcp};
use std::{error::Error, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[tokio::test]
async fn loopback_preserves_bytes_and_disconnect() -> Result<(), Box<dyn Error>> {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let address = listener.local_addr()?;
    let (mut dialog, io) = connection(Limits::new(2, Duration::from_secs(2))?);
    let server = async {
        let (stream, _) = listener.accept().await?;
        tcp::pump(stream, io).await?;
        Ok::<_, Box<dyn Error>>(())
    };
    let peer = async {
        let mut socket = TcpStream::connect(address).await?;
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        socket.write_all(b"abc").await?;
        assert!(
            matches!(dialog.receive().await?, Some(Event::Received { bytes, .. }) if bytes == b"abc")
        );
        socket.write_all(b"defghi").await?;
        let mut received = Vec::new();
        while received.len() < 6 {
            if let Some(Event::Received { bytes, .. }) = dialog.receive().await? {
                received.extend(bytes);
            }
        }
        assert_eq!(received, b"defghi");
        dialog.write(b"reply".to_vec()).await?;
        let mut reply = [0; 5];
        socket.read_exact(&mut reply).await?;
        assert_eq!(&reply, b"reply");
        socket.shutdown().await?;
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Closed(analyte_transport::Reason::Disconnect))
        ));
        Ok::<_, Box<dyn Error>>(())
    };
    let (server, peer) =
        tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(server, peer) }).await?;
    server?;
    peer?;
    Ok(())
}

#[tokio::test]
async fn dropping_dialog_cancels_owner() -> Result<(), Box<dyn Error>> {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let socket = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (dialog, side) = connection(Limits::new(1, Duration::from_secs(1))?);
    drop(dialog);
    tokio::time::timeout(Duration::from_millis(100), tcp::pump(stream, side)).await??;
    drop(socket);
    Ok(())
}

// ADR 0003 (refuse visibly); HL7 2.5.1 §2.9.2: the receiver answers every message.
// While the host is still storing a message the machine waits for its reply, so the
// read-idle limit must not close the socket before that reply is written.
#[tokio::test]
async fn busy_host_keeps_connection_until_its_reply() -> Result<(), Box<dyn Error>> {
    let deadline = Duration::from_millis(100);
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let mut socket = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut dialog, side) = connection(Limits::new(4, deadline)?);
    let host = async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        socket.write_all(b"message").await?;
        let mut received = Vec::new();
        while received.len() < 7 {
            if let Some(Event::Received { bytes, .. }) = dialog.receive().await? {
                received.extend(bytes);
            }
        }
        // A slow durable commit: several read-idle limits pass with no peer bytes.
        tokio::time::sleep(deadline.saturating_mul(5)).await;
        dialog.write(b"reply".to_vec()).await?;
        let mut reply = [0; 5];
        socket.read_exact(&mut reply).await?;
        assert_eq!(&reply, b"reply");
        // The silence clock restarts once the host waits again; a silent peer is then closed.
        let next = loop {
            match dialog.receive().await {
                Err(analyte_transport::Error::Deadline(_)) => {}
                next => break next?,
            }
        };
        assert!(
            matches!(
                next,
                Some(Event::Failed(analyte_transport::Reason::Deadline(
                    analyte_transport::DeadlineKind::TcpRead
                )))
            ),
            "a peer silent for the whole limit while the host waits is still closed"
        );
        Ok::<_, Box<dyn Error>>(())
    };
    let (host, pumped) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(host, tcp::pump(stream, side))
    })
    .await?;
    host?;
    pumped?;
    Ok(())
}

// AGENTS.md safety rule 7 (bound every wait): a peer that stays silent while the
// host waits for it is closed after the read-idle limit.
#[tokio::test]
async fn silent_peer_is_closed_after_read_limit() -> Result<(), Box<dyn Error>> {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let mut socket = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut dialog, side) = connection(Limits::new(4, Duration::from_millis(100))?);
    let host = async {
        let mut events = Vec::new();
        loop {
            match dialog.receive().await {
                Ok(Some(Event::Closed(reason))) => {
                    events.push(Event::Closed(reason));
                    return Ok::<_, Box<dyn Error>>(events);
                }
                Ok(Some(event)) => events.push(event),
                Ok(None) => return Err("events ended before Closed".into()),
                Err(analyte_transport::Error::Deadline(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
    };
    let (events, pumped) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(host, tcp::pump(stream, side))
    })
    .await?;
    pumped?;
    let silent = analyte_transport::Reason::Deadline(analyte_transport::DeadlineKind::TcpRead);
    assert!(matches!(
        events?.as_slice(),
        [Event::Opened { .. }, Event::Failed(failed), Event::Closed(closed)]
            if *failed == silent && *closed == silent
    ));
    let mut rest = Vec::new();
    socket.read_to_end(&mut rest).await?;
    assert!(rest.is_empty(), "the socket is closed with nothing sent");
    Ok(())
}

/// ADR 0003 (refuse visibly): a reply queued just before the owner cancels must still
/// reach the peer. The single-threaded runtime fixes the order: without the flush the
/// cancel is seen before the pump takes the queued bytes.
#[tokio::test(flavor = "current_thread")]
async fn flush_sends_queued_reply_before_cancel_closes_the_socket() -> Result<(), Box<dyn Error>> {
    let listener = tcp::bind("127.0.0.1:0".parse()?).await?;
    let mut socket = TcpStream::connect(listener.local_addr()?).await?;
    let (stream, _) = listener.accept().await?;
    let (mut dialog, side) = connection(Limits::new(4, Duration::from_secs(2))?);
    assert!(matches!(
        dialog.flush().await,
        Err(analyte_transport::Error::Offline)
    ));
    let cancel = dialog.cancellation();
    let host = async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        dialog.write(b"refusal".to_vec()).await?;
        dialog.flush().await?;
        cancel.cancel();
        Ok::<_, Box<dyn Error>>(())
    };
    let (host, pumped) = tokio::join!(host, tcp::pump(stream, side));
    host?;
    pumped?;
    let mut received = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut received)).await??;
    assert_eq!(received, b"refusal");
    Ok(())
}

#[test]
fn folder_keeps_claimed_bytes_after_crash() -> Result<(), Box<dyn Error>> {
    use analyte_transport::folder::Claimed;
    let root = tempfile::tempdir()?;
    let source = root.path().join("producer");
    let claim = root.path().join("claim");
    let archive = root.path().join("archive");
    std::fs::write(&source, b"00123\0\xff")?;
    let receipt = Claimed::claim(&source, &claim, 100)?;
    assert!(!source.exists());
    drop(receipt);
    let recovered = Claimed::recover(&claim, 100)?;
    assert_eq!(recovered.read()?, b"00123\0\xff");
    recovered.finish(&archive)?;
    assert_eq!(std::fs::read(archive.join("input"))?, b"00123\0\xff");
    Ok(())
}

#[test]
fn recording_retains_exact_bytes_and_record_boundaries() -> Result<(), Box<dyn Error>> {
    use analyte_transport::recording::{Kind, recording};
    let root = tempfile::tempdir()?;
    let path = root.path().join("capture");
    let (tap, worker) = recording(path.clone(), 4)?;
    let at = std::time::Instant::now();
    tap.record(Kind::Open, at, b"TCP localhost")?;
    tap.record(Kind::In, at, b"\0\xff00123")?;
    tap.record(Kind::Out, at, b"ack")?;
    tap.record(Kind::Close, at, b"disconnect")?;
    drop(tap);
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        scope
            .spawn(move || worker.run())
            .join()
            .map_err(|_| "worker panic")??;
        Ok(())
    })?;
    let (_, events) = analyte_transport::recording::read(
        std::fs::File::open(path)?,
        analyte_transport::recording::Limits::default(),
    )?;
    assert_eq!(
        events
            .iter()
            .map(|e| e.bytes.as_slice())
            .collect::<Vec<_>>(),
        vec![
            b"TCP localhost".as_slice(),
            b"\0\xff00123".as_slice(),
            b"ack".as_slice(),
            b"disconnect".as_slice()
        ]
    );
    Ok(())
}

#[test]
fn changed_file_is_not_ready() -> Result<(), Box<dyn Error>> {
    use analyte_transport::folder::Observation;
    let root = tempfile::tempdir()?;
    let path = root.path().join("partial");
    std::fs::write(&path, b"half")?;
    let mut observed = Observation::new(path.clone())?;
    assert!(!observed.ready(Duration::from_secs(1))?);
    std::fs::write(&path, b"complete")?;
    assert!(!observed.ready(Duration::from_secs(1))?);
    Ok(())
}
