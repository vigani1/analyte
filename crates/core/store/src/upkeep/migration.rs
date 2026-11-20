//! Creates the starting schema and applies later forward-only schema steps.
use crate::{
    MaintenanceError as Error, SCHEMA_VERSION, Store,
    startup::{Startup, StartupStep},
};
use rusqlite::Connection;

/// Version the starting schema creates. Stores below it predate the first release.
pub(crate) const STARTING_VERSION: i64 = 15;

/// Each step moves the committed version forward in one transaction. A later release
/// appends `(SCHEMA_VERSION, SCHEMA_VERSION + 1, include_str!(...))` and raises
/// `SCHEMA_VERSION`; the starting schema itself is never edited.
const STEPS: &[(i64, i64, &str)] = &[
    (0, STARTING_VERSION, include_str!("schema.sql")),
    // When each Needs review item was parked; the file says which stored time
    // fills the items already kept.
    (STARTING_VERSION, 16, include_str!("parked_at.sql")),
    // Indexes for the admin pages' reads, and when each delivery was last
    // accepted; the file says what each one serves.
    (16, 17, include_str!("read_indexes.sql")),
];

/// Refuses a stored version this build cannot open; 0 is an empty file.
pub(crate) fn check(version: i64) -> Result<(), Error> {
    if version > SCHEMA_VERSION {
        return Err(Error::SchemaTooNew(version));
    }
    if version < 0 {
        return Err(Error::Corruption);
    }
    if (1..STARTING_VERSION).contains(&version) {
        return Err(Error::SchemaTooOld(version));
    }
    Ok(())
}

pub(super) fn apply(db: &mut Connection, startup: &Startup, version: i64) -> Result<(), Error> {
    for &(from, to, sql) in STEPS {
        if to <= version {
            continue;
        }
        startup.run(StartupStep::Migration { from, to }, || {
            let tx = db.transaction()?;
            tx.execute_batch(sql)?;
            Store::integrity(&tx)?;
            tx.commit()?;
            Ok(())
        })?;
    }
    Ok(())
}
