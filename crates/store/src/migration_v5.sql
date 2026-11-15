-- Preserve v4 evidence without guessing coverage from a potentially newer cache.
CREATE TABLE machine_order_items (
 machine_order INTEGER NOT NULL REFERENCES machine_orders(id),
 item_index INTEGER NOT NULL CHECK(item_index BETWEEN 1 AND 100),
 tests TEXT NOT NULL,
 coverage_known INTEGER NOT NULL CHECK(coverage_known IN (0,1)),
 placer TEXT NOT NULL UNIQUE,
 placer_namespace TEXT,
 state TEXT NOT NULL CHECK(state IN ('"Sending"','"Sent"','"Accepted"','"Done"','"Rejected"','"CancelRequested"','"CancelConfirmed"','"Uncertain"')),
 PRIMARY KEY(machine_order,item_index)
);
CREATE TABLE machine_order_item_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 machine_order INTEGER NOT NULL,
 item_index INTEGER NOT NULL,
 evidence TEXT NOT NULL,
 FOREIGN KEY(machine_order,item_index) REFERENCES machine_order_items(machine_order,item_index)
);
CREATE INDEX machine_order_item_history_lookup ON machine_order_item_history(machine_order,item_index,sequence);
CREATE TRIGGER machine_order_items_retained BEFORE DELETE ON machine_order_items BEGIN SELECT RAISE(ABORT,'retained machine order item'); END;
CREATE TRIGGER machine_order_items_identity BEFORE UPDATE OF machine_order,item_index,tests,coverage_known,placer,placer_namespace ON machine_order_items BEGIN SELECT RAISE(ABORT,'immutable machine order item identity'); END;
CREATE TRIGGER machine_order_item_history_update BEFORE UPDATE ON machine_order_item_history BEGIN SELECT RAISE(ABORT,'immutable item history'); END;
CREATE TRIGGER machine_order_item_history_delete BEFORE DELETE ON machine_order_item_history BEGIN SELECT RAISE(ABORT,'retained item history'); END;
INSERT INTO machine_order_items(machine_order,item_index,tests,coverage_known,placer,state)
 SELECT id,1,'[]',0,CAST(id AS TEXT)||'-1',state FROM machine_orders;
INSERT INTO machine_order_item_history(machine_order,item_index,evidence)
 SELECT id,1,json_object('machine_order',id,'index',1,'tests',json('[]'),'coverage_known',json('false'),'placer',CAST(id AS TEXT)||'-1','state',json(state),'updated_at',updated_at,'source_code','migration-v5-coverage-unavailable') FROM machine_orders;
PRAGMA user_version=5;
