//! Keeps the currently reported software exception and its audited confirmation.
use crate::{Audit, OperatorError as Error, Store};
use analyte_model::{Digest, Identifier};
use jiff::Timestamp;
use rusqlite::{OptionalExtension, params};

/// The first receipt in the current run of unexpected software reports.
#[derive(Debug)]
pub struct SoftwareWarning {
    /// Exact bounded software token, or unknown when missing or unreadable.
    pub reported: Option<Identifier>,
    /// Receipt time of the first report of this version in the run.
    pub first_seen: Timestamp,
}

impl Store {
    /// Records an unexpected version without reopening the same confirmed version.
    /// Older receipts cannot replace a later observation for the same definition.
    /// # Errors
    /// Returns SQLite failures.
    pub fn observe_software_warning(
        &mut self,
        machine: &Identifier,
        revision: Digest,
        reported: Option<&Identifier>,
        when: Timestamp,
    ) -> Result<(), Error> {
        // Fixed precision is required for lexical ordering in SQLite.
        let stamp = format!("{when:.9}");
        let result = self.db.execute(
            "INSERT INTO machine_software_warning(machine,revision,reported,first_seen,last_seen)
             VALUES(?1,?2,?3,?4,?4)
             ON CONFLICT(machine) DO UPDATE SET
             revision=excluded.revision, reported=excluded.reported,
             first_seen=CASE WHEN revision=excluded.revision AND reported IS excluded.reported
                             THEN first_seen ELSE excluded.first_seen END,
             last_seen=excluded.last_seen,
             confirmed=CASE WHEN revision=excluded.revision AND reported IS excluded.reported
                            THEN confirmed ELSE 0 END
             WHERE revision<>excluded.revision OR last_seen<excluded.last_seen",
            params![
                machine.as_str(),
                revision.0.as_slice(),
                reported.map(Identifier::as_str),
                stamp
            ],
        );
        // A later write can concern a different observation, so keep the
        // missed write visible for the lifetime of this store owner.
        self.software_warning_write_failed |= result.is_err();
        Ok(result.map(|_| ())?)
    }

    /// Whether any software warning write failed since this store was opened.
    #[must_use]
    pub const fn software_warning_write_failed(&self) -> bool {
        self.software_warning_write_failed
    }

    /// Reads a warning only for the definition in use and only while unconfirmed.
    /// # Errors
    /// Reports SQLite errors and invalid stored metadata.
    pub fn software_warning(
        &self,
        machine: &Identifier,
        revision: Digest,
    ) -> Result<Option<SoftwareWarning>, Error> {
        let row = self
            .db
            .query_row(
                "SELECT reported,first_seen FROM machine_software_warning
             WHERE machine=?1 AND revision=?2 AND confirmed=0",
                params![machine.as_str(), revision.0.as_slice()],
                |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(reported, first_seen)| {
            Ok(SoftwareWarning {
                reported: reported
                    .as_deref()
                    .map(Identifier::new)
                    .transpose()
                    .map_err(|_| Error::Corruption)?,
                first_seen: first_seen.parse().map_err(|_| Error::Corruption)?,
            })
        })
        .transpose()
    }

    /// Confirms exactly the known warning the operator saw, atomically with its audit.
    /// # Errors
    /// Refuses a changed or already confirmed warning and reports storage errors.
    pub fn confirm_software_warning(
        &mut self,
        machine: &Identifier,
        revision: Digest,
        reported: &Identifier,
        first_seen: Timestamp,
        audit: &Audit,
    ) -> Result<(), Error> {
        let encoded = self.encode(audit)?;
        self.bound(encoded.as_bytes())?;
        self.capacity(encoded.len())?;
        let tx = self.db.transaction()?;
        if tx.execute(
            "UPDATE machine_software_warning SET confirmed=1
             WHERE machine=?1 AND revision=?2 AND reported=?3 AND first_seen=?4 AND confirmed=0",
            params![
                machine.as_str(),
                revision.0.as_slice(),
                reported.as_str(),
                format!("{first_seen:.9}")
            ],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        tx.execute("INSERT INTO audit(evidence) VALUES (?1)", [encoded])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestResult, open};

    // ADR 0036: duplicate reports retain their first time and exact confirmation survives restart.
    #[test]
    fn software_warning_repeats_and_clears_for_the_reviewed_version() -> TestResult {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("store.db");
        let machine = Identifier::new("machine-1")?;
        let reported = Identifier::new("2.0")?;
        let revision = Digest([1; 32]);
        let first: Timestamp = "2026-11-23T12:30:00.123456789Z".parse()?;
        let repeated: Timestamp = "2026-11-23T12:30:01Z".parse()?;
        let audit = Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("settings-checked-for-software-version")?,
            targets: vec![machine.clone(), reported.clone()],
            time: repeated.as_second(),
            reason: analyte_model::Reason::new("Settings checked for this version")?,
        };
        let mut store = open(&path)?;
        store.observe_software_warning(&machine, revision, Some(&reported), first)?;
        store.observe_software_warning(&machine, revision, Some(&reported), repeated)?;
        let warning = store
            .software_warning(&machine, revision)?
            .ok_or("missing warning")?;
        assert_eq!(warning.first_seen, first);
        assert_eq!(warning.reported.as_ref(), Some(&reported));
        assert!(store.software_warning(&machine, Digest([2; 32]))?.is_none());
        assert!(matches!(
            store.confirm_software_warning(&machine, revision, &reported, repeated, &audit),
            Err(Error::Conflict)
        ));
        store.confirm_software_warning(&machine, revision, &reported, first, &audit)?;
        assert_eq!(store.audits(0)?.len(), 1);
        drop(store);
        let mut store = open(&path)?;
        let newest: Timestamp = "2026-11-23T12:30:02Z".parse()?;
        store.observe_software_warning(&machine, revision, Some(&reported), newest)?;
        assert!(store.software_warning(&machine, revision)?.is_none());
        let unknown: Timestamp = "2026-11-23T12:30:03Z".parse()?;
        store.observe_software_warning(&machine, revision, None, unknown)?;
        let warning = store
            .software_warning(&machine, revision)?
            .ok_or("missing unknown warning")?;
        assert!(warning.reported.is_none());
        assert_eq!(warning.first_seen, unknown);
        Ok(())
    }
}
