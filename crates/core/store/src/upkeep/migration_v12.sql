-- ADR 0014: separate immutable event bytes from mutable endpoint progress.
CREATE INDEX alert_history_episode ON alert_history(key,active,sequence);
CREATE TABLE alert_installation(singleton INTEGER PRIMARY KEY CHECK(singleton=1), identity TEXT NOT NULL);
INSERT INTO alert_installation VALUES(1,lower(hex(randomblob(16))));
CREATE TABLE alert_endpoints(id TEXT PRIMARY KEY, cursor INTEGER NOT NULL DEFAULT 0, queue_full INTEGER NOT NULL DEFAULT 0 CHECK(queue_full IN (0,1)), last_success INTEGER);
CREATE TABLE alert_events(id TEXT PRIMARY KEY, endpoint TEXT NOT NULL REFERENCES alert_endpoints(id), condition TEXT NOT NULL, episode INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('raised','reminder','resolved')), observed INTEGER NOT NULL, payload BLOB NOT NULL CHECK(length(payload)<=16384));
CREATE INDEX alert_events_condition ON alert_events(endpoint,condition);
CREATE INDEX alert_events_endpoint ON alert_events(endpoint,observed,id);
CREATE UNIQUE INDEX alert_one_reminder ON alert_events(endpoint,condition) WHERE kind='reminder';
CREATE TABLE alert_progress(event TEXT PRIMARY KEY REFERENCES alert_events(id) ON DELETE CASCADE, attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0), next_attempt INTEGER NOT NULL, exhausted INTEGER NOT NULL DEFAULT 0 CHECK(exhausted IN (0,1)), failure TEXT CHECK(failure IN ('network','credentials','http','interrupted')));
CREATE INDEX alert_progress_due ON alert_progress(exhausted,next_attempt,event);
CREATE TABLE alert_reminders(endpoint TEXT NOT NULL REFERENCES alert_endpoints(id), condition TEXT NOT NULL, episode INTEGER NOT NULL, next_reminder INTEGER NOT NULL, PRIMARY KEY(endpoint,condition));
CREATE TRIGGER alert_events_immutable BEFORE UPDATE ON alert_events BEGIN SELECT RAISE(ABORT,'immutable alert event'); END;
CREATE TRIGGER alert_installation_immutable BEFORE UPDATE ON alert_installation BEGIN SELECT RAISE(ABORT,'immutable alert installation'); END;
CREATE TRIGGER alert_installation_retained BEFORE DELETE ON alert_installation BEGIN SELECT RAISE(ABORT,'retained alert installation'); END;
PRAGMA user_version=12;
