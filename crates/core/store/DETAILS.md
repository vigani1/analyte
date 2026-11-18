# analyte-store

One protected SQLite database holds receipt evidence, the bounded frame journal,
versioned readings, parked items, immutable deliveries, attempts, order pages and
audit entries. The host must protect the database directory (including WAL, SHM
and backup files); nothing here logs clinical payloads.

## Durability and recovery

Opening runs `integrity_check` and `foreign_key_check`. An empty file gets the
starting schema (version 15) in one transaction; a store with an older or newer
version is refused. Later releases add forward-only steps. WAL and `synchronous=FULL` deliberately require
SQLite to sync each committed WAL transaction before success is returned.
Foreign keys are enabled on every connection; lock waits have a configured
nonzero timeout capped at 30 seconds. These guarantees still require a local
filesystem and storage hardware that honor flush requests. See SQLite's
[PRAGMA documentation](https://sqlite.org/pragma.html).

`journal_frame` commits independently before returning the sequence a dialog may ACK.
Incomplete and aborted transfers retain every frame; recovery is paged by
transfer ID and sequence. `finish_transfer` can link only to an existing receipt.
Receipts include exact effective config bytes and checked SHA-256 digests.
Ordinary APIs cannot update or delete receipt/frame/interpretation evidence;
SQL triggers reject accidental changes. ADR 0013 retention transactions can purge
eligible finished bundles while retaining keyed tombstones and an audit.
Reprocessing creates another interpretation version.

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

Other processes, such as operator commands, may open the same store while the
service runs, and some of them write (every audited command does). Every store
transaction therefore takes SQLite's write lock when it begins (`IMMEDIATE`). A
deferred transaction that has read and then writes gets `SQLITE_BUSY` at once,
without calling the busy handler, if another connection is writing at that
moment ([sqlite3_busy_handler](https://www.sqlite.org/c3ref/busy_handler.html),
[transaction behavior](https://www.sqlite.org/lang_transaction.html)). With
`IMMEDIATE`, contention waits up to the configured busy timeout instead. The
owner lock file is released only after the connection, including SQLite's
checkpoint on close, has closed.

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
interpretation, native volume probing, credential loading, machine order
transmission or pilot hardware flush/latency certification. Those are host or
other-crate responsibilities. The worker's Tokio timer paths require the host's
Tokio runtime; the crate enables only `sync` and `time` features.

Order events have disjoint CHECK-constrained provenance and per-order ordering.
Claims expose durable earlier attempt counts. Health queries expose unresolved
count, bytes, oldest age, pause and feed status. Hosts supply the destination freshness to the shared typed trust decision.

## Observation history (ADR 0003)

Observation revisions and frame provenance are immutable. No old interpretation
is rewritten.

`reconcile` accepts mapped results, explicit correction evidence and
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

`reconcile` is the sole interpretation entry point for ordinary
reception; builders receive assigned revisions before constructing delivery bytes.
Mapping findings are stored in the same transaction.

`journal_frame` commits immutable machine/driver/config provenance before its
frame journal write. A failed frame write may leave provenance without a frame;
it never permits an ACK. Incomplete transfers are parked when closed. Read APIs
provide bounded receipt and audit pages for the protected operator interface.


## Machine order items (ADR 0003 rules 4/8; ADR 0004/0008)

Each new reservation creates retained command items and append-only item history
in the same transaction, without an app event. `OrderItems::Together` covers the
current cached test list in one command. `OrderItems::Groups` retains the explicit
groups and order supplied by the caller, after checking that they partition the
cached tests exactly. Empty, missing, duplicate or invented coverage refuses the
transaction. Local identity `<machine-order-id>-<item-index>` is unique through
SQLite's retained AUTOINCREMENT parent ID and the one-based index. The optional
logical namespace is retained so config changes cannot alter cancellation identity.

The caller supplies `max_identity_bytes` for the generated local identity after
accounting for its encoding overhead. The store does not choose a delimiter,
protocol grouping or composite identity limit. A too-small budget rolls back the
whole reservation; no truncation or partial reservation is possible.

Existing reservation identities block resending, including after a cache version
change.

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

## Restore and reconciliation

Follow the [operator restore procedure](../../analyte/DETAILS.md#restore-and-reconciliation-plan-11-adr-0003-rules-2-4-8).

## Trusted analyzer-pending lookup

`pending_orders(destination, machine, after, now, freshness)` is the bounded read
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

Orders and items use the model's single lifecycle transition function, including
audited operator reconciliation. Cancellation refusal restores the retained
per-item `cancel_from` states rather than searching historical aggregate JSON.
Correlated HL7/LAW replies may accept or reject a reserved Sending exchange.

`Store::order_trust` distinguishes Trusted, Stale, Incomplete, RestorePaused and
Unknown. Lookups, candidate reads, transaction admission and cache alerts use
that rule; only Trusted can establish no orders. Cached state is typed and
shares the model eligibility function with the host. Cancellation scans filter
typed states in bounded pages rather than comparing state text in SQL.

## Implementation ownership

`upkeep/open` owns integrity checks and runs `upkeep/migration`; `upkeep/capacity`
owns admission and size limits. `intake/records` retains raw inputs,
`intake/transfers` journals frames and
provenance, `delivery/outbox` owns claims and attempts, `operator/parked` owns
operator parking and audit writes, `delivery/feed` owns cache writes and trust,
and `upkeep/backup` owns online copies.
`results/revisions` owns interpretation transactions. `upkeep/retention` owns
eligibility, holds, keyed tombstones, bounded purges and space reclamation; the
host supplies the credential and schedules batches.

Machine-order eligibility, reservation, listing, cancellation admission and item
exchange completion live under `orders/machine_orders/`. The parent module retains
shared lifecycle types and transaction helpers so state changes and history
writes still have one implementation. These boundaries do not change the
schema, public API, transaction boundaries or evidence formats. Existing store
test filenames remain stable for mutation-coverage work.

`reserve_send(candidate, options)` is the single reservation entry point.
`ReserveOptions` selects connection ownership, item grouping and an optional pure
preflight callback. Defaults reserve one unowned item covering all tests. Preflight
sees assigned identities inside the transaction; an error rolls back reservation,
items and history. Callers retain any prepared payload locally until commit succeeds.

The public write path for frames is `journal_frame`, which retains provenance.
Unfinished-transfer inspection is a crate-test helper; host receive tests inspect
the protected journal directly after running the real service path. Operator
resends use `resend_delivery`, including audited recovery from a setup pause.

## Typed persistence (ADR 0003 rules 2/3/8)

Order/item SQL states use plain lifecycle names rather than JSON string literals.
`sql_types` owns the SQLite codecs for lifecycle state, delivery state, delivery
kind and ownership kind; callers bind typed values and reject unknown SQL values.
The model remains independent of SQLite. Delivery encodings and immutable bytes
remain unchanged.

Each lifecycle history row now has checked `owner_kind`, `owner_id` and
`cancel_version` columns. Ownership checks read those columns, and cancellation
replay checks use an indexed, exact version comparison. Versions are canonical
unsigned decimal strings so the full `u64` range survives SQLite's signed integer
limit. Human-readable source codes are evidence only; they never decide ownership.
Cancellations retain the supplied full connection ID explicitly.

All parking writes use one object containing `kind`, `reason`, `targets` and
`context`. Context retains interpretation version and result position.
Parking evidence is immutable; resolution appends an audit.

## Caller-owned protocol constraints

`FrameSequence::new(number, maximum)` validates a byte-sized sequence against the
caller's inclusive bound before journal writes. The store has no ASTM rollover
rule; the frame column accepts the whole byte domain.

`TransferOutcome::Complete` links retained receipt evidence;
`TransferOutcome::Incomplete` requires the caller's typed reason. The host owns
ASTM-specific explanations and supplies the framing crate's sequence maximum.
HL7 and LAW callers build one group per test and calculate the remaining identity
budget using named constants from `analyte-hl7`. Existing wire limits and the
logical namespace budgets are preserved; the store does not know their encodings.

## Typed cursor custody

`begin_poll_intent(machine, reason)` atomically retains a Pending intent, an
immutable history entry and linked operator-visible parked evidence before the
host transmits a cursor-changing poll. An indexed machine/state query,
`unconfirmed_poll_intent`, replaces parked-page scans. Admission rechecks this
predicate in an immediate transaction: Pending and Uncertain both block a new poll.
A lost store reply may leave Pending committed, so cancellation never proves the
poll was not admitted or transmitted.

`record_poll_outcome` accepts Confirmed or Uncertain with a targeted audit and
reason, retaining previous reasons in append-only history. Confirmation reconciles
the linked parked evidence in the same transaction when every intent linked to it
is confirmed; confirming one machine never clears another machine's uncertainty. Confirmed is terminal; late
cleanup or a repeated confirmation cannot downgrade it or replay that identity.
The existing audited `parked resolve` path also reconciles linked intents
atomically. Parked rows remain operator evidence; they no longer own poll state.
Capacity, history or audit failures roll the entire operation back.

The Autolyser host records observed failures as Uncertain and treats an interrupted
Pending intent as uncertain when reconnecting. It confirms only after the existing
dialog establishes retained response custody. The operator can inspect and reconcile
the linked parked item using the existing commands; no protocol parser or order
lifecycle rule is added to the store.

Tests cover admission before wire output, unknown-outcome reconnect blocking,
Pending/Uncertain/Confirmed transitions, restart, independent machines, terminal
confirmation, capacity refusal, transactional audit/history failures and operator
resolution.

## Retention schema (ADR 0013)

The store keeps receipt tombstones, delivery UUID associations, keyed result
identity/component/content digests, a key fingerprint and explicit holds. The
credential is never serialized into the database. Startup configures the key
before machine admission and refuses a missing or changed credential when a
fingerprint is retained. There is no key rotation.

The single receipt eligibility query requires retained interpretations, resolved
identities, a delivery for every interpretation's result, every delivery done with
an acknowledgment, and a complete retention period after the last acknowledgment.
It excludes parking evidence, unfinished transfers, open or uncertain machine
orders/items and poll intents, and explicit holds. In this version, ambiguous
machine links retain the whole machine's receipts; order holds cover linked receipts, with uncertain links retained. Resolved parking evidence is retained too.

Use `analyte --actor NAME holds receipt UUID --reason TEXT` or `holds order
DESTINATION ORDER --reason TEXT` to place a hold; add `--release` to release it.
Each transition and its audit commit together; repeated transitions are refused.

## Purge transactions and filesystem space

`purge_batch` examines at most `min(page_rows, 16)` receipts in one immediate
transaction. The same eligibility query used by the public store rule runs inside
that transaction. Its delete guards are removed only transactionally and restored
before commit. A rollback or abrupt process death restores both evidence and guards.
The batch preserves original raw digests, provenance, receive/ack/purge times,
receipt/delivery UUIDs, keyed result components/content and revisions. Audit fields
record policy, receipt/result/delivery counts, scan cursor and the purged UUID range.
They retain the common Audit header, so ordinary audit readers keep working.

SQLite's progress handler interrupts overdue SQL once, permitting rollback to
finish. Lock waits are capped by the same deadline. Backups share a host maintenance
lock; purge/reclamation require it exclusively. Offline restore uses the separate
exclusive store lock. The runtime owns and joins its retention task at shutdown.

Incremental vacuum avoids running a whole-file rebuild during live receiving.
Every new file sets `auto_vacuum=INCREMENTAL` before schema creation; finite
retention refuses any file without it. Online vacuum requests at most 32 pages and
never waits for a checkpoint reader. `secure_delete=ON` clears deleted page
content. See SQLite's
[auto_vacuum and incremental_vacuum](https://www.sqlite.org/pragma.html#pragma_auto_vacuum).

Tests named `adr0013_items3_4_5_6_real_purge_audit_refusals_and_late_duplicate`,
`adr0013_item4_crash_mid_batch_rolls_back_data_tombstones_and_guards` and
`adr0013_item2_random_unfinished_states_never_purge` guard the key safety outcomes.
The crash test terminates a child process before commit without running destructors.
The space test checks a real file shrink after bounded vacuum.

## Refined order retention (ADR 0013 items 2, 6 and 7)

The store keeps immutable keyed order tombstones and delivery associations. Order bundles
use separate bounded scan cursors, the existing transaction deadline/backup pause
and the common audit header. Only Done, Rejected and CancelConfirmed parents with
finished items and old enough final/acknowledgment times are eligible. Holds,
unfinished app events, newer or unknown cached versions and potentially linked
retained receipts prevent deletion. Shared cache/events remain until the last
machine reservation is purged. Pending-order discovery and atomic reservation
check keyed tombstones, so feed replay cannot resend purged work.

Receipt links use exact namespaced order or retained placer identities, or sample
IDs in interpretations and the order cache. A matching sample conservatively
covers the receipt; distinct known samples establish separation only when source
order evidence does not contradict them. Missing evidence, excessive histories
and cache-version inconsistencies remain unknown and block deletion. Explicit
poll receipt targets can establish separation; machine-only legacy targets cannot.

## Error recovery

`Busy` preserves SQLite BUSY/LOCKED codes and diagnostics: the refused operation
has not committed. Retry with the original identity after contention clears.
`Timeout` means a submitted worker operation may have committed; reconcile that
identity before deciding what to do. `ConstraintViolation` preserves SQLite's
extended constraint code and source, separately from application identity/state
`Conflict`. These distinctions follow [SQLite result codes](https://www.sqlite.org/rescode.html)
and [transaction completion](https://www.sqlite.org/lang_transaction.html).

`AuditMismatch` requires corrected authorization, and `LimitExceeded { what }`
names a configured bound. Other input refusals name their boundary: limits,
receipt, interpretation, delivery, order, operator, maintenance or host command.
`Text` retains the model's precise bounded-text cause. Encoding refusals retain
the original JSON error, including its I/O category for bounded serialization.

`src/upkeep/schema.sql` is the starting schema: every table, seed row, index and
trigger of a new store. `src/upkeep/migration.rs` applies it as the first step
and is the one place a later release appends numbered steps; each step checks
integrity before it commits. The starting schema is never edited after release.
