CREATE TABLE poll_intents(
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 machine TEXT NOT NULL CHECK(length(CAST(machine AS BLOB)) BETWEEN 1 AND 256),
 parked INTEGER NOT NULL REFERENCES parked(id),
 state TEXT NOT NULL CHECK(state IN ('pending','confirmed','uncertain')),
 reason TEXT NOT NULL CHECK(length(CAST(reason AS BLOB)) BETWEEN 1 AND 1024),
 UNIQUE(parked,machine)
);
CREATE INDEX poll_intents_machine_state ON poll_intents(machine,state,id);
CREATE TABLE poll_intent_history(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 intent INTEGER NOT NULL REFERENCES poll_intents(id),
 state TEXT NOT NULL CHECK(state IN ('pending','confirmed','uncertain')),
 reason TEXT NOT NULL CHECK(length(CAST(reason AS BLOB)) BETWEEN 1 AND 1024),
 audit TEXT
);
CREATE TRIGGER poll_intents_retained BEFORE DELETE ON poll_intents BEGIN SELECT RAISE(ABORT,'retained poll intent'); END;
CREATE TRIGGER poll_intents_identity BEFORE UPDATE OF id,machine,parked ON poll_intents BEGIN SELECT RAISE(ABORT,'immutable poll identity'); END;
CREATE TRIGGER poll_intents_lifecycle BEFORE UPDATE OF state ON poll_intents
 WHEN NEW.state='pending' OR (OLD.state='confirmed' AND NEW.state!='confirmed')
 BEGIN SELECT RAISE(ABORT,'poll outcome cannot authorize replay'); END;
CREATE TRIGGER poll_intent_history_update BEFORE UPDATE ON poll_intent_history BEGIN SELECT RAISE(ABORT,'immutable poll history'); END;
CREATE TRIGGER poll_intent_history_delete BEFORE DELETE ON poll_intent_history BEGIN SELECT RAISE(ABORT,'retained poll history'); END;
