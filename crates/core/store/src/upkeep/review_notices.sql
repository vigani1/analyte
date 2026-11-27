-- Step 20 to 21: Needs review notices to the lab app (ADR 0038). Every stored
-- value stays as it was.
--
-- The outbox gains a third kind, 'review_notice', and a seventh state,
-- 'declined': a lab app that answers 404 or 501 for /review-notices does not
-- take notices, so nothing more is sent to it and nothing is parked. SQLite
-- cannot change a table's checks in place, so the table is rebuilt with the
-- same columns, rows, sequence counter, indexes and triggers.
--
-- The connection keeps foreign keys on inside a migration, so the rows are held
-- in a plain copy while the old table is dropped and the new one is created,
-- then written back: each attempt is then again linked to its delivery before
-- the step commits, and the check after the step finds no broken link.
CREATE TABLE outbox_rebuild AS SELECT * FROM outbox;
CREATE TABLE outbox_rebuild_sequence AS SELECT seq FROM sqlite_sequence WHERE name='outbox';
PRAGMA defer_foreign_keys=ON;
DROP TABLE outbox;
CREATE TABLE "outbox"(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 id TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL DEFAULT 'result' CHECK(kind IN ('result','order_event','review_notice')),
 receipt TEXT,
 version INTEGER,
 order_id TEXT,
 destination TEXT NOT NULL,
 ordering_key TEXT NOT NULL,
 payload BLOB NOT NULL,
 digest BLOB NOT NULL,
 contract INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','in_flight','done','parked','paused','conflict','declined')),
 next_attempt INTEGER NOT NULL,
 enqueued_at INTEGER NOT NULL DEFAULT (unixepoch()),
 enqueue_time_backfilled INTEGER NOT NULL DEFAULT 0 CHECK(enqueue_time_backfilled IN (0,1)),
 delivered_at INTEGER CHECK(delivered_at IS NULL OR typeof(delivered_at)='integer'),
 delivered_attempt INTEGER CHECK(delivered_attempt IS NULL OR typeof(delivered_attempt)='integer'),
 -- A notice names neither a receipt nor an order: its ordering key is the
 -- Needs review item's number, so an item's closing notice waits for its
 -- opening one. Only a notice can be declined.
 CHECK((kind='result' AND receipt IS NOT NULL AND version IS NOT NULL AND order_id IS NULL)
    OR (kind='order_event' AND receipt IS NULL AND version IS NULL AND order_id IS NOT NULL AND ordering_key=order_id)
    OR (kind='review_notice' AND receipt IS NULL AND version IS NULL AND order_id IS NULL)),
 CHECK(state!='declined' OR kind='review_notice'),
 FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version)
);
INSERT INTO outbox(sequence,id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at,enqueue_time_backfilled,delivered_at,delivered_attempt)
 SELECT sequence,id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at,enqueue_time_backfilled,delivered_at,delivered_attempt FROM outbox_rebuild ORDER BY sequence;
-- Keep the counter where it was, so a number used by a delivery that data
-- keeping removed is never given again.
UPDATE sqlite_sequence SET seq=max(seq,(SELECT seq FROM outbox_rebuild_sequence)) WHERE name='outbox' AND EXISTS(SELECT 1 FROM outbox_rebuild_sequence);
INSERT INTO sqlite_sequence(name,seq) SELECT 'outbox',seq FROM outbox_rebuild_sequence WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='outbox');
DROP TABLE outbox_rebuild;
DROP TABLE outbox_rebuild_sequence;
CREATE INDEX outbox_destination_state ON outbox(destination,state);
CREATE INDEX outbox_receipt ON outbox(receipt);
CREATE INDEX outbox_ordering ON outbox(destination,kind,ordering_key);
CREATE INDEX outbox_delivered_at ON outbox(destination,delivered_at) WHERE delivered_at IS NOT NULL;
CREATE INDEX outbox_delivered_attempt ON outbox(destination,delivered_attempt) WHERE delivered_attempt IS NOT NULL;
-- A declined notice is finished, like an accepted delivery.
CREATE INDEX alert_oldest_pending ON outbox(enqueued_at) WHERE state NOT IN ('done','declined');
CREATE TRIGGER outbox_immutable BEFORE UPDATE OF id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,enqueued_at,enqueue_time_backfilled ON outbox BEGIN SELECT RAISE(ABORT,'immutable delivery'); END;
CREATE TRIGGER outbox_retained BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT,'retained delivery'); END;

-- Each lab app that takes notices: the last Needs review item it was checked
-- for, and when and why it declined them. A row exists only while the lab app's
-- setting is on; the engine writes the rows at startup.
CREATE TABLE review_notice_destinations(
 destination TEXT PRIMARY KEY CHECK(length(CAST(destination AS BLOB)) BETWEEN 1 AND 256),
 scanned INTEGER NOT NULL CHECK(typeof(scanned)='integer' AND scanned>=0),
 declined_at INTEGER CHECK(declined_at IS NULL OR typeof(declined_at)='integer'),
 declined_reason TEXT CHECK(declined_reason IS NULL OR length(CAST(declined_reason AS BLOB)) BETWEEN 1 AND 1024),
 CHECK((declined_at IS NULL)=(declined_reason IS NULL))
);
-- One opening and one closing notice at most per item and lab app, each one
-- delivery; the delivery keeps its ID and bytes on every retry.
CREATE TABLE review_notices(
 delivery TEXT PRIMARY KEY REFERENCES outbox(id),
 destination TEXT NOT NULL,
 item INTEGER NOT NULL REFERENCES parked(id),
 phase TEXT NOT NULL CHECK(phase IN ('opened','closed')),
 UNIQUE(destination,item,phase)
);
CREATE INDEX review_notices_item ON review_notices(item);
-- The items a lab app was told about and not yet told are settled: the rows a
-- closing notice looks for, so finding them reads only what is still open.
CREATE TABLE review_notices_open(
 destination TEXT NOT NULL,
 item INTEGER NOT NULL REFERENCES parked(id),
 opening TEXT NOT NULL UNIQUE REFERENCES review_notices(delivery),
 PRIMARY KEY(destination,item)
);
CREATE TRIGGER review_notices_immutable BEFORE UPDATE ON review_notices BEGIN SELECT RAISE(ABORT,'immutable review notice'); END;
PRAGMA user_version=21;
