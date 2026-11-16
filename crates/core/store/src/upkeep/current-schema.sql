-- Current schema, ordered by object type and name. Checked against Store::open.
CREATE INDEX alert_events_condition ON alert_events(endpoint,condition);
CREATE INDEX alert_events_endpoint ON alert_events(endpoint,observed,id);
CREATE INDEX alert_history_episode ON alert_history(key,active,sequence);
CREATE INDEX alert_oldest_pending ON outbox(enqueued_at) WHERE state!='done';
CREATE UNIQUE INDEX alert_one_reminder ON alert_events(endpoint,condition) WHERE kind='reminder';
CREATE INDEX alert_progress_due ON alert_progress(exhausted,next_attempt,event);
CREATE INDEX awos_execution_item ON awos_execution_history(machine_order,item_index,sequence);
CREATE INDEX law_message_identity ON law_messages(scope,message);
CREATE INDEX law_work_item ON law_work_history(machine_order,item_index,sequence);
CREATE INDEX machine_order_cancel_version ON machine_order_history(machine_order,cancel_version);
CREATE INDEX machine_order_item_history_lookup ON machine_order_item_history(machine_order,item_index,sequence);
CREATE INDEX poll_intents_machine_state ON poll_intents(machine,state,id);
CREATE INDEX retention_results_receipt ON retention_results(receipt);
CREATE TABLE alert_counts(kind TEXT PRIMARY KEY, value INTEGER NOT NULL CHECK(typeof(value)='integer' AND value>=0));
CREATE TABLE alert_endpoints(id TEXT PRIMARY KEY, cursor INTEGER NOT NULL DEFAULT 0, queue_full INTEGER NOT NULL DEFAULT 0 CHECK(queue_full IN (0,1)), last_success INTEGER);
CREATE TABLE alert_events(id TEXT PRIMARY KEY, endpoint TEXT NOT NULL REFERENCES alert_endpoints(id), condition TEXT NOT NULL, episode INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('raised','reminder','resolved')), observed INTEGER NOT NULL, payload BLOB NOT NULL CHECK(length(payload)<=16384));
CREATE TABLE alert_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT NOT NULL REFERENCES alert_state(key),
 active INTEGER NOT NULL CHECK(active IN (0,1)), time INTEGER NOT NULL,
 responder TEXT NOT NULL, value TEXT CHECK(value NOT GLOB '*[^0-9]*' AND length(value) BETWEEN 1 AND 20)
);
CREATE TABLE alert_installation(singleton INTEGER PRIMARY KEY CHECK(singleton=1), identity TEXT NOT NULL);
CREATE TABLE alert_progress(event TEXT PRIMARY KEY REFERENCES alert_events(id) ON DELETE CASCADE, attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0), next_attempt INTEGER NOT NULL, exhausted INTEGER NOT NULL DEFAULT 0 CHECK(exhausted IN (0,1)), failure TEXT CHECK(failure IN ('network','credentials','http','interrupted')));
CREATE TABLE alert_reminders(endpoint TEXT NOT NULL REFERENCES alert_endpoints(id), condition TEXT NOT NULL, episode INTEGER NOT NULL, next_reminder INTEGER NOT NULL, PRIMARY KEY(endpoint,condition));
CREATE TABLE alert_state (
 key TEXT PRIMARY KEY, responder TEXT NOT NULL CHECK(length(responder) BETWEEN 1 AND 256),
 active INTEGER NOT NULL CHECK(active IN (0,1)), raised_at INTEGER, cleared_at INTEGER,
 value TEXT CHECK(value NOT GLOB '*[^0-9]*' AND length(value) BETWEEN 1 AND 20), evaluated_at INTEGER NOT NULL,
 CHECK(active=0 OR raised_at IS NOT NULL)
);
CREATE TABLE attempts(sequence INTEGER PRIMARY KEY AUTOINCREMENT, delivery TEXT NOT NULL REFERENCES outbox(id), outcome TEXT NOT NULL, time INTEGER NOT NULL);
CREATE TABLE audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT, evidence TEXT NOT NULL);
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
CREATE TABLE feed(destination TEXT PRIMARY KEY, cursor TEXT NOT NULL, complete INTEGER NOT NULL, trusted INTEGER NOT NULL, polled INTEGER NOT NULL, generation INTEGER NOT NULL DEFAULT 0);
CREATE TABLE frame_provenance(transfer TEXT PRIMARY KEY, evidence TEXT NOT NULL);
CREATE TABLE "frames"(sequence INTEGER PRIMARY KEY AUTOINCREMENT, transfer TEXT NOT NULL REFERENCES transfers(id), number INTEGER NOT NULL CHECK(number BETWEEN 0 AND 255), raw BLOB NOT NULL);
CREATE TABLE interpretations(receipt TEXT NOT NULL REFERENCES receipts(id), version INTEGER NOT NULL CHECK(version>0), evidence TEXT NOT NULL, PRIMARY KEY(receipt,version));
CREATE TABLE law_messages(receipt TEXT NOT NULL,version INTEGER NOT NULL,scope TEXT NOT NULL,message TEXT NOT NULL,digest BLOB NOT NULL,disposition TEXT NOT NULL,evidence TEXT NOT NULL,PRIMARY KEY(receipt,version),FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version));
CREATE TABLE law_work_history(sequence INTEGER PRIMARY KEY AUTOINCREMENT,receipt TEXT NOT NULL,version INTEGER NOT NULL,machine_order INTEGER NOT NULL,item_index INTEGER NOT NULL,evidence TEXT NOT NULL,FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version),FOREIGN KEY(machine_order,item_index) REFERENCES machine_order_items(machine_order,item_index));
CREATE TABLE machine_activity(machine TEXT PRIMARY KEY, time INTEGER NOT NULL);
CREATE TABLE machine_order_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 machine_order INTEGER NOT NULL REFERENCES machine_orders(id),
 evidence TEXT NOT NULL
, owner_kind TEXT NOT NULL DEFAULT 'none' CHECK(owner_kind IN ('none','reservation','cancellation')), owner_id TEXT, cancel_version TEXT CHECK(
 (owner_kind='none' AND owner_id IS NULL AND cancel_version IS NULL) OR
 (owner_kind='reservation' AND owner_id IS NOT NULL AND length(owner_id)>0 AND cancel_version IS NULL) OR
 (owner_kind='cancellation' AND (owner_id IS NULL OR length(owner_id)>0) AND cancel_version IS NOT NULL AND length(cancel_version) BETWEEN 1 AND 20 AND cancel_version NOT GLOB '*[^0-9]*' AND substr(cancel_version,1,1)!='0' AND (length(cancel_version)<20 OR cancel_version<='18446744073709551615'))
));
CREATE TABLE machine_order_item_history (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 machine_order INTEGER NOT NULL,
 item_index INTEGER NOT NULL,
 evidence TEXT NOT NULL,
 FOREIGN KEY(machine_order,item_index) REFERENCES machine_order_items(machine_order,item_index)
);
CREATE TABLE "machine_order_items" (
 machine_order INTEGER NOT NULL REFERENCES machine_orders(id),
 item_index INTEGER NOT NULL CHECK(item_index BETWEEN 1 AND 100),
 tests TEXT NOT NULL,
 coverage_known INTEGER NOT NULL CHECK(coverage_known IN (0,1)),
 placer TEXT NOT NULL UNIQUE,
 placer_namespace TEXT,
 state TEXT NOT NULL CHECK(state IN ('Sending','Sent','Accepted','Done','Rejected','CancelRequested','CancelConfirmed','Uncertain')),
 PRIMARY KEY(machine_order,item_index)
);
CREATE TABLE "machine_orders" (
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
CREATE TABLE observation_revisions(identity_key TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0), receipt TEXT NOT NULL, interpretation_version INTEGER NOT NULL, content_digest BLOB NOT NULL CHECK(length(content_digest)=32), PRIMARY KEY(identity_key,revision), FOREIGN KEY(receipt,interpretation_version) REFERENCES interpretations(receipt,version));
CREATE TABLE orders(destination TEXT NOT NULL REFERENCES feed(destination), id TEXT NOT NULL, sample TEXT NOT NULL, evidence TEXT NOT NULL, tombstone INTEGER NOT NULL, expiry INTEGER NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(destination,id));
CREATE TABLE "outbox"(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 id TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL DEFAULT 'result' CHECK(kind IN ('result','order_event')),
 receipt TEXT,
 version INTEGER,
 order_id TEXT,
 destination TEXT NOT NULL,
 ordering_key TEXT NOT NULL,
 payload BLOB NOT NULL,
 digest BLOB NOT NULL,
 contract INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','in_flight','done','parked','paused','conflict')),
 next_attempt INTEGER NOT NULL,
 enqueued_at INTEGER NOT NULL DEFAULT (unixepoch()),
 enqueue_time_backfilled INTEGER NOT NULL DEFAULT 0 CHECK(enqueue_time_backfilled IN (0,1)),
 CHECK((kind='result' AND receipt IS NOT NULL AND version IS NOT NULL AND order_id IS NULL)
    OR (kind='order_event' AND receipt IS NULL AND version IS NULL AND order_id IS NOT NULL AND ordering_key=order_id)),
 FOREIGN KEY(receipt,version) REFERENCES interpretations(receipt,version)
);
CREATE TABLE parked(id INTEGER PRIMARY KEY AUTOINCREMENT, evidence TEXT NOT NULL, resolution TEXT, original_evidence TEXT);
CREATE TABLE poll_intent_history(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 intent INTEGER NOT NULL REFERENCES poll_intents(id),
 state TEXT NOT NULL CHECK(state IN ('pending','confirmed','uncertain')),
 reason TEXT NOT NULL CHECK(length(CAST(reason AS BLOB)) BETWEEN 1 AND 1024),
 audit TEXT
);
CREATE TABLE poll_intents(
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 machine TEXT NOT NULL CHECK(length(CAST(machine AS BLOB)) BETWEEN 1 AND 256),
 parked INTEGER NOT NULL REFERENCES parked(id),
 state TEXT NOT NULL CHECK(state IN ('pending','confirmed','uncertain')),
 reason TEXT NOT NULL CHECK(length(CAST(reason AS BLOB)) BETWEEN 1 AND 1024),
 UNIQUE(parked,machine)
);
CREATE TABLE receipts(id TEXT PRIMARY KEY, metadata TEXT NOT NULL, raw BLOB NOT NULL, config BLOB NOT NULL);
CREATE TABLE retention_deliveries(delivery TEXT PRIMARY KEY, receipt TEXT NOT NULL REFERENCES retention_tombstones(receipt), acknowledged_at INTEGER NOT NULL);
CREATE TABLE retention_holds(kind TEXT NOT NULL CHECK(kind IN ('receipt','order')), target TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), PRIMARY KEY(kind,target));
CREATE TABLE retention_key(singleton INTEGER PRIMARY KEY CHECK(singleton=1), fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32));
CREATE TABLE retention_order_deliveries(delivery TEXT PRIMARY KEY, order_tombstone INTEGER NOT NULL REFERENCES retention_orders(id), acknowledged_at INTEGER NOT NULL);
CREATE TABLE retention_orders(id INTEGER PRIMARY KEY, identity_digest BLOB NOT NULL UNIQUE CHECK(length(identity_digest)=32), machine TEXT NOT NULL, order_digest BLOB NOT NULL CHECK(length(order_digest)=32), sample_digest BLOB CHECK(sample_digest IS NULL OR length(sample_digest)=32), original_digest BLOB CHECK(original_digest IS NULL OR length(original_digest)=32), created_at INTEGER NOT NULL, finalized_at INTEGER NOT NULL, purged_at INTEGER NOT NULL);
CREATE TABLE retention_results(identity_digest BLOB NOT NULL CHECK(length(identity_digest)=32), revision INTEGER NOT NULL CHECK(revision>=0), receipt TEXT NOT NULL REFERENCES retention_tombstones(receipt), order_digest BLOB NOT NULL CHECK(length(order_digest)=32), sample_digest BLOB NOT NULL CHECK(length(sample_digest)=32), run_digest BLOB NOT NULL CHECK(length(run_digest)=32), content_digest BLOB NOT NULL CHECK(length(content_digest)=32), PRIMARY KEY(identity_digest,revision));
CREATE TABLE retention_tombstones(receipt TEXT PRIMARY KEY, machine TEXT NOT NULL, driver TEXT NOT NULL, config_version TEXT NOT NULL, original_digest BLOB NOT NULL CHECK(length(original_digest)=32), received_at INTEGER NOT NULL, acknowledged_at INTEGER NOT NULL, purged_at INTEGER NOT NULL);
CREATE TABLE transfers(id TEXT PRIMARY KEY, state TEXT NOT NULL CHECK(state IN ('open','aborted','complete')), receipt TEXT REFERENCES receipts(id));
CREATE TRIGGER alert_events_immutable BEFORE UPDATE ON alert_events BEGIN SELECT RAISE(ABORT,'immutable alert event'); END;
CREATE TRIGGER alert_history_retained_delete BEFORE DELETE ON alert_history BEGIN SELECT RAISE(ABORT,'retained alert history'); END;
CREATE TRIGGER alert_history_retained_update BEFORE UPDATE ON alert_history BEGIN SELECT RAISE(ABORT,'immutable alert history'); END;
CREATE TRIGGER alert_installation_immutable BEFORE UPDATE ON alert_installation BEGIN SELECT RAISE(ABORT,'immutable alert installation'); END;
CREATE TRIGGER alert_installation_retained BEFORE DELETE ON alert_installation BEGIN SELECT RAISE(ABORT,'retained alert installation'); END;
CREATE TRIGGER alert_parked_insert AFTER INSERT ON parked WHEN NEW.resolution IS NULL BEGIN
 UPDATE alert_counts SET value=value+1 WHERE kind='parked';
END;
CREATE TRIGGER alert_parked_resolution AFTER UPDATE OF resolution ON parked BEGIN
 UPDATE alert_counts SET value=value+(NEW.resolution IS NULL)-(OLD.resolution IS NULL) WHERE kind='parked';
END;
CREATE TRIGGER alert_state_retained BEFORE DELETE ON alert_state BEGIN SELECT RAISE(ABORT,'retained alert state'); END;
CREATE TRIGGER alert_uncertain_insert AFTER INSERT ON machine_orders WHEN NEW.state='Uncertain' BEGIN
 UPDATE alert_counts SET value=value+1 WHERE kind='uncertain';
END;
CREATE TRIGGER alert_uncertain_transition AFTER UPDATE OF state ON machine_orders BEGIN
 UPDATE alert_counts SET value=value+(NEW.state='Uncertain')-(OLD.state='Uncertain') WHERE kind='uncertain';
END;
CREATE TRIGGER awos_execution_immutable BEFORE UPDATE ON awos_execution_history BEGIN SELECT RAISE(ABORT,'immutable AWOS execution evidence'); END;
CREATE TRIGGER awos_execution_retained BEFORE DELETE ON awos_execution_history BEGIN SELECT RAISE(ABORT,'retained AWOS execution evidence'); END;
CREATE TRIGGER frame_activity AFTER INSERT ON frames WHEN EXISTS(SELECT 1 FROM frame_provenance WHERE transfer=NEW.transfer) BEGIN
 INSERT INTO machine_activity SELECT json_extract(evidence,'$.machine'),unixepoch() FROM frame_provenance WHERE transfer=NEW.transfer
 ON CONFLICT(machine) DO UPDATE SET time=max(time,excluded.time);
END;
CREATE TRIGGER frame_provenance_immutable_delete BEFORE DELETE ON frame_provenance BEGIN SELECT RAISE(ABORT,'immutable frame provenance'); END;
CREATE TRIGGER frame_provenance_immutable_update BEFORE UPDATE ON frame_provenance BEGIN SELECT RAISE(ABORT,'immutable frame provenance'); END;
CREATE TRIGGER immutable_attempts_delete BEFORE DELETE ON attempts BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_attempts_update BEFORE UPDATE ON attempts BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_audit_delete BEFORE DELETE ON audit BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_audit_update BEFORE UPDATE ON audit BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_frames_delete BEFORE DELETE ON frames BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_frames_update BEFORE UPDATE ON frames BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_interpretations_delete BEFORE DELETE ON interpretations BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_interpretations_update BEFORE UPDATE ON interpretations BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_receipts_delete BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER immutable_receipts_update BEFORE UPDATE ON receipts BEGIN SELECT RAISE(ABORT,'immutable evidence'); END;
CREATE TRIGGER law_messages_delete BEFORE DELETE ON law_messages BEGIN SELECT RAISE(ABORT,'retained LAW disposition'); END;
CREATE TRIGGER law_messages_update BEFORE UPDATE ON law_messages BEGIN SELECT RAISE(ABORT,'immutable LAW disposition'); END;
CREATE TRIGGER law_work_delete BEFORE DELETE ON law_work_history BEGIN SELECT RAISE(ABORT,'retained LAW evidence'); END;
CREATE TRIGGER law_work_update BEFORE UPDATE ON law_work_history BEGIN SELECT RAISE(ABORT,'immutable LAW evidence'); END;
CREATE TRIGGER machine_order_history_delete BEFORE DELETE ON machine_order_history BEGIN SELECT RAISE(ABORT,'retained lifecycle history'); END;
CREATE TRIGGER machine_order_history_update BEFORE UPDATE ON machine_order_history BEGIN SELECT RAISE(ABORT,'immutable lifecycle history'); END;
CREATE TRIGGER machine_order_item_history_delete BEFORE DELETE ON machine_order_item_history BEGIN SELECT RAISE(ABORT,'retained item history'); END;
CREATE TRIGGER machine_order_item_history_update BEFORE UPDATE ON machine_order_item_history BEGIN SELECT RAISE(ABORT,'immutable item history'); END;
CREATE TRIGGER machine_order_items_identity BEFORE UPDATE OF machine_order,item_index,tests,coverage_known,placer,placer_namespace ON machine_order_items BEGIN SELECT RAISE(ABORT,'immutable machine order item identity'); END;
CREATE TRIGGER machine_order_items_retained BEFORE DELETE ON machine_order_items BEGIN SELECT RAISE(ABORT,'retained machine order item'); END;
CREATE TRIGGER machine_orders_identity BEFORE UPDATE OF id,destination,order_id,machine,version,created_at ON machine_orders BEGIN SELECT RAISE(ABORT,'immutable machine order identity'); END;
CREATE TRIGGER machine_orders_retained BEFORE DELETE ON machine_orders BEGIN SELECT RAISE(ABORT,'retained machine order'); END;
CREATE TRIGGER observation_revisions_immutable_delete BEFORE DELETE ON observation_revisions BEGIN SELECT RAISE(ABORT,'immutable revision'); END;
CREATE TRIGGER observation_revisions_immutable_update BEFORE UPDATE ON observation_revisions BEGIN SELECT RAISE(ABORT,'immutable revision'); END;
CREATE TRIGGER outbox_immutable BEFORE UPDATE OF id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,enqueued_at,enqueue_time_backfilled ON outbox BEGIN SELECT RAISE(ABORT,'immutable delivery'); END;
CREATE TRIGGER outbox_retained BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT,'retained delivery'); END;
CREATE TRIGGER parked_evidence_immutable BEFORE UPDATE OF evidence,original_evidence ON parked BEGIN SELECT RAISE(ABORT,'immutable parking evidence'); END;
CREATE TRIGGER parked_retained BEFORE DELETE ON parked BEGIN SELECT RAISE(ABORT,'retained parked item'); END;
CREATE TRIGGER poll_intent_history_delete BEFORE DELETE ON poll_intent_history BEGIN SELECT RAISE(ABORT,'retained poll history'); END;
CREATE TRIGGER poll_intent_history_update BEFORE UPDATE ON poll_intent_history BEGIN SELECT RAISE(ABORT,'immutable poll history'); END;
CREATE TRIGGER poll_intents_identity BEFORE UPDATE OF id,machine,parked ON poll_intents BEGIN SELECT RAISE(ABORT,'immutable poll identity'); END;
CREATE TRIGGER poll_intents_lifecycle BEFORE UPDATE OF state ON poll_intents
 WHEN NEW.state='pending' OR (OLD.state='confirmed' AND NEW.state!='confirmed')
 BEGIN SELECT RAISE(ABORT,'poll outcome cannot authorize replay'); END;
CREATE TRIGGER poll_intents_retained BEFORE DELETE ON poll_intents BEGIN SELECT RAISE(ABORT,'retained poll intent'); END;
CREATE TRIGGER receipt_activity AFTER INSERT ON receipts BEGIN
 INSERT INTO machine_activity VALUES(json_extract(NEW.metadata,'$.machine'),unixepoch(json_extract(NEW.metadata,'$.received_at')))
 ON CONFLICT(machine) DO UPDATE SET time=max(time,excluded.time);
END;
CREATE TRIGGER retention_deliveries_delete BEFORE DELETE ON retention_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_deliveries_update BEFORE UPDATE ON retention_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_key_delete BEFORE DELETE ON retention_key BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_key_update BEFORE UPDATE ON retention_key BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_order_deliveries_delete BEFORE DELETE ON retention_order_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_order_deliveries_update BEFORE UPDATE ON retention_order_deliveries BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_orders_delete BEFORE DELETE ON retention_orders BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_orders_update BEFORE UPDATE ON retention_orders BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_results_delete BEFORE DELETE ON retention_results BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_results_update BEFORE UPDATE ON retention_results BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_tombstones_delete BEFORE DELETE ON retention_tombstones BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
CREATE TRIGGER retention_tombstones_update BEFORE UPDATE ON retention_tombstones BEGIN SELECT RAISE(ABORT,'immutable retention evidence'); END;
