-- One current software warning per machine, scoped to the effective definition.
CREATE TABLE machine_software_warning (
    machine TEXT PRIMARY KEY,
    revision BLOB NOT NULL CHECK(length(revision)=32),
    reported TEXT,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    confirmed INTEGER NOT NULL DEFAULT 0 CHECK(confirmed IN (0, 1))
);
PRAGMA user_version=20;
