//! Tests durable transfer chunks and acknowledgment evidence.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn adr0003_committed_frames_survive_restart() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("store.db");
    let transfer = Identifier::new("transfer-1")?;
    {
        let mut store = open(&path)?;
        let first = store.frame(&transfer, FrameSequence::new(1, 7)?, b"frame-one")?;
        assert!(store.frame(&transfer, FrameSequence::new(2, 7)?, b"frame-two")? > first);
    }
    let store = open(&path)?;
    assert_eq!(store.incomplete("")?, vec!["transfer-1"]);
    let bytes: Vec<u8> = store.db.query_row(
        "SELECT raw FROM frames ORDER BY sequence LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(bytes, b"frame-one");
    Ok(())
}

#[test]
fn adr0003_complete_and_aborted_transfers_keep_all_evidence() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let complete = Identifier::new("complete")?;
    let aborted = Identifier::new("aborted")?;
    store.frame(&complete, FrameSequence::new(1, 7)?, b"first")?;
    store.frame(&aborted, FrameSequence::new(1, 7)?, b"partial")?;
    let metadata = receipt(b"first")?;
    store.receive(&metadata, b"first", b"config")?;
    store.finish_transfer(&complete, TransferOutcome::Complete(metadata.id))?;
    store.finish_transfer(
        &aborted,
        TransferOutcome::Incomplete(Reason::new(
            "ASTM transfer ended before a complete receipt was retained",
        )?),
    )?;
    assert_eq!(store.incomplete("")?, vec!["aborted"]);
    assert_eq!(
        store
            .frames(&complete, 0)?
            .first()
            .map(|f| f.raw.as_slice()),
        Some(b"first".as_slice())
    );
    assert_eq!(
        store.frames(&aborted, 0)?.first().map(|f| f.raw.as_slice()),
        Some(b"partial".as_slice())
    );
    assert!(matches!(
        store.frame(&complete, FrameSequence::new(2, 7)?, b"late"),
        Err(IntakeError::Conflict)
    ));
    assert_eq!(
        store.receipt(metadata.id)?.ok_or(Error::Corruption)?.config,
        b"config"
    );
    Ok(())
}

#[test]
fn mutation_adr0003_transfer_provenance_conflicts_preserve_original_frames_and_configuration()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("transfer-provenance.db");
    let mut store = open(&path)?;
    let transfer = Identifier::new("synthetic-transfer")?;
    let metadata = receipt(b"raw")?;
    let provenance = FrameProvenance {
        machine: metadata.machine,
        driver: metadata.driver,
        config: b"config".to_vec(),
    };
    let first = store.journal_frame(&transfer, FrameSequence::new(0, 0)?, b"first", &provenance)?;
    assert!(
        store.journal_frame(&transfer, FrameSequence::new(1, 1)?, b"second", &provenance)? > first
    );
    let changed = FrameProvenance {
        machine: provenance.machine.clone(),
        driver: provenance.driver.clone(),
        config: b"changed".to_vec(),
    };
    assert!(matches!(
        store.journal_frame(&transfer, FrameSequence::new(0, 0)?, b"refused", &changed),
        Err(IntakeError::Conflict)
    ));
    drop(store);
    let store = open(&path)?;
    let frames = store.frames(&transfer, 0)?;
    assert_eq!(
        frames.iter().map(|f| f.raw.as_slice()).collect::<Vec<_>>(),
        vec![b"first".as_slice(), b"second".as_slice()]
    );
    let retained: String = store.db.query_row(
        "SELECT evidence FROM frame_provenance WHERE transfer=?1",
        [transfer.as_str()],
        |r| r.get(0),
    )?;
    let retained: FrameProvenance = serde_json::from_str(&retained)?;
    assert_eq!(retained.machine, provenance.machine);
    assert_eq!(retained.driver, provenance.driver);
    assert_eq!(retained.config, provenance.config);
    Ok(())
}
