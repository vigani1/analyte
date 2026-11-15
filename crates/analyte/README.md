# Analyte receive service

Evidence: **documented**. Tests use synthetic data, temporary directories and
localhost peers. No real analyzer qualification is claimed.

Run `analyte check-config <file>` before `analyte run <file>`. The example
`examples/analyte.toml` loads the synthetic HL7 definition and site settings.
Program paths are relative to that file. Each machine has a unique installation
ID, a definition path, a site path and an optional overrides path. The loader
rejects unknown keys, duplicate IDs, oversized files and inconsistent budgets.
Secrets are not program settings; destination entries contain credential names.
The receive service currently requires strict ASTM checksum hex and transport
operation timeouts no greater than 60 seconds; unsupported choices fail config
checking explicitly.

The store lives at `<data_directory>/analyte.db`. Restrict that directory,
including WAL/SHM files and file handoff areas, to the service/operator account.
New Unix data directories use mode 0700; existing directories with group/other
permissions are refused. Windows installations must restrict inherited ACLs.
The CLI is a local operator tool protected by those operating-system permissions,
not a network admin interface. It intentionally displays protected evidence.

Operator commands use `--config <file>` (default `analyte.toml`) and
`--actor <operator-id>`:

- `analyte parked list [--after <id>]` reads one bounded page.
- `analyte parked show <id>` reads a specific item.
- `analyte parked resolve <id> --reason <reason>` records resolution once.
- `analyte backup <target>` creates and verifies a new SQLite backup.

List, show and resolution are audited before evidence is printed. Use the final
listed ID with `--after` for the next page. Resolution never deletes evidence or
resends an order. A backup target must not already exist. An unsuccessful backup
may leave a partial target, which must not be restored.

Each machine has one supervised owner. TCP servers accept one session at a time;
TCP clients reconnect without carrying writes between sessions. Serial ports run
on dedicated joined threads. File producers must rename complete files into the
configured incoming directory; the dedicated file owner claims, stores and
interprets each file before moving it to protected archive or quarantine.
Interrupted claims survive for recovery. No producer file is deleted.

ASTM capacity is checked through `Handle::request` before the capacity event.
Every frame and its transfer provenance are durable before its ACK. Complete
messages retain their frame bytes; syntax parsing uses the assembled record text.
Aborted transfers retain their journal, config and a parked reason. Partial frames
are parked when a session ends. HL7 retains exact MLLP blocks and commits the
interpretation plus mapping findings before choosing AA or AE. Storage refusal
produces a negative protocol response or closes the session, with a nonclinical
structured diagnostic. Failed parses and all mapping findings stay protected.

Store revision assignment follows ADR 0003, including delayed retransmissions,
corrections without originals and conflicting identities. New and corrected
eligible production results each enqueue one immutable delivery in the same
transaction as their interpretation. Retransmissions, conflicts, parked results,
training and debug do not enqueue. One supervised worker serves all destinations,
and one poller per destination maintains the order cache. Startup automatically
recovers interrupted app attempts with an audit per delivery under ADR 0008.
Machine orders require explicit `[orders]` capabilities; queries never receive
invented empty orders.

Ctrl-C, termination on Unix and Ctrl-Break on Windows stop admission. Owners
finish submitted commits (15 seconds), then the delivery worker stops claiming
and drains its current attempt (5 seconds, retaining the HTTP deadline). Pollers
stop, transports close and join (5 seconds), and the store drains and closes
(5 seconds). Interrupted claims retry after restart with the same ID and bytes.
Delivery/store failures stop the service visibly. Individual machine failures
remain isolated. HTTP requests have a 3-second deadline, local delivery storage
operations 2 seconds, idle claims back off 100 ms, retries range from 1 to 60 seconds,
and order polls run every 5 seconds with a 30-second freshness policy.
An uninterruptible operating-system flush cannot be cancelled within a worker;
owner destructors join rather than detach. The process watchdog enforces the
overall stop deadline even when such a join blocks; see Running as a service.

The capacity probe invokes Windows PowerShell `Get-Volume -FilePath` or GNU `df
--output=avail --block-size=1` on Linux, with a three-second deadline and bounded
output. Paths are passed as data, never interpolated into executable scripts.
Missing utilities, permission errors and invalid output refuse admission.
Installing a faster native probe later does not change the storage boundary.

The admin webpage, retention and real machine validation
remain outside this work. Serial hardware and Linux runtime
verification remain CI/lab work; localhost integration tests run on Windows here.

Credentials are loaded by the host at startup from `ANALYTE_SECRET_<NAME>`:
uppercase ASCII letters and digits are retained, all other characters become
underscores. `demo-api` therefore reads `ANALYTE_SECRET_DEMO_API`. Missing,
empty or invalid credentials fail startup naming only the credential. Config
checking does not read the environment. Restart after changing credentials.
For rotation, `[signing_rotation]` in the program file maps destination names
to up to seven additional signing credential names; the primary key is retained.
No credential values belong in TOML.

`analyte status <file>` prints each destination's pending count and bytes,
oldest pending age in seconds, pause state, last successful poll (Unix seconds)
and cache trust under the freshness policy. It also prints each machine's last
receipt timestamp, or unknown when none exists. It reads bounded metadata pages
with a five-second overall receipt-scan deadline, and prints no clinical payloads
or credentials. This command works while the service runs and needs no secrets.

Phase 1 tests in `tests/delivery.rs` run the reference receiver library on random
loopback ports. Synthetic TCP ASTM and MLLP HL7 peers cover signed durable
content, retransmission suppression, corrections, training/debug exclusion,
conflicting content, outage/retry, authentication pause/status, bounded active
shutdown and audited interrupted-attempt recovery. When an interrupted attempt
had already arrived, restart sends identical bytes and ID and receives duplicate;
the receiver retains one record. These are documented tests, not real-machine
or external-app qualification.


## Documented vendor receive stacks

Machine definitions in `crates/config/examples/poct.toml`, `vidas_literal.toml`
and `autolyser.toml` are synthetic examples, not installed settings. Every vendor
policy is explicit. POCT uses TCP; VIDAS Literal and Autolyser use TCP or serial.
Field-position mappings are rejected for these syntaxes; `[codes]` uses the same
exact test and unit translations as the record mapper.

Vendor input is durably journaled before the dialog sees it. Complete receipts
and interpretations keep driver/config versions. Unresolved identity is preserved
and parked, never replaced with a generated run ID. `parked list` exposes its
specific reason. No current vendor crate yields a resolved result identity, so
vendor receive tests assert no reference-receiver result delivery.

VIDAS work-list sending uses the shared order mapper, durable `reserve_send`
and lifecycle APIs; see [ORDERS.md](ORDERS.md). The download permission alone
does not schedule an order: `[orders]` must also explicitly enable send-ahead
and configure the patient namespace and outbound codes. Successful transfer
means Sent, never Accepted. Interrupted orders become Uncertain and are never
automatically replayed. Autolyser host-initiated insertions use the same durable path after a completed
host poll, with an explicit specimen-medium table, clone flag and cup position.

## Test-only boundary hooks

The non-default `test-hooks` Cargo feature exposes one one-shot ASTM hook:
`AstmCommitted`, after durable frame storage and before ACK generation. Tests
install it only on an explicitly injected loopback listener. It can pause,
resume normal processing, or stop that connection; it cannot modify wire bytes
or bypass storage. Pauses stop on cancellation, dropped control or a five-second
deadline. Without the feature, both the hook and its call site compile out.
`test-hooks` is test-only and must never be enabled for release builds.
Exercise its restart test with
`cargo test -p analyte --all-features --test delivery recording`.

The same feature also exposes `Program::stall_store_on_shutdown_for_test`,
restricted to the loopback test service with explicitly injected listeners.
It stalls one store operation at shutdown and sets a 500 ms process deadline;
it deliberately requires a child process because the real watchdog exits 3.
It changes no protocol or storage bytes and cannot be enabled by TOML. Run the
ADR 0003 recovery case with
`cargo test -p analyte --all-features --test delivery plan11_adr0003_agents_forced_exit`.
The test retains committed raw receipts, confirms identical result retry bytes
and ID with one result at the receiver, and recovers Sending to Uncertain.
`tests/service_unit.rs` checks the unit's structure, sandbox settings and stop
margin without requiring systemd or installing anything.


HL7 legacy QRY^Q02 queries can opt into the shared order cache and reservation
path with the explicit [DSP layout](../config/examples/hl7_orders.toml).
QCK/DSR replies follow durable raw storage. Trusted empty is NF; untrusted,
stale or late lookup is AE. DSR application receipt is Sent, never Accepted;
negative, missing or disconnected ACKs are Uncertain. HL7 ORM/OML send-ahead
and cancellation require the separate explicit `[orders.hl7_send]` capability. See [ORDERS.md](ORDERS.md).

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

## Running as a service

Use a release build without `test-hooks`. Evidence remains **documented**:
synthetic subprocess and SCM seam tests do not establish a real SCM installation,
systemd runtime or machine qualification. There is no MSI/installer GUI, signing
or automatic update mechanism.

For Linux, follow the [systemd installation guide](../../packaging/linux/README.md).
The unit starts `/usr/local/bin/analyte run --config /etc/analyte/analyte.toml
--shutdown-timeout-secs 30` as the dedicated `analyte` user without a login.
The config's `data_directory` **must be `/var/lib/analyte`**, the only writable
persistent data path allowed by the unit. The state directory is mode 0700;
file handoff areas must also live beneath it. Configuration and binaries remain
read-only to the service. `dialout` supplies serial device access; confirm the
group and actual device permissions for the distribution. Logs go to journald:
`sudo journalctl -u analyte.service`. Stop safely with
`sudo systemctl stop analyte.service`; SIGTERM and SIGINT both use graceful shutdown.

For Windows, follow the [PowerShell installation guide](../../packaging/windows/README.md).
The scripts configure automatic start under `NT SERVICE\Analyte`, restart actions
on failure, and Modify access only on the dedicated data directory, with SYSTEM
and administrators retaining FullControl. By default the program file is
`C:\ProgramData\Analyte\config\analyte.toml` and its `data_directory` must match
the installer's `C:\ProgramData\Analyte\data` parameter. Prepare read access to
configuration, device permissions and protected service credentials before
`Start-Service Analyte`. Service logs append to `<data_directory>/analyte.log`.
Protect and rotate the log. Stop safely with `Stop-Service Analyte` or Services.msc.
Uninstall removes service registration and retains data, logs and configuration.

`analyte service run --config <path>` is the Windows SCM entry point; invoking it
outside the SCM fails explicitly. Use `analyte run --config <path>` in a terminal.
Neither mode installs a service in Rust. Configure the lab PC not to sleep.
Provision secrets through the platform's protected service environment, never
TOML or command arguments. Logs contain component IDs and diagnostics, never
patient payloads; raw evidence stays in the protected data directory.

Both commands accept `--shutdown-timeout-secs` (30 through 300, default 30).
Stop requests first run the staged drain described above. A dedicated OS thread
enforces the overall deadline even if a blocking owner destructor cannot finish.
At expiry, Analyte logs one error with the IDs of remaining components and exits
with code **3**, bypassing destructors. This follows ADR 0003 crash recovery:
unacknowledged/uncommitted input can be retried, committed receipts survive,
deliveries retain their IDs, and interrupted Sending orders become Uncertain.
A successful graceful stop exits 0; other failures exit nonzero.

During a requested Windows stop, Analyte reports Stopped through the crate's
public API before exiting on deadline. The SCM therefore shows a normal stop;
the deadline reason is in the Analyte log and process exit code, not an SCM
service-specific exit code. Linux systemd records exit code 3 as a failure.
Keep the service stop timeout above the configured Analyte deadline.
The shipped Linux unit uses `TimeoutStopSec=35s` for the 30-second Analyte bound;
when changing the bound, keep a five-second systemd margin. A requested normal
stop, including a forced Windows stop reported as Stopped, is not automatically
restarted by recovery actions. Inspect the log after a forced stop, preserve the
store and follow the existing recovery/reconciliation procedures. Never replay
an uncertain machine order merely because a service was restarted.

## LAW results and query runtime (steps 2–3)

`law_manager` now runs a dedicated LAB-29 receiver on an analyzer-initiated TCP
server link. Its complete MLLP block and configuration provenance are durable
before profile validation, mapping or ACK. Existing LAW views/ACK builders and
the shared revision/outbox transaction own interpretation and delivery. AA
requires successful durable interpretation; failed content is parked with AE
or AR. A storage failure closes without AA. Malformed syntax without a trustworthy
response header is retained and the connection closes. Partial blocks are
bounded by the configured first-byte receive deadline and parked on expiry.

AWOS is the order identity when present. An unsolicited result can use an
explicit analyzer order/authority; missing evidence is unresolved, never invented.
The entire OG is retained; changed non-C content conflicts, a C correction gets
a new revision, and retransmissions create no delivery. A conflicting batch
cannot register revisions or deliver its otherwise-new subset; a later accepted
retry can still deliver those results. The shared transaction rolls back the
batch before its failed interpretation is parked. Units and values keep source spelling.

LAB-27 is enabled by the explicit query-work layout in the config crate's
`examples/law_orders.toml`. It supports exact SampleId/container and analyzer-specific
all-pending lookup. The latter pages through one new store read using the same
trust/freshness/restore rule as SampleId lookup. Unsupported forms receive QAK AR;
unavailable, stale, failed, paused or late answers receive AE, never no-work.

Positive availability owns a built LAB-28 followup before the RSP can be issued.
Each AWOS is a shared durable per-test item, reserved and preflighted before any
positive output. ORL bytes are stored before per-AWOS acceptance/rejection reaches
`finish_order_items`, matched by ID even when reply order differs. Missing replies,
disconnects and invalid correlation become Uncertain; reservations forbid replay.
Trusted empty uses the crate's restricted negative followup with no reservation;
its ORL is correlated and audited without changing an order. Query budgets start
at the first MLLP byte, including raw storage and reservations. Response timers
start at followup issuance. Cancellation cleanup retains every submitted key so
a committed reservation can be recovered even if its caller lost the response.

Unsolicited work, send-ahead, cancellation, other connection roles and independent
LAW link topology remain disabled/unqualified pending later steps. Results-only
config still rejects queries without a work layout. No full actor conformance is
claimed at this checkpoint.
`tests/law/results.rs` contains nine synthetic program/reference-receiver cases,
including a durable-commit pause, raw/interpretation storage failures, low disk,
unsolicited results, complete OG run identities and partial-block expiry.
`tests/receive.rs::law_tf2b_3_29_manager_selects_dedicated_receiver_without_startup_io`
checks startup selection/role restriction. `tests/law/queries.rs` adds thirteen
program/reference-receiver cases, including paging past a full page for other
analyzers, blocked cache access, an unreachable receiver, restore pause, late
queries, item reservation storage failure, ORL outcomes and no replay. Evidence
remains documented; originating-work result correlation, send-ahead/cancellation,
restart qualification and Gazelle remain.
