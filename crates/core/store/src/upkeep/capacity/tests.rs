//! Tests disk reserve and storage capacity refusal.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0003_admission_check_matches_the_write_guard_without_writing() -> TestResult {
    let temp = tempfile::tempdir()?;
    let full = Store::open(&temp.path().join("full.db"), limits(), Box::new(Space(0)))?;
    assert!(matches!(full.admission(1), Err(IntakeError::Capacity)));
    let roomy = open(&temp.path().join("roomy.db"))?;
    roomy.admission(1)?;
    assert_eq!(roomy.incomplete("")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn adr0003_capacity_guard_refuses_receipts_and_frames() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = Store::open(&temp.path().join("store.db"), limits(), Box::new(Space(0)))?;
    let metadata = receipt(b"raw")?;
    assert!(matches!(
        store.receive(&metadata, b"raw", b"config"),
        Err(IntakeError::Capacity)
    ));
    assert!(matches!(
        store.frame(
            &Identifier::new("transfer")?,
            FrameSequence::new(0, 7)?,
            b"frame"
        ),
        Err(IntakeError::Capacity)
    ));
    assert_eq!(store.raw(metadata.id)?, None);
    Ok(())
}

#[test]
fn plan11_corruption_and_database_size_limit_refuse_visibly() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("invalid.db");
    std::fs::write(&path, b"this is not SQLite")?;
    assert!(matches!(open(&path), Err(Error::StorageCorruption(_))));
    let mut small = limits();
    small.database_bytes = 1;
    let mut store = Store::open(
        &temp.path().join("small.db"),
        small,
        Box::new(Space(u64::MAX)),
    )?;
    assert!(matches!(
        store.receive(&receipt(b"raw")?, b"raw", b"config"),
        Err(IntakeError::Capacity)
    ));
    Ok(())
}

#[test]
fn mutation_adr0003_rule7_capacity_checks_reserve_and_database_bound_independently() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    store.limits.record_bytes = 2;
    store.bound(b"")?;
    store.bound(b"ab")?;
    assert!(matches!(
        store.bound(b"abc"),
        Err(StorageError::LimitExceeded {
            what: "record_bytes"
        })
    ));
    let pages: u64 = store.db.query_row("PRAGMA page_count", [], |r| {
        r.get::<_, u32>(0).map(u64::from)
    })?;
    let page: u64 = store.db.query_row("PRAGMA page_size", [], |r| {
        r.get::<_, u32>(0).map(u64::from)
    })?;
    let size = pages.checked_mul(page).ok_or("size overflow")?;
    for input in [0usize, 17] {
        let input_bytes = u64::try_from(input)?;
        let needed = 1024u64.checked_add(input_bytes).ok_or("reserve overflow")?;
        let final_size = size.checked_add(input_bytes).ok_or("size overflow")?;
        for (available, free_ok) in [(needed - 1, false), (needed, false), (needed + 1, true)] {
            for (maximum, size_ok) in [
                (final_size - 1, false),
                (final_size, false),
                (final_size + 1, true),
            ] {
                store.space = Box::new(Space(available));
                store.limits.database_bytes = maximum;
                let admitted = store.admission(input);
                if free_ok && size_ok {
                    admitted?;
                } else {
                    assert!(matches!(admitted, Err(IntakeError::Capacity)));
                }
            }
        }
    }
    store.limits.reserve_bytes = u64::MAX;
    store.limits.database_bytes = u64::MAX;
    store.space = Box::new(Space(u64::MAX));
    assert!(matches!(store.admission(1), Err(IntakeError::Capacity)));
    store.limits.reserve_bytes = 0;
    store.space = Box::new(FailedProbe);
    assert!(
        matches!(store.admission(0),Err(IntakeError::Io(error)) if error.kind()==std::io::ErrorKind::PermissionDenied)
    );
    Ok(())
}

struct FailedProbe;
impl FreeSpace for FailedProbe {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "synthetic probe failure",
        ))
    }
}
