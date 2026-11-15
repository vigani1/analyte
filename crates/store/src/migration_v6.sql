CREATE TABLE alert_state (
 key TEXT PRIMARY KEY, responder TEXT NOT NULL CHECK(length(responder) BETWEEN 1 AND 256),
 active INTEGER NOT NULL CHECK(active IN (0,1)), raised_at INTEGER, cleared_at INTEGER,
 value TEXT CHECK(value NOT GLOB '*[^0-9]*' AND length(value) BETWEEN 1 AND 20), evaluated_at INTEGER NOT NULL,
 CHECK(active=0 OR raised_at IS NOT NULL)
);
CREATE TABLE alert_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT NOT NULL REFERENCES alert_state(key),
 active INTEGER NOT NULL CHECK(active IN (0,1)), time INTEGER NOT NULL,
 responder TEXT NOT NULL, value TEXT CHECK(value NOT GLOB '*[^0-9]*' AND length(value) BETWEEN 1 AND 20)
);
CREATE TRIGGER alert_history_retained_update BEFORE UPDATE ON alert_history BEGIN SELECT RAISE(ABORT,'immutable alert history'); END;
CREATE TRIGGER alert_history_retained_delete BEFORE DELETE ON alert_history BEGIN SELECT RAISE(ABORT,'retained alert history'); END;
CREATE TRIGGER alert_state_retained BEFORE DELETE ON alert_state BEGIN SELECT RAISE(ABORT,'retained alert state'); END;
CREATE TABLE machine_activity(machine TEXT PRIMARY KEY, time INTEGER NOT NULL);
INSERT INTO machine_activity SELECT json_extract(metadata,'$.machine'), max(unixepoch(json_extract(metadata,'$.received_at'))) FROM receipts GROUP BY json_extract(metadata,'$.machine') HAVING max(unixepoch(json_extract(metadata,'$.received_at'))) IS NOT NULL;
CREATE TRIGGER receipt_activity AFTER INSERT ON receipts BEGIN
 INSERT INTO machine_activity VALUES(json_extract(NEW.metadata,'$.machine'),unixepoch(json_extract(NEW.metadata,'$.received_at')))
 ON CONFLICT(machine) DO UPDATE SET time=max(time,excluded.time);
END;
CREATE TRIGGER frame_activity AFTER INSERT ON frames WHEN EXISTS(SELECT 1 FROM frame_provenance WHERE transfer=NEW.transfer) BEGIN
 INSERT INTO machine_activity SELECT json_extract(evidence,'$.machine'),unixepoch() FROM frame_provenance WHERE transfer=NEW.transfer
 ON CONFLICT(machine) DO UPDATE SET time=max(time,excluded.time);
END;
CREATE INDEX alert_oldest_pending ON outbox(enqueued_at) WHERE state!='done';
CREATE INDEX alert_open_parked ON parked(id) WHERE resolution IS NULL;
CREATE INDEX alert_uncertain_orders ON machine_orders(id) WHERE state='"Uncertain"';
PRAGMA user_version=6;
