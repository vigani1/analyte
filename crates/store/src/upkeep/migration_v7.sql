-- Original history JSON, row identities, foreign keys and reservation high-water marks survive.
DROP TRIGGER alert_uncertain_insert;
DROP TRIGGER alert_uncertain_transition;
CREATE TEMP TABLE store_v7_sequence(value INTEGER);
INSERT INTO store_v7_sequence SELECT seq FROM sqlite_sequence WHERE name='machine_orders';
CREATE TABLE machine_orders_v7 (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 destination TEXT NOT NULL,
 order_id TEXT NOT NULL,
 machine TEXT NOT NULL,
 version TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('Sending','Sent','Accepted','Done','Rejected','CancelRequested','CancelConfirmed','Uncertain')),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 source_code TEXT NOT NULL,
 UNIQUE(destination,order_id,machine)
);
CREATE TABLE machine_order_items_v7 (
 machine_order INTEGER NOT NULL REFERENCES machine_orders(id),
 item_index INTEGER NOT NULL CHECK(item_index BETWEEN 1 AND 100),
 tests TEXT NOT NULL,
 coverage_known INTEGER NOT NULL CHECK(coverage_known IN (0,1)),
 placer TEXT NOT NULL UNIQUE,
 placer_namespace TEXT,
 state TEXT NOT NULL CHECK(state IN ('Sending','Sent','Accepted','Done','Rejected','CancelRequested','CancelConfirmed','Uncertain')),
 PRIMARY KEY(machine_order,item_index)
);

INSERT INTO machine_orders_v7 SELECT id,destination,order_id,machine,version,json_extract(state,'$'),created_at,updated_at,source_code FROM machine_orders;
INSERT INTO machine_order_items_v7 SELECT machine_order,item_index,tests,coverage_known,placer,placer_namespace,json_extract(state,'$') FROM machine_order_items;
DROP TABLE machine_order_items;
DROP TABLE machine_orders;
ALTER TABLE machine_orders_v7 RENAME TO machine_orders;
ALTER TABLE machine_order_items_v7 RENAME TO machine_order_items;
UPDATE sqlite_sequence SET seq=max(seq,coalesce((SELECT max(value) FROM store_v7_sequence),seq)) WHERE name='machine_orders';
INSERT INTO sqlite_sequence(name,seq) SELECT 'machine_orders',max(value) FROM store_v7_sequence
 HAVING max(value) IS NOT NULL AND NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='machine_orders');
DROP TABLE store_v7_sequence;
CREATE TRIGGER machine_orders_retained BEFORE DELETE ON machine_orders BEGIN SELECT RAISE(ABORT,'retained machine order'); END;
CREATE TRIGGER machine_orders_identity BEFORE UPDATE OF id,destination,order_id,machine,version,created_at ON machine_orders BEGIN SELECT RAISE(ABORT,'immutable machine order identity'); END;
CREATE TRIGGER machine_order_items_retained BEFORE DELETE ON machine_order_items BEGIN SELECT RAISE(ABORT,'retained machine order item'); END;
CREATE TRIGGER machine_order_items_identity BEFORE UPDATE OF machine_order,item_index,tests,coverage_known,placer,placer_namespace ON machine_order_items BEGIN SELECT RAISE(ABORT,'immutable machine order item identity'); END;
CREATE TRIGGER alert_uncertain_insert AFTER INSERT ON machine_orders WHEN NEW.state='Uncertain' BEGIN
 UPDATE alert_counts SET value=value+1 WHERE kind='uncertain';
END;
CREATE TRIGGER alert_uncertain_transition AFTER UPDATE OF state ON machine_orders BEGIN
 UPDATE alert_counts SET value=value+(NEW.state='Uncertain')-(OLD.state='Uncertain') WHERE kind='uncertain';
END;
ALTER TABLE machine_order_history ADD COLUMN owner_kind TEXT NOT NULL DEFAULT 'none' CHECK(owner_kind IN ('none','reservation','cancellation'));
ALTER TABLE machine_order_history ADD COLUMN owner_id TEXT;
ALTER TABLE machine_order_history ADD COLUMN cancel_version TEXT CHECK(
 (owner_kind='none' AND owner_id IS NULL AND cancel_version IS NULL) OR
 (owner_kind='reservation' AND owner_id IS NOT NULL AND length(owner_id)>0 AND cancel_version IS NULL) OR
 (owner_kind='cancellation' AND (owner_id IS NULL OR length(owner_id)>0) AND cancel_version IS NOT NULL AND length(cancel_version) BETWEEN 1 AND 20 AND cancel_version NOT GLOB '*[^0-9]*' AND substr(cancel_version,1,1)!='0' AND (length(cancel_version)<20 OR cancel_version<='18446744073709551615'))
);
DROP TRIGGER machine_order_history_update;
ALTER TABLE parked ADD COLUMN original_evidence TEXT;
CREATE INDEX machine_order_cancel_version ON machine_order_history(machine_order,cancel_version);
