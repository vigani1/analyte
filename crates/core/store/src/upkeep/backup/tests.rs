//! Tests protected database backups.
use crate::test_support::*;
use crate::*;

// Plan §11
#[test]
fn online_backup_reopens_with_exact_receipt_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"protected\0bytes")?;
    store.receive(&metadata, b"protected\0bytes", b"config")?;
    let target = temp.path().join("backup.db");
    store.backup(
        &target,
        Instant::now()
            .checked_add(HANG_GUARD)
            .ok_or(MaintenanceError::DeadlineOverflow)?,
    )?;
    let restored = open(&target)?;
    assert_eq!(
        restored.raw(metadata.id)?,
        Some(b"protected\0bytes".to_vec())
    );
    Ok(())
}

// Plan §11; mutation-testing regression
#[test]
fn locked_backup_source_is_not_reported_corrupt() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = open(&temp.path().join("source.db"))?;
    store.db.execute_batch("BEGIN IMMEDIATE")?;
    let result = store.backup(
        &temp.path().join("locked-copy.db"),
        Instant::now() + HANG_GUARD,
    );
    store.db.execute_batch("ROLLBACK")?;
    assert!(matches!(result, Err(MaintenanceError::Timeout)));
    Ok(())
}
