-- Step 15 to 16: when each Needs review item was parked, for the Activity page
-- (ADR 0018). The column is NULL where no stored time says it; pages then show
-- "Not recorded". Nothing is guessed.
--
-- New items: the trigger below sets parked_at in the same write that parks the
-- item, from the store's clock (unixepoch(), as outbox.enqueued_at and
-- machine_activity use). No writer changes, and once set the time never changes.
--
-- Items already stored: the only time written in the same transaction as the
-- parking is that of a delivery the lab app rejected (kind 'parked') or holds
-- differently (kind 'conflict'): the `attempts` row whose outcome is Parked or
-- Conflict, written by `attempted` together with the item. A delivery parked
-- more than once has as many such attempts as items, so the n-th item of a
-- delivery and kind takes the time of its n-th attempt with that outcome.
-- Every other kind (a message that could not be read, a mapping finding, an
-- incomplete transfer, a refused order version, a cancel conflict, a request
-- for results) was parked with no time beside it: a receipt's arrival or an
-- order's update is not when it was parked, so those items keep NULL.
ALTER TABLE parked ADD COLUMN parked_at INTEGER CHECK(parked_at IS NULL OR typeof(parked_at)='integer');
WITH items AS (
 SELECT id,
  json_quote(json_extract(evidence,'$.targets[0]')) AS delivery,
  CASE json_extract(evidence,'$.kind') WHEN 'parked' THEN 'Parked' ELSE 'Conflict' END AS outcome,
  row_number() OVER (PARTITION BY json_extract(evidence,'$.targets[0]'),json_extract(evidence,'$.kind') ORDER BY id) AS n
 FROM parked
 WHERE json_valid(evidence) AND json_extract(evidence,'$.kind') IN ('parked','conflict')
),
tries AS (
 SELECT delivery, outcome, time,
  row_number() OVER (PARTITION BY delivery,outcome ORDER BY sequence) AS n
 FROM (
  SELECT delivery, time, sequence,
   CASE WHEN json_type(outcome,'$.Parked') IS NOT NULL THEN 'Parked'
        WHEN json_type(outcome,'$.Conflict') IS NOT NULL THEN 'Conflict' END AS outcome
  FROM attempts WHERE json_valid(outcome) AND json_type(outcome)='object'
 ) WHERE outcome IS NOT NULL
)
UPDATE parked SET parked_at=(
 SELECT t.time FROM items i JOIN tries t ON t.delivery=i.delivery AND t.outcome=i.outcome AND t.n=i.n
 WHERE i.id=parked.id
) WHERE id IN (SELECT id FROM items);
CREATE TRIGGER parked_time AFTER INSERT ON parked WHEN NEW.parked_at IS NULL BEGIN
 UPDATE parked SET parked_at=unixepoch() WHERE id=NEW.id;
END;
CREATE TRIGGER parked_time_immutable BEFORE UPDATE OF parked_at ON parked WHEN OLD.parked_at IS NOT NULL BEGIN SELECT RAISE(ABORT,'immutable parking time'); END;
PRAGMA user_version=16;
