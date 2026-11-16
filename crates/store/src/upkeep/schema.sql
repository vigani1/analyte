CREATE TABLE receipts(id TEXT PRIMARY KEY, metadata TEXT NOT NULL, raw BLOB NOT NULL, config BLOB NOT NULL);
CREATE TABLE transfers(id TEXT PRIMARY KEY, state TEXT NOT NULL CHECK(state IN ('open','aborted','complete')), receipt TEXT REFERENCES receipts(id));
CREATE TABLE frames(sequence INTEGER PRIMARY KEY AUTOINCREMENT, transfer TEXT NOT NULL REFERENCES transfers(id), number INTEGER NOT NULL CHECK(number BETWEEN 0 AND 7), raw BLOB NOT NULL);
CREATE TABLE interpretations(receipt TEXT NOT NULL REFERENCES receipts(id), version INTEGER NOT NULL CHECK(version>0), evidence TEXT NOT NULL, PRIMARY KEY(receipt,version));
CREATE TABLE outbox(sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, receipt TEXT NOT NULL, version INTEGER NOT NULL, destination TEXT NOT NULL, ordering_key TEXT NOT NULL, payload BLOB NOT NULL, digest BLOB NOT NULL, contract INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('pending','in_flight','done','parked','paused','conflict')), next_attempt INTEGER NOT NULL, FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version));
CREATE TABLE attempts(sequence INTEGER PRIMARY KEY AUTOINCREMENT, delivery TEXT NOT NULL REFERENCES outbox(id), outcome TEXT NOT NULL, time INTEGER NOT NULL);
CREATE TABLE parked(id INTEGER PRIMARY KEY AUTOINCREMENT, evidence TEXT NOT NULL, resolution TEXT);
CREATE TABLE audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT, evidence TEXT NOT NULL);
CREATE TABLE feed(destination TEXT PRIMARY KEY, cursor TEXT NOT NULL, complete INTEGER NOT NULL, trusted INTEGER NOT NULL, polled INTEGER NOT NULL, generation INTEGER NOT NULL DEFAULT 0);
CREATE TABLE orders(destination TEXT NOT NULL REFERENCES feed(destination), id TEXT NOT NULL, sample TEXT NOT NULL, evidence TEXT NOT NULL, tombstone INTEGER NOT NULL, expiry INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(destination,id));
PRAGMA user_version=1;

CREATE TRIGGER outbox_immutable BEFORE UPDATE OF id,receipt,version,destination,ordering_key,payload,digest,contract ON outbox BEGIN SELECT RAISE(ABORT,'immutable delivery'); END;
CREATE TRIGGER outbox_retained BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT,'retained delivery'); END;
CREATE TRIGGER parked_retained BEFORE DELETE ON parked BEGIN SELECT RAISE(ABORT,'retained parked item'); END;
