-- ADR 0013 items 3, 6 and 7. No credential material belongs in these tables.
CREATE TABLE retention_key(singleton INTEGER PRIMARY KEY CHECK(singleton=1), fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32));
CREATE TABLE retention_holds(kind TEXT NOT NULL CHECK(kind IN ('receipt','order')), target TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), PRIMARY KEY(kind,target));
CREATE TABLE retention_tombstones(receipt TEXT PRIMARY KEY, machine TEXT NOT NULL, driver TEXT NOT NULL, config_version TEXT NOT NULL, original_digest BLOB NOT NULL CHECK(length(original_digest)=32), received_at INTEGER NOT NULL, acknowledged_at INTEGER NOT NULL, purged_at INTEGER NOT NULL);
CREATE TABLE retention_deliveries(delivery TEXT PRIMARY KEY, receipt TEXT NOT NULL REFERENCES retention_tombstones(receipt), acknowledged_at INTEGER NOT NULL);
CREATE TABLE retention_results(identity_digest BLOB NOT NULL CHECK(length(identity_digest)=32), revision INTEGER NOT NULL CHECK(revision>=0), receipt TEXT NOT NULL REFERENCES retention_tombstones(receipt), order_digest BLOB NOT NULL CHECK(length(order_digest)=32), sample_digest BLOB NOT NULL CHECK(length(sample_digest)=32), run_digest BLOB NOT NULL CHECK(length(run_digest)=32), content_digest BLOB NOT NULL CHECK(length(content_digest)=32), PRIMARY KEY(identity_digest,revision));
CREATE INDEX retention_results_receipt ON retention_results(receipt);

CREATE TRIGGER retention_key_update BEFORE UPDATE ON retention_key BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_key_delete BEFORE DELETE ON retention_key BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_tombstones_update BEFORE UPDATE ON retention_tombstones BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_tombstones_delete BEFORE DELETE ON retention_tombstones BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_deliveries_update BEFORE UPDATE ON retention_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_deliveries_delete BEFORE DELETE ON retention_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_results_update BEFORE UPDATE ON retention_results BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_results_delete BEFORE DELETE ON retention_results BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
PRAGMA user_version=10;
