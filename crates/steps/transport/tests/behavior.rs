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
