CREATE TABLE outbox_v2(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 id TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL DEFAULT 'result' CHECK(kind IN ('result','order_event')),
 receipt TEXT,
 version INTEGER,
 order_id TEXT,
 destination TEXT NOT NULL,
 ordering_key TEXT NOT NULL,
 payload BLOB NOT NULL,
 digest BLOB NOT NULL,
 contract INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','in_flight','done','parked','paused','conflict')),
 next_attempt INTEGER NOT NULL,
 enqueued_at INTEGER NOT NULL DEFAULT (unixepoch()),
 enqueue_time_backfilled INTEGER NOT NULL DEFAULT 0 CHECK(enqueue_time_backfilled IN (0,1)),
 CHECK((kind='result' AND receipt IS NOT NULL AND version IS NOT NULL AND order_id IS NULL)
    OR (kind='order_event' AND receipt IS NULL AND version IS NULL AND order_id IS NOT NULL AND ordering_key=order_id)),
 FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version)
);
INSERT INTO outbox_v2(sequence,id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at,enqueue_time_backfilled)
 SELECT sequence,id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,unixepoch(),1 FROM outbox;
DROP TRIGGER outbox_retained;
DROP TRIGGER outbox_immutable;
DROP TABLE outbox;
ALTER TABLE outbox_v2 RENAME TO outbox;
CREATE TRIGGER outbox_immutable BEFORE UPDATE OF id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,enqueued_at,enqueue_time_backfilled ON outbox BEGIN SELECT RAISE(ABORT,'immutable delivery'); END;
CREATE TRIGGER outbox_retained BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT,'retained delivery'); END;
PRAGMA user_version=2;
