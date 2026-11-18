# Building a GET /orders cursor that never misses a change

A guide for lab app developers who implement `GET /orders` from
[contract v1](v1.md#get-orderscursoropaquelimit100). Analyte answers machine
queries only from orders it trusts, so a change the cursor skips can send a
cancelled order to a machine or leave a new one unanswered. This page shows a
design that cannot skip a committed change. The
[reference receiver](../../crates/testing/receiver/src/lib.rs) follows it on
SQLite.

## Why timestamps miss changes

A cursor such as "give me orders changed after 10:00:00.200" loses changes in
three ways:

- **Out-of-order commits.** Transaction A stamps 10:00:00.100 and commits at
  .900. Transaction B stamps .200 and commits at .300. A poll at .500 sees B
  and moves the cursor to .200. A commits later with .100 and is never
  returned.
- **Ties.** Two changes share one timestamp and a page ends between them.
  With `>` the second is skipped; with `>=` the client cannot tell new from
  repeated rows.
- **Late commits.** In PostgreSQL, `now()` is the start time of the
  transaction, so a long transaction writes an old stamp when it commits.
  Clock steps and several app servers with different clocks make it worse.

## The safe design

1. A **change log** table: one row per change to an order, holding the full
   order document as served, including its version.
2. A **change number** for each row, taken from a monotonically increasing
   counter **inside the same transaction** that changes the order. Commit
   order then matches number order (see below).
3. The **cursor** carries the last change number returned.
4. A page is `change_no > cursor AND change_no <= head ORDER BY change_no
   LIMIT n`, where `head` is the highest committed number, read when the
   request starts.
5. `complete` is true when the page reaches `head`, even when the page is
   empty. Always return a `next_cursor`; Analyte keeps it for the next poll.

### Concurrent commits

With an ordinary sequence (PostgreSQL `SEQUENCE`, MySQL `AUTO_INCREMENT`),
transaction A can take 41, transaction B can take 42, and B can commit first.
A reader then sees 42, moves its cursor past 41, and misses A.

Use a **single-writer counter row** instead. Each writing transaction updates
one row (`last_no = last_no + 1`) as its last step. The update locks that row
until the transaction commits or rolls back, so the next writer waits and
reads the new value. Numbers are therefore handed out and committed in the
same order: every number at or below the committed head is already visible.
The cost is that order writes are serialized for the short time between the
counter update and commit, which is fine at lab order rates.

The other correct design keeps a sequence and lets readers stop below the
lowest number still in flight (in PostgreSQL by tracking transaction IDs). It
is harder to get right; start with the counter row.

### Gaps from rolled-back numbers

A rolled-back counter update leaves no gap, because the update is undone too.
A database sequence (PostgreSQL `nextval`, MySQL `AUTO_INCREMENT`) is not
rolled back, so a failed transaction leaves a hole. Holes are harmless: the
page uses `>` and never waits for a particular next number, so no committed
change is missed. Out-of-order commits, not holes, are what make sequences
unsafe.

### Cancellations and deletes

Never delete or edit a published row. A cancellation is a new change with a
new version and state `cancel_requested`. A removed order is a new change with
`tombstone: true`, its order ID and a new version; its tests may be empty. A
missing row would look like missing data, which Analyte must not confuse with
"no orders".

### Versions

Each change carries the order's version, raised by one in the same
transaction. Versions only increase, and one version has one content: a
unique `(order_id, version)` key enforces this. Analyte keeps its cached order
and parks the offer for an operator if it ever sees a lower version, or the
same version with different content.

### Snapshot without a cursor

A request without a cursor reads `head` once and pages the newest change of
each order at or below it, by change number. Store that `head` with the
cursor so every snapshot page uses the same ceiling. When the snapshot is
complete, the next cursor starts at `head`, so changes made during paging
come on the next poll.

### Expiry and 410

The contract expires each cursor one hour after it was issued. Answer 410
with the versioned error body (`contract_version`, `error`, `detail`) when:

- the cursor is unknown or older than one hour;
- the cursor is below the compaction floor (below);
- the database was restored from a backup or the log was rebuilt, since change
  numbers may then be reused. Keep a log generation in each cursor and refuse
  older generations.

Never answer an expired cursor with an empty page: that would claim nothing
changed. On 410, Analyte stops trusting its cache for that app, so machine
queries get "lookup failed" rather than "no orders". It then loads a full
snapshot without a cursor and trusts the cache again only when that snapshot
is complete. Orders missing from the new snapshot are not served.

To compact the log, pick a floor that is older than one hour, remove only
changes at or below it that a newer change of the same order replaces (and
old tombstones), and record the floor. Always keep the newest change of every
live order: the snapshot reads it.

### Freshness

Analyte counts freshness from when a poll began, and only after it reaches a
page with `complete: true`. That is correct only if a complete page includes
every change committed before the request began. Reading `head` at the start
of each request guarantees it.

## SQL example (PostgreSQL)

A sketch for app developers; the repository's tests do not run it. Tables. `body` is the exact order JSON served to Analyte.

```sql
CREATE TABLE order_change_head (
    only_row boolean PRIMARY KEY DEFAULT true CHECK (only_row),
    last_no  bigint  NOT NULL,
    log_generation bigint NOT NULL  -- raise after a restore or rebuild
);
INSERT INTO order_change_head VALUES (true, 0, 1);

CREATE TABLE order_changes (
    change_no bigint  PRIMARY KEY,
    order_id  text    NOT NULL,
    version   bigint  NOT NULL CHECK (version > 0),
    tombstone boolean NOT NULL,
    body      text    NOT NULL,
    UNIQUE (order_id, version)
);
CREATE INDEX order_changes_by_order ON order_changes (order_id, change_no);

CREATE TABLE order_cursors (
    token          text PRIMARY KEY,  -- random, at most 256 bytes
    position       bigint NOT NULL,   -- last change number returned
    ceiling        bigint,            -- snapshot head; NULL after the snapshot
    log_generation bigint NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now()
);
```

Write: the order change and its log row in one transaction, at the default
READ COMMITTED level. Take the counter last so its lock is held briefly.

```sql
BEGIN;
UPDATE orders SET state = 'cancel_requested', version = version + 1
 WHERE id = $1 RETURNING version;                    -- -> $version
-- build $body from the updated order, then:
UPDATE order_change_head SET last_no = last_no + 1
 RETURNING last_no;                                  -- -> $no
INSERT INTO order_changes (change_no, order_id, version, tombstone, body)
VALUES ($no, $1, $version, false, $body);
COMMIT;
```

Page after a cursor: look up the token (no row, wrong generation, older than
one hour or below the floor means 410), then read the head and fetch one row
more than the limit.

```sql
SELECT last_no FROM order_change_head;               -- -> $head
SELECT change_no, body FROM order_changes
 WHERE change_no > $position AND change_no <= $head
 ORDER BY change_no
 LIMIT $limit + 1;
```

If `$limit + 1` rows come back, drop the last one and answer
`complete: false`; otherwise answer `complete: true`. Insert a new cursor row
whose position is the last change number returned (or the old position when
the page is empty) and return its token as `next_cursor`.

A snapshot page adds "newest change of its order at or below the ceiling":

```sql
SELECT c.change_no, c.body FROM order_changes c
 WHERE c.change_no > $position AND c.change_no <= $ceiling
   AND NOT EXISTS (SELECT 1 FROM order_changes n
                    WHERE n.order_id = c.order_id
                      AND n.change_no > c.change_no
                      AND n.change_no <= $ceiling)
 ORDER BY c.change_no
 LIMIT $limit + 1;
```

When it is complete, the new cursor has position `$ceiling` and no ceiling.

**Other databases.** SQLite allows one writer at a time, so an
`AUTOINCREMENT` key assigned inside the write transaction is safe; the
reference receiver does this. MySQL has no `UPDATE … RETURNING`: use
`UPDATE order_change_head SET last_no = LAST_INSERT_ID(last_no + 1)` and then
`SELECT LAST_INSERT_ID()` in the same transaction. Do not use MySQL
`AUTO_INCREMENT` for the change number when writers run concurrently.
