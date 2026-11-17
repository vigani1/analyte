-- ADR 0014: retain setup failures separately from attempted network delivery.
ALTER TABLE alert_progress RENAME TO alert_progress_v14;
CREATE TABLE alert_progress(event TEXT PRIMARY KEY REFERENCES alert_events(id) ON DELETE CASCADE, attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0), next_attempt INTEGER NOT NULL, exhausted INTEGER NOT NULL DEFAULT 0 CHECK(exhausted IN (0,1)), failure TEXT CHECK(failure IN ('network','credentials','http','interrupted','setup','tls_setup')));
INSERT INTO alert_progress SELECT * FROM alert_progress_v14;
DROP TABLE alert_progress_v14;
CREATE INDEX alert_progress_due ON alert_progress(exhausted,next_attempt,event);
PRAGMA user_version=15;
