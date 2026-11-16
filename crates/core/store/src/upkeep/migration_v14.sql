-- ADR 0016: durable message disposition and immutable per-item source evidence.
CREATE TABLE law_messages(receipt TEXT NOT NULL,version INTEGER NOT NULL,scope TEXT NOT NULL,message TEXT NOT NULL,digest BLOB NOT NULL,disposition TEXT NOT NULL,evidence TEXT NOT NULL,PRIMARY KEY(receipt,version),FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version));
CREATE INDEX law_message_identity ON law_messages(scope,message);
CREATE TABLE law_work_history(sequence INTEGER PRIMARY KEY AUTOINCREMENT,receipt TEXT NOT NULL,version INTEGER NOT NULL,machine_order INTEGER NOT NULL,item_index INTEGER NOT NULL,evidence TEXT NOT NULL,FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version),FOREIGN KEY(machine_order,item_index) REFERENCES machine_order_items(machine_order,item_index));
CREATE INDEX law_work_item ON law_work_history(machine_order,item_index,sequence);
CREATE TRIGGER law_messages_update BEFORE UPDATE ON law_messages BEGIN SELECT RAISE(ABORT,'immutable LAW disposition'); END;
CREATE TRIGGER law_messages_delete BEFORE DELETE ON law_messages BEGIN SELECT RAISE(ABORT,'retained LAW disposition'); END;
CREATE TRIGGER law_work_update BEFORE UPDATE ON law_work_history BEGIN SELECT RAISE(ABORT,'immutable LAW evidence'); END;
CREATE TRIGGER law_work_delete BEFORE DELETE ON law_work_history BEGIN SELECT RAISE(ABORT,'retained LAW evidence'); END;
PRAGMA user_version=14;
