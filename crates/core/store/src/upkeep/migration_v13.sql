-- ADR 0016: unknown execution stays absent; no command history is rewritten.
CREATE TABLE awos_execution_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 machine_order INTEGER NOT NULL,
 item_index INTEGER NOT NULL,
 receipt TEXT NOT NULL,
 interpretation INTEGER NOT NULL CHECK(interpretation>0),
 state TEXT NOT NULL CHECK(state IN ('Scheduled','InProgress','AnalyzerCancelled')),
 recorded_at INTEGER NOT NULL,
 cancel_outstanding INTEGER NOT NULL CHECK(cancel_outstanding IN (0,1)),
 FOREIGN KEY(machine_order,item_index) REFERENCES machine_order_items(machine_order,item_index),
 FOREIGN KEY(receipt,interpretation) REFERENCES interpretations(receipt,version),
 UNIQUE(machine_order,item_index,receipt,interpretation)
);
CREATE INDEX awos_execution_item ON awos_execution_history(machine_order,item_index,sequence);
CREATE TRIGGER awos_execution_immutable BEFORE UPDATE ON awos_execution_history BEGIN SELECT RAISE(ABORT,'immutable AWOS execution evidence'); END;
CREATE TRIGGER awos_execution_retained BEFORE DELETE ON awos_execution_history BEGIN SELECT RAISE(ABORT,'retained AWOS execution evidence'); END;
PRAGMA user_version=13;
