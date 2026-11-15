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

Admin actions, retention and real machine validation
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

## Alerts

Alerts are opt-in program settings, evaluated by the running service. A fresh
configuration without `[[alerts]]` raises none. Active alerts appear in
`analyte status <file>` and on the authenticated, read-only admin overview,
with the named responder and seconds since the latest raise. These views read
durable state; they do not evaluate or acknowledge alerts. Unknown measurements
and clock rollback remain unknown rather than being shown as zero.

Each responder names the local role or person who will investigate and arrange
cover when absent. Agree that responsibility with the lab before enabling a
policy; naming someone does not contact them. Keep patient information out of
responder text and machine IDs. There is no external notification channel yet:
email, chat and notification HTTP targets are not implemented. Channels will be
chosen at the pilot.

Start with the following policies, then agree ages and disk margin against the
lab's expected traffic, outage budget and order-cache freshness. Put the global
interval before any TOML table; append the alert tables at the end of the
program file.

```toml
alert_interval_seconds = 30

[[alerts]]
kind = "oldest_undelivered"
threshold = 300
responder = "Lab duty officer"

[[alerts]]
kind = "disk_reserve"
threshold = 1073741824
responder = "Lab IT on-call"

[[alerts]]
kind = "order_cache_stale"
threshold = 60
responder = "Lab app support"

[[alerts]]
kind = "uncertain_orders"
threshold = 0
responder = "Analyzer operator"

[[alerts]]
kind = "parked_items"
threshold = 0
responder = "Lab duty officer"

[[alerts]]
kind = "restore_paused"
responder = "Restore lead"
```

Age thresholds are seconds and raise at or above the threshold. Count thresholds
raise strictly above the threshold: zero catches any uncertain order or unresolved
parked item. The disk threshold is a margin in bytes above `store.reserve_bytes`;
an alert raises when free space is below reserve plus margin. `restore_paused`
has no threshold and rejects one in config. Other kinds require a threshold.
Responders must be nonempty, single-line and at most 256 UTF-8 bytes. At most
128 policies are accepted; the interval is 1 through 3600 seconds (default 30).
Run `analyte check-config <file>` after editing.

Only enable machine silence for a machine the lab expects to be busy. Replace
the example ID with its installed `[[machines]].id`:

```toml
[[alerts]]
kind = "machine_silent"
machine = "busy-machine-id"
threshold = 600
responder = "Analyzer operator"
```

Silence uses the most recent durable received message or ASTM frame, rather than
connection state or control bytes. Missing machine activity or a missing,
incomplete or untrusted configured order cache raises with an unknown value.
The cache policy checks every configured destination.

Raise/clear history is retained, and restart preserves active state without
another raise. Measurements are counts, ages or free bytes; transition logs
contain the signal, optional machine ID, active state and numeric value, never
responder text or clinical payloads. Removing a policy stops its evaluation but
does not hide its retained active state; re-enable it to evaluate and clear the
condition. Resolution of a parked item or reconciliation of an uncertain order
remains an explicit, separately audited operator action.

## Admin page

The optional `[admin]` configuration accepts `port` and defaults `address` to
`127.0.0.1`. Other addresses and port zero are refused. Omitting the section
disables administration. For example, add `[admin]` with `port = 8080`, provision
the token below, then start the program and open `http://127.0.0.1:8080/`.
An enabled endpoint refuses startup if its token file is missing or invalid.

Provision or rotate the host credential with
`analyte --config analyte.toml admin token --rotate`. The command writes a
base64-encoded 256-bit token to `<data_directory>/admin-token` and prints it
once to stdout. Treat stdout as a secret; do not send it to ordinary logs.
Entropy comes only from the operating system; failure refuses rotation with
a specific error. No weaker fallback exists.

Unix staging files are created with mode 0600 within the protected directory.
On Windows, the token file's protection comes from the data directory ACL set by
[`packaging/windows/install.ps1`](../../packaging/windows/install.ps1). Files
inherit that ACL; it grants access to the service account, SYSTEM and administrators.
For manual installations, apply the same protected directory ACL. Rotation
flushes the new file before replacing the old one. A failed or interrupted
rotation may leave `admin-token.pending`; preserve it securely and investigate
before retrying. Never move a partial file into place as a credential.

Supply `Authorization: Bearer <token>` to read the pages, or use the login
form. Missing or wrong credentials receive HTTP 401 without credential values
in logs. Open `/login` to obtain a five-minute challenge cookie and CSRF form
token, then POST the form. Every POST, including login and raw disclosure,
requires that session's CSRF token, compared in constant time. Login replaces
the challenge with a separate random session cookie: HttpOnly, SameSite=Strict,
host-only and without a persistent cookie expiry. Authenticated sessions expire
on the server after one hour. At most 128 sessions are retained; new challenges
can evict older unauthenticated challenges, never an authenticated session.
All sessions share the administrator credential's authority; there is no
password database or per-person login. Each login gets a separate audit actor
(`admin-session-<UUID>`), which identifies that session rather than a person.
Restart after token rotation: the server loads the bearer token at startup,
and restart also invalidates all session cookies.

The listener serves HTTP on IPv4 loopback only; its cookies therefore omit
Secure. Any future HTTPS transport must set Secure. Forwarded headers do not
enable HTTPS or change the trusted host/origin. The form and overview use no
inline scripts or external assets. Every application response carries CSP,
frame denial, no-referrer and no-store headers. Values pass through one HTML
escaping helper. Requests are limited to two seconds, connections to two seconds
and 32 concurrent connections, and form bodies to 1 KiB. Service shutdown stops
admission and joins the admin owner within five seconds. Cancelled reads may
finish on the storage worker; they do not change machine orders. Cancellation
after submitting a raw-view audit may leave a durable audit without a completed
HTTP response. Retrying creates another audit, never a machine command.

The read-only overview shows machine owner state and last received activity,
oldest undelivered age, destination order-cache age and trust, Uncertain order
count, restore pause, and free disk bytes against the configured reserve.
Connection state is explicitly unknown; running an owner does not establish a
machine connection. Reads use bounded store pages and deadlines. If a complete
snapshot cannot be read in time, the page reports failure rather than displaying
an incomplete count as a total. Raw payloads and sample IDs are not shown on
the overview.

Use the navigation links for machines, parked items, and machine orders. Lists
show at most 50 entries and may show fewer under the configured store page bound.
Follow Next page to continue. Cursors are nonnegative and bounded; unknown
query fields are refused. Parked lists show category, reason, linked identities
and receipt times without loading payloads. The store does not record a parking
timestamp, so the page labels receipt time and explicitly reports missing times.
Parked details paginate targets in groups of 50 and keep raw bytes hidden.

Log in with a browser session before using **Show raw (audited)**. It is a POST
with a session-specific CSRF token; GET never reveals raw bytes. Choose the
linked receipt or transfer-frame source and optionally enter a reason. Each
4 KiB page renders exact hex alongside escaped ASCII text (dots for nonprinting
or non-ASCII bytes). A durable audit records the session actor, parked item,
source identity, cursor, time and reason before any bytes are returned. Audit
failure prevents disclosure. Raw pages retain POST forms for further pages;
no credentials or raw bytes are placed in URLs or ordinary logs.

Machine orders show the recorded lifecycle state, including Uncertain, and the
global restore pause. Durable machine items show their individual states, placer
identities and exact retained test coverage. Legacy coverage is explicitly unknown.
The separate retained-cache panel shows sample IDs, items and its version. A
different cache version is explicitly labelled as current cache items rather than
the historical machine items; missing or invalid documents never cause guessed items.

Actions such as resolving, reconciling and retrying remain CLI-only.

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
