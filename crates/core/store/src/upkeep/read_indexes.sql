-- Step 16 to 17: indexes for the admin pages' reads, so Overview, Machines,
-- Lab apps, Data keeping, Activity and GET /status read the newest rows they
-- need instead of every kept row. Every stored value stays as it was.
--
-- Two helper columns are added to outbox, because no index can join attempts to
-- their delivery's lab app: when each delivery was last accepted
-- (delivered_at, the latest time of its "Done" attempts) and by which attempt
-- (delivered_attempt, the highest sequence of its "Done" attempts). Both are
-- read from the attempts already kept, NULL for a delivery never accepted, and
-- kept current by the trigger below in the same write as the attempt.
--
-- An index expression must give the same value every time, and a write fails if
-- it raises an error. Receipt metadata is always JSON (the receipt_activity
-- trigger reads it on every insert), as are audit entries and order snapshots;
-- a parked item's resolution is read only where it is JSON.

-- Receipts by received second, and by machine then received second.
CREATE INDEX receipts_received ON receipts(unixepoch(json_extract(metadata,'$.received_at')));
CREATE INDEX receipts_machine_received ON receipts(json_extract(metadata,'$.machine'),unixepoch(json_extract(metadata,'$.received_at')));

-- Parked items by parking time, and resolved items by resolution time.
CREATE INDEX parked_time ON parked(parked_at);
CREATE INDEX parked_resolved_time ON parked(CASE WHEN json_valid(resolution) THEN json_extract(resolution,'$.time') END) WHERE resolution IS NOT NULL;

-- Attempts by delivery, and by accepted or not, then time.
CREATE INDEX attempts_delivery ON attempts(delivery);
CREATE INDEX attempts_accepted_time ON attempts(outcome='"Done"',time);

-- Deliveries by lab app and state, by receipt, and by when last accepted.
CREATE INDEX outbox_destination_state ON outbox(destination,state);
CREATE INDEX outbox_receipt ON outbox(receipt);
-- The delivery worker's claim looks up earlier deliveries of the same lab app,
-- kind and ordering key; without this the indexes by lab app above would make
-- that lookup read every delivery of the lab app.
CREATE INDEX outbox_ordering ON outbox(destination,kind,ordering_key);
ALTER TABLE outbox ADD COLUMN delivered_at INTEGER CHECK(delivered_at IS NULL OR typeof(delivered_at)='integer');
ALTER TABLE outbox ADD COLUMN delivered_attempt INTEGER CHECK(delivered_attempt IS NULL OR typeof(delivered_attempt)='integer');
UPDATE outbox SET
 delivered_at=(SELECT max(a.time) FROM attempts a WHERE a.delivery=outbox.id AND a.outcome='"Done"'),
 delivered_attempt=(SELECT max(a.sequence) FROM attempts a WHERE a.delivery=outbox.id AND a.outcome='"Done"')
WHERE id IN (SELECT delivery FROM attempts WHERE outcome='"Done"');
-- Only accepted deliveries: other reads by lab app then use the indexes above.
CREATE INDEX outbox_delivered_at ON outbox(destination,delivered_at) WHERE delivered_at IS NOT NULL;
CREATE INDEX outbox_delivered_attempt ON outbox(destination,delivered_attempt) WHERE delivered_attempt IS NOT NULL;
CREATE TRIGGER outbox_delivered AFTER INSERT ON attempts WHEN NEW.outcome='"Done"' BEGIN
 UPDATE outbox SET delivered_at=max(coalesce(delivered_at,NEW.time),NEW.time),
  delivered_attempt=max(coalesce(delivered_attempt,NEW.sequence),NEW.sequence)
 WHERE id=NEW.delivery;
END;

-- Order snapshots by update time, and by machine then update time.
CREATE INDEX machine_order_history_updated ON machine_order_history(json_extract(evidence,'$.updated_at'));
CREATE INDEX machine_order_history_machine_updated ON machine_order_history(json_extract(evidence,'$.key.machine'),json_extract(evidence,'$.updated_at'));

-- Audit entries by time.
CREATE INDEX audit_time ON audit(json_extract(evidence,'$.time'));
PRAGMA user_version=17;
