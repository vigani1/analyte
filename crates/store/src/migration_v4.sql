CREATE TABLE machine_orders (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 destination TEXT NOT NULL,
 order_id TEXT NOT NULL,
 machine TEXT NOT NULL,
 version TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('"Sending"','"Sent"','"Accepted"','"Done"','"Rejected"','"CancelRequested"','"CancelConfirmed"','"Uncertain"')),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 source_code TEXT NOT NULL,
 UNIQUE(destination,order_id,machine)
);
CREATE TABLE machine_order_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 machine_order INTEGER NOT NULL REFERENCES machine_orders(id),
 evidence TEXT NOT NULL
);
CREATE TRIGGER machine_orders_retained BEFORE DELETE ON machine_orders BEGIN SELECT RAISE(ABORT,'retained machine order'); END;
CREATE TRIGGER machine_orders_identity BEFORE UPDATE OF id,destination,order_id,machine,version,created_at ON machine_orders BEGIN SELECT RAISE(ABORT,'immutable machine order identity'); END;
CREATE TRIGGER machine_order_history_update BEFORE UPDATE ON machine_order_history BEGIN SELECT RAISE(ABORT,'immutable lifecycle history'); END;
CREATE TRIGGER machine_order_history_delete BEFORE DELETE ON machine_order_history BEGIN SELECT RAISE(ABORT,'retained lifecycle history'); END;
PRAGMA user_version=4;
