use crate::test_support::*;
use crate::*;
use proptest::prelude::*;

#[test]
fn adr0002_0003_receipts_and_reinterpretations_are_immutable() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw\0exact")?;
    store.receive(&metadata, b"raw\0exact", b"config")?;
    assert!(matches!(
        store.receive(&metadata, b"raw\0exact", b"config"),
        Err(Error::Conflict)
    ));
    let reading = interpretation()?;
    assert_eq!(
        store
            .interpret_reconciled(
                metadata.id,
                reading.clone(),
                &[],
                &[],
                |_, _| Ok(Vec::new())
            )
            .map(|commit| commit.version)?,
        1
    );
    assert_eq!(
        store
            .interpret_reconciled(
                metadata.id,
                reading.clone(),
                &[],
                &[],
                |_, _| Ok(Vec::new())
            )
            .map(|commit| commit.version)?,
        2
    );
    assert!(
        store
            .db
            .execute("UPDATE receipts SET raw=x'00'", [])
            .is_err()
    );
    assert!(store.db.execute("DELETE FROM interpretations", []).is_err());
    assert_eq!(store.raw(metadata.id)?, Some(b"raw\0exact".to_vec()));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn rule3_random_operations_never_lose_or_change_receipt_bytes(operations in prop::collection::vec(prop::collection::vec(any::<u8>(),0..128),1..20)) {
        let temp=tempfile::tempdir()?; let path=temp.path().join("store.db"); let mut store=open(&path)?;
        let mut saved=Vec::new();
        for raw in operations {
            let metadata=receipt(&raw).map_err(|e| TestCaseError::fail(e.to_string()))?; store.receive(&metadata,&raw,b"config")?;
            prop_assert!(store.receive(&metadata,&raw,b"config").is_err());
            saved.push((metadata.id,raw));
            drop(store); store=open(&path)?;
            for (id,raw) in &saved { let stored=store.raw(*id)?; prop_assert_eq!(stored.as_ref(),Some(raw)); }
        }
    }
}

#[test]
fn mutation_adr0003_receipt_digests_validate_raw_and_config_independently_without_writes()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("receipt-digests.db"))?;
    for (raw, config) in [
        (b"other".as_slice(), b"config".as_slice()),
        (b"raw", b"other"),
    ] {
        let metadata = receipt(b"raw")?;
        assert!(matches!(
            store.receive(&metadata, raw, config),
            Err(Error::InvalidInput)
        ));
        assert!(store.receipt(metadata.id)?.is_none());
    }
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let stored = store.receipt(metadata.id)?.ok_or("missing receipt")?;
    assert_eq!(stored.raw, b"raw");
    assert_eq!(stored.config, b"config");
    assert_eq!(stored.metadata.raw_digest, metadata.raw_digest);
    assert_eq!(stored.metadata.config_digest, metadata.config_digest);
    assert_eq!(stored.metadata.machine, metadata.machine);
    assert_eq!(stored.metadata.driver, metadata.driver);
    assert_eq!(stored.metadata.config_version, metadata.config_version);
    assert_eq!(store.raw(metadata.id)?, Some(b"raw".to_vec()));
    assert!(store.raw(receipt(b"missing")?.id)?.is_none());
    Ok(())
}
