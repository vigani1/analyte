# analyte-sink

Host-owned outbound HTTPS delivery and order polling (ADR 0004/0008, plan 7/11).
Evidence level: documented. Synthetic reference-peer tests do not prove
interoperability with an external app or a real analyzer.

Construct a Destination with explicit limits and a credential name. HTTPS uses
verified certificates; redirects and automatic HTTP retries are disabled. An
explicit `for_loopback_test` constructor permits literal loopback HTTP only.
SecretsSource returns preloaded host credentials promptly without I/O. The host
loads/rotates secrets with bounded I/O and supplies up to eight signing secrets.
No secret belongs in machine config or logs.

`build_delivery` checks production eligibility before creating one UUIDv7 and
serializing the contract conversion. Commit it atomically with its interpretation.
`build_order_event` freezes a validated event (including its host-created ID) for
`enqueue_order_event`. The store keeps result and per-order event ordering apart.

One Worker owns delivery for the store's configured destinations. `deliver_once`
claims, signs the stored bytes with fresh metadata, sends, classifies, then records
the attempt and scheduling decision atomically. Retry history survives restart.
Backoff doubles with positive jitter and is capped, including Retry-After.
Paused destinations require an audited operator release through the store.

Run one Poller per destination. It commits each page with the cursor, resumes
stored cursors after restart, and invalidates on 410 before requesting a new
snapshot on the next iteration. Page count, page size and response bytes are
bounded. Successful pages update poll time; trust requires a complete snapshot
and configured freshness. `Destination::orders` applies that policy to lookups;
health combines store status with the same freshness policy.

The loops borrow their resources and spawn no tasks. A watch boolean (true),
or closing its sender, stops active and idle waits. The host owns and joins any
tasks it creates. The program uses `run_draining(stop, budget)`: stop claiming,
let the active attempt finish under its existing request deadline and remaining
shutdown budget, then cancel only on budget expiry. The lower-level `run` retains
immediate cancellation for hosts that need it. A submitted store write can commit.
Before worker startup, call the store's paged `recover_in_flight(now, audit)` until
zero: interrupted app attempts automatically retry with the same ID and exact
bytes, with an uncertain attempt and an audit per delivery (ADR 0008). An app may
already have accepted it and must answer duplicate. Parked, conflict and paused
items still need operator release. This recovery never sends orders to machines.
Network timeouts recorded as retries also retain the same ID and bytes.

This crate does not dispatch orders to analyzers, interpret clinical results,
resolve parked evidence, host an admin UI, or load secrets from files.

Tests build and run the reference receiver binary on 127.0.0.1 using Cargo.
Owned child processes and a bounded loopback fault relay exercise uncertain
acceptance, deduplication, conflict, setup pause/resume, Retry-After, correction
ordering, events with key rotation, cache changes/reset, and active cancellation.
Only synthetic fixtures are used. Store migration tests cover every v1 table,
retained immutability triggers, disjoint provenance constraints and restart counts.
