# analyte-store

One protected SQLite database holds receipt evidence, the ASTM frame journal,
versioned readings, parked items, immutable deliveries, attempts, order pages and
audit entries. The host must protect the database directory (including WAL, SHM
and backup files); nothing here logs clinical payloads.

## Durability and recovery

Opening runs `integrity_check` and `foreign_key_check`. Schema version 5 is
created by forward-only migrations from version 1; newer versions are refused. Migration preserves all rows and immutability triggers.
Existing enqueue times are backfilled with migration time and explicitly marked.
Migrations only move forward. WAL and `synchronous=FULL` deliberately require
SQLite to sync each committed WAL transaction before success is returned.
Foreign keys are enabled on every connection; lock waits have a configured
nonzero timeout capped at 30 seconds. These guarantees still require a local
filesystem and storage hardware that honor flush requests. See SQLite's
[PRAGMA documentation](https://sqlite.org/pragma.html).

`frame` commits independently before returning the sequence a dialog may ACK.
Incomplete and aborted transfers retain every frame; recovery is paged by
transfer ID and sequence. `finish_transfer` can link only to an existing receipt.
Receipts include exact effective config bytes and checked SHA-256 digests.
There is no receipt/frame/interpretation deletion or update API, and SQL triggers
reject accidental changes. Reprocessing creates another interpretation version.

Interpretations and their deliveries commit together. A parse failure, an unsafe
result or a rejected/conflicting delivery creates operator evidence. Outbox IDs,
payloads, digests, destinations and keys cannot change. Claiming is serialized
through the writer: every unresolved earlier delivery blocks its own destination
and observation key, while unrelated keys can proceed. Setup pause blocks the
whole destination. At startup, before starting the sole worker, the host calls
`recover_in_flight(now, audit)` until it returns zero. Each bounded page commits
atomically: only interrupted `in_flight` app deliveries receive an Interrupted
attempt (uncertain outcome), a separate audit naming the delivery and ADR 0008
idempotent receiver reason, and immediate pending eligibility. IDs and exact bytes
never change. A startup failure must prevent worker startup; retrying recovery is
safe. Parked, conflict and paused entries require audited operator release.
This store never sends orders to a machine.

Order pages and cursors commit together. A 410 reset increments a snapshot
generation, so an order absent from a replacement snapshot cannot become active
again. Trust requires a complete snapshot and a strictly fresh successful poll;
a backward clock or too many matching rows fails closed. Expired orders and
tombstones cannot be returned as active orders. Feed calls must be serialized per
destination by the polling owner; reset starts a new full snapshot.

## Host integration and limits

The host supplies a `FreeSpace` implementation for its database volume. Probe
errors refuse admission. Receipt and frame admission checks free-space reserve
and SQLite allocated page bytes, including free pages; limits must account for
WAL growth, transaction overhead and the deployment's supported outage. The
probe is deliberately injected so full-disk behavior is testable without filling
a real disk. SQLite disk-full errors also remain specific errors.

`Handle` opens and owns the connection on one dedicated thread, with a bounded
Tokio channel. Queue admission and response waits have a caller deadline.
Cancellation before execution skips a request; after execution starts, timeout
or cancellation does not prove rollback. Reconcile using the original identity.
Callers of `request` must submit only bounded store operations, never arbitrary
blocking work. Explicit shutdown closes admission, drains admitted requests and
joins the worker. Drop also joins, so callers must shut down before destroying
the async runtime. A blocking OS operation cannot be forcibly cancelled safely;
shutdown deadlines bound the async wait, not an uninterruptible OS flush.

Backup uses SQLite's online backup API in bounded page steps with a deadline,
then checks the target integrity. Existing targets are refused. A failed backup
may leave a partial target; it must never be used as a successful backup.
Restoring an older backup still requires host-level delivery/order reconciliation
before machine commands are enabled (plan §11).

## Evidence and exclusions

Tests cite ADR 0002/0003/0004/0008, AGENTS clinical rules and plan §11. They use
synthetic data and temporary directories, including restart-before-transfer-end,
full disk, immutable receipt bytes, atomic enqueue rollback, per-key head blocking,
reset/stale cache behavior and restore. A property test reopens the database
through randomized receipt sequences and checks all previously stored bytes.

This crate does not implement delivery HTTP/backoff calculation, clinical
interpretation, native volume probing, credentials, retention, machine order
transmission or pilot hardware flush/latency certification. Those are host or
other-crate responsibilities. The worker's Tokio timer paths require the host's
Tokio runtime; the crate enables only `sync` and `time` features.

Order events have disjoint CHECK-constrained provenance and per-order ordering.
Claims expose durable earlier attempt counts. Health queries expose unresolved
count, bytes, oldest age, pause and feed status. Hosts also check freshness.

## Observation history (ADR 0003)

Version 3 adds immutable observation revisions and frame provenance. The forward
migration retains existing rows/triggers and indexes resolved interpretation
history; contradictory legacy content at the same revision refuses migration
rather than picking a winner. No old interpretation is rewritten.

`interpret_reconciled` accepts mapped results, explicit correction evidence and
positioned operator findings. One transaction assigns revisions, stores the
interpretation, parks every finding/conflict and enqueues only eligible builder
output. The builder sees assigned revisions before constructing delivery bytes.
A retransmission matches any retained revision and cannot enqueue again.
Changed content needs declared correction evidence or a withdrawn status;
otherwise identity becomes unresolved and the item is parked. Machine identity
namespaces observation history so independent analyzers cannot collide.

Clinical content is canonical JSON of sample identity, processing mode and parts,
with each part's host receive time removed. Receipt IDs and assigned revisions
are not part of the digest. Source times and source status remain clinical
evidence. The SHA-256 input is independent of receipt/retry receive time.

The compatibility `interpret` method also reconciles history, but has no declared
correction-code evidence and cannot fix prebuilt bytes after revision assignment;
use the builder API for corrections. Both reject unsafe deliveries. Mapping
findings are stored in the same transaction, not as a later best-effort write.

`journal_frame` commits immutable machine/driver/config provenance before its
frame journal write. A failed frame write may leave provenance without a frame;
it never permits an ACK. Incomplete transfers are parked when closed. Read APIs
provide bounded receipt and audit pages for the protected operator interface.


## Machine order items (ADR 0003 rules 4/8; ADR 0004/0008)

Schema v5 is a forward-only migration; v4 and its histories are unchanged.
Each new reservation creates retained command items and append-only item history
in the same transaction, without an app event. ASTM/vendor records and query
displays use one item covering all app tests. HL7 send-ahead uses one item per
test through `reserve_send_items`. EI.1 is `<machine-order-id>-<item-index>`;
SQLite's retained AUTOINCREMENT parent ID and the one-based index own uniqueness.
The host supplies the required configured EI.2 namespace; it is retained with
the item so configuration changes cannot alter a later cancellation identity. Encoded identity
limits refuse the transaction; no truncation or partial reservation is possible.

V4 did not retain original test coverage. Its migrated orders get one item with
`coverage_known=false`, empty coverage, and `migration-v5-coverage-unavailable`
in history. A newer cache cannot reconstruct that evidence. Existing reservation
identities still block resending, including after a cache version change.

`finish_order_items` applies an entire exchange atomically. All accepted yields
Accepted; all rejected yields Rejected; all transfer-confirmed yields Sent;
all cancellations confirmed yields CancelConfirmed. Every mixed, uncertain or
missing outcome yields Uncertain and an operator summary of per-item reasons.
A refused cancellation restores the previous parent state, retaining confirmed
item cancellations and a CancelRefused reason and event. Any missing answer
still makes the outcome Uncertain. No outcome permits another reservation.
Illegal/duplicate/unknown item outcomes or an event-builder failure roll back
items, parent, history and outbox together. Recovery marks Sending items uncertain.
Reconciliation remains per order with an audit and updates its items together.

Whole-order transitions cannot bypass multi-item aggregation, except for a
cancellation request, recovery to Uncertain or audited operator reconciliation.
The protected operator listing includes each item's coverage, placer, state and
reason. These details do not change contract v1 or enter ordinary logs.

## Restore and reconciliation (plan ?11, ADR 0003 rules 2, 4, 8)

1. Stop the service. Keep the backup and data directory protected as clinical data.
2. Run `analyte --config analyte.toml --actor operator-name restore --from backup.db --reason "Recovery after disk failure"`.
   Integrity and supported schema are checked before migration of a staged copy.
   Normal store owners prevent restore through an OS file lock.
3. The previous database is retained as `analyte.before-restore`; it is never
   overwritten or deleted. Existing recovery files refuse a further restore.
   Archive the previous store securely before a later restore. Interrupted
   `analyte.restore-pending` files block opening: preserve all files and investigate
   before proceeding; never remove evidence just to start the service.
4. Restart the service. Result receiving and app deliveries continue. Delivery
   retries retain the original ID and bytes; the receiver must accept duplicates.
   Machine order reservations are paused, including query answers. Untrusted
   lookups follow their configured error/silence policy, never ?no orders?.
5. Inspect `analyte --config analyte.toml orders list` and status. Compare each
   machine's work list and actual outcomes with the app, including commands sent
   after the backup. Correct the app's requested work and reconcile uncertain
   entries before releasing the pause. This is an operator responsibility;
   Analyte cannot infer what reached a machine from an older backup.
6. Run `analyte --config analyte.toml --actor operator-name orders resume-after-restore --reason "Machines and app reconciled"`.
   This audited command clears the global pause; it sends no machine command.
   Existing reservations continue to forbid automatic replay.

The marker is the latest restore/resume action in the append-only audit trail;
no schema migration or driver-specific exception is needed. Evidence remains
**documented**, based on synthetic tests, without real-machine qualification.


`reserve_send_items_with` runs a pure message preflight with the actual assigned
IDs before committing Sending. A failed preflight rolls back every item and
history row. The shared `start_order` transaction checks `restore_paused` for
all reservation variants, including per-test sends and owned DSR responses.
App cancellation admission rechecks the feed's trust/version and restore pause
in its write transaction. The admitted app version and connection owner are
retained in append-only history; a refused cancellation cannot cause another
automatic cancel for that same version. Original reservations always block
resending new work, independently of subsequent app versions.

Aggregate source summaries list item states and bounded diagnostic names; full
machine reasons stay unchanged on every item. A long collection of reasons
cannot overflow the aggregate text field and roll back otherwise valid evidence.

## Trusted analyzer-pending lookup (LAW step 3)

`pending_orders(destination, machine, after, now, freshness)` is the one new read
API for analyzer-specific all-pending queries. It shares the SampleId lookup's
restore, completeness, trust and clock-age rule, and the existing reservation
path uses that same age predicate. It scans bounded ordered pages, filtering
expired/tombstoned, non-requested, wrong-target and previously reserved work.
One extra bounded sentinel row detects continuation. A filtered-empty page may
still have an `after` cursor; only an exhausted trusted scan establishes empty.
Read all pages in one store-owner request for a coherent answer and cap the
aggregate work/deadline in the caller. A reservation remains mandatory before I/O.

Tests `law_tf2b_3_27_4_pending_pages_filter_targets_without_false_empty_and_never_replay`
and `law_tf2b_3_27_4_sample_and_pending_share_freshness_generation_and_restore_trust`
cover paging, targeting, prior reservations, expiry/tombstones, future/stale clocks,
reset generations and restore pause. No schema or contract change is involved.
