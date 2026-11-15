//! Folder completion and recovery with an integrated connection recorder.

use analyte_transport::{Event, Limits, Reason, connection, folder::Watcher, recording::recording};
use std::{error::Error, time::Duration};
mod support;

#[test]
fn half_written_file_is_claimed_only_after_stability_and_recovers() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let incoming = root.path().join("incoming");
    let claimed = root.path().join("claimed");
    std::fs::create_dir(&incoming)?;
    std::fs::create_dir(&claimed)?;
    let source = incoming.join("input");
    std::fs::write(&source, b"half")?;
    let stable = Duration::from_millis(20);
    let mut watcher = Watcher::new(incoming, claimed, Some(stable), 100)?;
    assert!(watcher.scan()?.is_empty());
    std::thread::sleep(stable);
    std::fs::write(&source, b"complete")?;
    assert!(watcher.scan()?.is_empty());
    assert!(source.exists());
    std::thread::sleep(stable);
    let receipts = watcher.scan()?;
    assert_eq!(receipts.len(), 1);
    assert!(!source.exists());
    drop(receipts); // Simulated process loss: only the on-disk claim survives.
    let recovered = watcher.recover()?;
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered.first().ok_or("missing recovered file")?.read()?,
        b"complete"
    );
    Ok(())
}

#[test]
fn folder_delivery_records_every_exact_chunk_and_preserves_archive() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("producer");
    let destination = root.path().join("claim");
    let bytes = vec![0xff; 8193];
    std::fs::write(&source, &bytes)?;
    let receipt = analyte_transport::folder::Claimed::claim(&source, &destination, 10_000)?;
    let path = root.path().join("recording");
    let (tap, worker) = recording(path.clone(), 16)?;
    let (mut dialog, side) = connection(Limits::new(16, Duration::from_secs(1))?);
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        scope
            .spawn(|| receipt.deliver(side.with_recording(tap)))
            .join()
            .map_err(|_| "folder thread panic")??;
        Ok(())
    })?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        assert!(matches!(
            dialog.receive().await?,
            Some(Event::Opened { .. })
        ));
        let mut received = Vec::new();
        while let Some(event) = dialog.receive().await? {
            match event {
                Event::Received { bytes, .. } => received.extend(bytes),
                Event::Closed(Reason::Disconnect) => break,
                Event::Opened { .. }
                | Event::Closed(_)
                | Event::Failed(_)
                | Event::RecordingFailed(_) => return Err("unexpected folder event".into()),
            }
        }
        assert_eq!(received, bytes);
        Ok::<_, Box<dyn Error>>(())
    })?;
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        scope
            .spawn(|| worker.run())
            .join()
            .map_err(|_| "recorder panic")??;
        Ok(())
    })?;
    let records = support::decode(&std::fs::read(path)?)?;
    let recorded: Vec<_> = records
        .iter()
        .filter(|(kind, _, _)| *kind == 1)
        .flat_map(|(_, _, bytes)| bytes.iter().copied())
        .collect();
    assert_eq!(recorded, bytes);
    assert_eq!(records.len(), 5);
    let archive = root.path().join("archive");
    receipt.finish(&archive)?;
    assert_eq!(std::fs::read(archive.join("input"))?, bytes);
    Ok(())
}
