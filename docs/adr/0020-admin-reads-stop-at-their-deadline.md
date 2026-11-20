# 0020. Admin reads stop at their deadline; writes never do

Status: accepted, 2026-11-20 (owner decision: stop admin reads at their limit
and keep one store worker, with no second connection)

## Context

The store has one SQLite connection, owned by one worker thread. Every store
request queues there: a machine's message (stored before the machine is
answered, [ADR 0003](0003-clinical-safety-rules.md) rule 1 and
[ADR 0010](0010-acknowledgment-boundaries.md)), deliveries, orders, data
keeping, and the admin pages' reads.

An admin page gives up after 1.5 s and shows "This took too long". Until now
the worker only skipped a request that had not started yet; a read that had
started ran to its end. A slow read therefore held the only store thread after
nobody wanted its answer, and a machine's message queued behind it waited.
The review of 2026-11-20 measured this at ten times a realistic store: one
abandoned page read left seconds of work queued, and the Setup check before
removing a machine took 77 to 153 s.

## Decision

- **A separate kind of request for reads that may be stopped:**
  `Handle::read`. Its work gets a shared `&Store`. Every store method that
  writes takes `&mut Store`, so such work cannot call one; the compiler
  refuses it. `Handle::request` is unchanged and is the only way to write.
- **Who uses it:** the admin pages' reads, `GET /status` and the `/events`
  change check. Anything that writes, even beside a read (showing the original
  message records an audit entry; opening the reprocess dialog may rebuild a
  receipt), stays a `request`. Intake, delivery, orders and data keeping never
  use `read`.
- **When a read stops:** once its deadline passes, or once its caller has gone
  (the page gave up and dropped its future). Still queued, it is skipped and
  never runs. Already running, it is stopped by a SQLite progress handler that
  checks the deadline and the caller every 1000 virtual machine steps.
- **Why a progress handler, not `sqlite3_interrupt`:** the handler runs on the
  worker thread, inside the read's own statements, and is disarmed before the
  next request starts. An interrupt sent from another thread could arrive just
  after the read finished and stop the request behind it, which may be a
  machine's message.
- **Guards around every read:**
  - it starts only outside any transaction; if a transaction is open, the read
    is refused (`ReadInsideTransaction`) and the transaction is left alone;
  - it runs with `PRAGMA query_only`, so SQLite refuses any write it is asked
    for, before a page changes;
  - afterwards the handler is disarmed and removed, any transaction is closed
    and `query_only` is turned off. A guard that cannot be cleared reports
    `ReadGuard`; left in place it refuses writes and never stops one.
- **Errors stay specific:** a stopped read returns `ReadStopped` ("store read
  stopped at its deadline; nothing was changed"), and the admin pages show it
  as the same "This took too long" page (504) they show when they give up.

## Guarantees for writes

A `request` is never skipped once it has started, never interrupted and never
run with a progress handler that can fire: its caller's deadline passing or
its caller going away mid-transaction changes nothing, and it commits whole or
fails with its own error, as before. Tests cover a write whose caller times out
and goes while its transaction is open, right after a stopped read.

## Measured effect

Release build, the review's synthetic stores (x1: 50,000 messages; x10:
500,000).

- A machine's message queued behind an admin read that the page gave up on
  at 1.5 s: with the old request it waited for the whole read, **3.0–3.5 s**
  behind a controlled 3 s read and **133 s** behind the old check before
  removing a machine at x10; with a read that stops, **1.51 s** in every case
  (the page's limit, plus about 10 ms to stop and store the message).
- The check before removing a machine, the slowest read found: its parked-item
  count compared every machine order with every target. It now finds the order
  by its key, with the same answers: **0.5–1.0 s → 25–44 ms** at x1 and
  **77–153 s → 0.26–0.44 s** at x10.

## Consequences

- No admin read can hold the store thread much past its page's limit, so
  admin browsing cannot hold back a machine's message for longer than that.
- A slow read is still slow; it now fails visibly at its limit instead of
  delaying everything behind it. Making reads fast stays the job of the queries
  and indexes.
- A read that needs to write, or a write that needs to read first, uses
  `request` and keeps its old behaviour.
