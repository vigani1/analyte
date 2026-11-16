-- ADR 0013 items 2, 3 and 6: preserve completed order identities without source identifiers.
CREATE TABLE retention_orders(id INTEGER PRIMARY KEY, identity_digest BLOB NOT NULL UNIQUE CHECK(length(identity_digest)=32), machine TEXT NOT NULL, order_digest BLOB NOT NULL CHECK(length(order_digest)=32), sample_digest BLOB CHECK(sample_digest IS NULL OR length(sample_digest)=32), original_digest BLOB CHECK(original_digest IS NULL OR length(original_digest)=32), created_at INTEGER NOT NULL, finalized_at INTEGER NOT NULL, purged_at INTEGER NOT NULL);
CREATE TABLE retention_order_deliveries(delivery TEXT PRIMARY KEY, order_tombstone INTEGER NOT NULL REFERENCES retention_orders(id), acknowledged_at INTEGER NOT NULL);
CREATE TRIGGER retention_orders_update BEFORE UPDATE ON retention_orders BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_orders_delete BEFORE DELETE ON retention_orders BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_order_deliveries_update BEFORE UPDATE ON retention_order_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_order_deliveries_delete BEFORE DELETE ON retention_order_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
PRAGMA user_version=11;
