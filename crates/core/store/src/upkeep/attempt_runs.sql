-- Step 17 to 18: the runs of failed delivery tries, which Activity shows as one
-- line each. Every stored value stays as it was.
--
-- While a lab app is down, every waiting delivery retries on its own clock.
-- Activity folds a delivery's failed tries in a row that ended the same way
-- into one line, at the newest of them. Whether a try folds depends on the
-- delivery's next try, which no index can hold, so without this table the feed
-- would read every folded try to fill a page. Each row here is one run instead:
-- every try that was not accepted is in exactly one run. The rows are read from
-- the attempts already kept and kept current by the triggers below, in the
-- same write as each attempt.
--
-- A try's kind is 1 when it was cut off, 2 when it failed with a retry time,
-- and NULL for a rejection, a pause or a conflict, which always stands alone.
-- A try joins the run whose newest try is its delivery's previous try, when
-- both have the same kind; any other try, an accepted one included, ends it.
-- `first` and `last` are the run's first and newest tries, `time` is when the
-- newest was stored and `count` how many tries it holds.
CREATE TABLE attempt_runs(
 first INTEGER PRIMARY KEY,
 delivery TEXT NOT NULL,
 kind INTEGER CHECK(kind IS NULL OR kind IN (1,2)),
 last INTEGER NOT NULL UNIQUE,
 time INTEGER NOT NULL,
 count INTEGER NOT NULL CHECK(count>=1),
 CHECK(last>=first AND (kind IS NOT NULL OR count=1))
);
INSERT INTO attempt_runs(first,delivery,kind,last,time,count)
SELECT r.first,r.delivery,r.kind,r.last,(SELECT a.time FROM attempts a WHERE a.sequence=r.last),r.count
FROM (
 SELECT min(sequence) AS first,delivery,kind,max(sequence) AS last,count(*) AS count
 FROM (
  SELECT sequence,delivery,outcome,kind,
   sum(CASE WHEN kind IS NOT NULL AND kind IS previous THEN 0 ELSE 1 END) OVER (PARTITION BY delivery ORDER BY sequence) AS run
  FROM (
   SELECT sequence,delivery,outcome,kind,lag(kind) OVER (PARTITION BY delivery ORDER BY sequence) AS previous
   FROM (
    SELECT sequence,delivery,outcome,
     CASE WHEN outcome='"uncertain (interrupted)"' THEN 1 WHEN substr(outcome,1,11)='{"RetryAt":' THEN 2 END AS kind
    FROM attempts
   )
  )
 )
 WHERE outcome!='"Done"'
 GROUP BY delivery,run
) r;
-- Activity's newest lines first, and one delivery's runs.
CREATE INDEX attempt_runs_time ON attempt_runs(time,last);
CREATE INDEX attempt_runs_delivery ON attempt_runs(delivery);
CREATE TRIGGER attempt_runs_grow AFTER INSERT ON attempts WHEN NEW.outcome!='"Done"' BEGIN
 UPDATE attempt_runs SET last=NEW.sequence,time=NEW.time,count=count+1
 WHERE last=(SELECT max(p.sequence) FROM attempts p WHERE p.delivery=NEW.delivery AND p.sequence<NEW.sequence)
 AND kind=(CASE WHEN NEW.outcome='"uncertain (interrupted)"' THEN 1 WHEN substr(NEW.outcome,1,11)='{"RetryAt":' THEN 2 END);
 INSERT INTO attempt_runs(first,delivery,kind,last,time,count)
 SELECT NEW.sequence,NEW.delivery,
  CASE WHEN NEW.outcome='"uncertain (interrupted)"' THEN 1 WHEN substr(NEW.outcome,1,11)='{"RetryAt":' THEN 2 END,
  NEW.sequence,NEW.time,1
 WHERE NOT EXISTS(SELECT 1 FROM attempt_runs WHERE last=NEW.sequence);
END;
-- Attempts are deleted only when data keeping removes a delivery's tries, all
-- of them in one write; its runs go with them.
CREATE TRIGGER attempt_runs_removed AFTER DELETE ON attempts BEGIN
 DELETE FROM attempt_runs WHERE delivery=OLD.delivery;
END;
PRAGMA user_version=18;
