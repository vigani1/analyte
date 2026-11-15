use crate::{Error, Store};
use rusqlite::Connection;

impl Store {
    pub(super) fn migrate_v2(db: &mut Connection) -> Result<(), Error> {
        // Rebuild with foreign keys temporarily disabled so attempts continue to
        // reference the same outbox name. The transaction retains every sequence.
        db.execute_batch("PRAGMA foreign_keys=OFF")?;
        let result = (|| {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("migration_v2.sql"))?;
            tx.commit()?;
            Ok::<_, Error>(())
        })();
        db.execute_batch("PRAGMA foreign_keys=ON")?;
        result?;
        Self::integrity(db)
    }
}
