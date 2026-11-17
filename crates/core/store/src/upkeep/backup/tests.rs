//! Tests protected database backups.
use crate::test_support::*;
use crate::*;

#[test]
fn plan11_online_backup_copy_reopens_with_exact_receipt_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"protected\0bytes")?;
    store.receive(&metadata, b"protected\0bytes", b"config")?;
    let target = temp.path().join("backup.db");
    store.backup(
        &target,
        Instant::now()
            .checked_add(HANG_GUARD)
            .ok_or(Error::Timeout)?,
    )?;
    let restored = open(&target)?;
    assert_eq!(
        restored.raw(metadata.id)?,
        Some(b"protected\0bytes".to_vec())
    );
    Ok(())
}

#[test]
fn mutation_plan11_backup_refuses_a_locked_source_without_reporting_corruption() -> TestResult {
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
