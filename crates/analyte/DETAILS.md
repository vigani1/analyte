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

## Reprocess and resend

Use **reprocess** after fixing parsing or a machine definition setting. It reads the
receipt's original bytes using the current driver and configuration files and
appends a new interpretation with that provenance and config digest. The original
receipt and earlier interpretations stay unchanged. Existing revision rules still
apply: identical results are retransmissions with no new delivery; changed content
without correction evidence is parked as a conflict. Unresolved identities remain
parked. A successful parked-item reprocess records its resolution and new version;
a failed interpretation records a new parked reason. Each action has one durable
audit in the same transaction as its effects.

```sh
analyte --config analyte.toml --actor operator parked reprocess <parked-id> --reason 'Fixed the order ID selector'
analyte --config analyte.toml --actor operator receipts reprocess <receipt-uuid> --reason 'Reviewed the current mapping'
```

Use **resend** when the app needs an existing delivery again, after repairing the
cause of a rejection or setup pause, or after losing previously accepted data.
It queues the same delivery ID and exact stored bytes; the app answers a repeat
as `duplicate`. It does not interpret the receipt again or build new bytes.
Only Done, Parked and setup-paused deliveries are eligible; pending, in-flight,
conflicting and unknown deliveries are refused. Naming a delivery ID explicitly
on the CLI acknowledges selecting it even if a newer result revision exists.
Check the ID carefully before requesting an older revision.

```sh
analyte --config analyte.toml --actor operator deliveries list --reason 'Find the delivery for app recovery'
analyte --config analyte.toml --actor operator deliveries resend <delivery-uuid> --reason 'App recovery requested this delivery'
```

All four commands require an explicit `--actor` and `--reason`. The delivery list
is bounded: use its final sequence number with `--after` to read the next page.
The list contains IDs, destinations and states, without clinical payloads. A resend
is queued for the service's ordinary delivery worker; the CLI does not contact the
app itself. Neither action contacts a machine, answers a machine query or resends
an order. A timeout can leave an action committed: inspect its audit and retained
versions before deciding whether to repeat it.

### Needs review notices

A lab app whose setting "Send Needs review notices" is yes (`review_notices =
true` in `lab-apps.toml`, or `analyte lab-apps set <name> --review-notices
yes`) is told about Needs review items with `POST /review-notices`
([ADR 0038](../../docs/adr/0038-needs-review-notices.md)). Every five seconds
the engine reads the items parked since the last read and the items it told
the lab app about that staff have since settled. It queues one opening and one
closing notice per item in the outbox, each with its own delivery ID kept on
every retry, and the lab app's delivery worker sends them with its results. An
item goes to the lab app it names (a delivery, an order) or else to its
machine's lab app. A notice carries the machine, its model, the sample ID, the
test codes and a reason category, never a raw message or a value. The lab app
never decides: staff still settle the item here.

If the lab app answers 404 or 501, its notices are declined: nothing more is
queued, nothing is parked or paused, results go on, and the lab app's Setup
page says so. The engine asks again after its next start. Settings take effect
at engine start; a lab app is told about items parked from then on.

Each machine has one supervised owner. TCP servers accept one session at a time;
TCP clients reconnect without carrying writes between sessions. Serial ports run
on dedicated joined threads. Every session loop (TCP server, TCP client, serial
and inbound independent LAW) follows one rule when a session fails: it logs the
specific error without payload, waits a short cancellable backoff and serves the
next session. A failed serial session closes and reopens the port so the next
session starts from a fresh connection. Only an unusable listener, invalid link
settings, or a serial thread that panicked or could not be joined stop the owner.
File links are the exception: a failed file keeps its claim and ends the file
owner, because only restart recovery may offer that claim again. File producers must rename complete files into the
configured incoming directory; the dedicated file owner claims, stores and
interprets each file before moving it to protected archive or quarantine.
Interrupted claims survive for recovery. No producer file is deleted.

ASTM capacity is checked through `Handle::request` before the capacity event.
Every frame and its transfer provenance are durable before its ACK. Complete
messages retain their frame bytes; syntax parsing uses the assembled record text.
Aborted transfers retain their journal, config and a parked reason. Partial frames
are parked when a session ends. HL7 retains exact MLLP blocks and commits the
interpretation plus mapping findings before answering. A held interpretation
is still answered AA (ADR 0010, 2026-11-26); only an HL7 syntax error found
while reading, or a message type the connection does not read, is answered AE. Storage refusal
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
remain isolated. Each lab-app HTTP request (a delivery or one order page) has the
lab app's `request_timeout_seconds` deadline (default 10, 1 through 120);
an attempt still running when the 5-second drain ends is cancelled, reported as
a delivery shutdown timeout, and retried after restart with the same ID and bytes. Local delivery storage operations take
2 seconds, idle claims back off 100 ms, retries range from 1 to 60 seconds,
and order polls run every 5 seconds. The lab app's `freshness_seconds`
(default 30, nonzero) sets `Limits.freshness` for lookups, reservations and alerts.
An uninterruptible operating-system flush cannot be cancelled within a worker;
owner destructors join rather than detach. The process watchdog enforces the
overall stop deadline even when such a join blocks; see Running as a service.

The capacity probe uses native free-space APIs through `fs4`: `statvfs` on
Linux and `GetDiskFreeSpaceExW` on Windows. It starts no subprocess. Probe errors
refuse admission; the store also checks its configured reserve and size limits.

Retention is implemented under ADR 0013: supervised bounded purges, explicit
holds, keyed tombstones and offline conversion for existing stores. See
[Retention and holds](#retention-and-holds-adr-0013) below. Real machine validation
remains open. Serial hardware and Linux runtime verification remain CI/lab work;
localhost integration tests run on Windows here.

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
or credentials. This command works while the service runs. It opens the store and requires
`ANALYTE_SECRET_RETENTION_DIGEST` for finite retention periods or a retained key
fingerprint (including existing keyed tombstones), even if all policies now use
`keep`. It does not require destination API or signing credentials.

Phase 1 tests in `tests/delivery.rs` run the reference receiver library on random
loopback ports. Synthetic TCP ASTM and MLLP HL7 peers cover signed durable
content, retransmission suppression, corrections, training/debug exclusion,
conflicting content, outage/retry, authentication pause/status, bounded active
shutdown and audited interrupted-attempt recovery. When an interrupted attempt
had already arrived, restart sends identical bytes and ID and receives duplicate;
the receiver retains one record. These are documented tests, not real-machine
or external-app qualification.


## Documented vendor receive stacks

Machine definitions in `crates/core/config/examples/poct.toml`, `vidas_literal.toml`
and `autolyser.toml` are synthetic examples, not installed settings. Every vendor
policy is explicit. POCT uses TCP; VIDAS Literal and Autolyser use TCP or serial.
Field-position mappings are rejected for these syntaxes; `[codes]` names the
machine's `test_system` and uses the same exact unit translations as the record
mapper. Test codes pass through as sent ([ADR 0029](../../docs/adr/0029-lab-app-owns-test-names.md)).

Vendor input is durably journaled before the dialog sees it. Complete receipts
and interpretations keep driver/config versions. Unresolved identity is preserved
and parked, never replaced with a generated run ID. `parked list` exposes its
specific reason. VIDAS and Autolyser receive tests assert no result delivery
because their identities remain unresolved. POCT can resolve identity through
explicit device/time/analyte settings and durably retained Hello context; its
reference-receiver test covers delivery, retransmission and correction.

VIDAS work-list sending uses the shared order mapper, durable `reserve_send`
and lifecycle APIs; see [ORDERS.md](ORDERS.md). The download permission alone
does not schedule an order: `[orders]` must also explicitly enable send-ahead
and configure the patient namespace and outbound codes. Successful transfer
means Sent, never Accepted. Interrupted orders become Uncertain and are never
automatically replayed. Autolyser host-initiated insertions use the same durable path after a completed
host poll, with the order's own medium (`S` or `U` in the machine's
`specimen_system`, ADR 0030), clone flag and cup position.
Before a cursor-changing poll, the host durably reserves a typed poll intent.
Pending or uncertain custody blocks another poll, including across reconnects.
Observed failures retain their reason; confirmation requires the existing dialog's
response-custody evidence. The protected parked list still shows the linked guard.
Use the audited `parked resolve` command only after checking the machine's cursor
and retained responses: it reconciles the typed intent and keeps the parked row.
An upgrade imports old custody guards without deleting their evidence.

## Test-only boundary hooks

The non-default `test-hooks` Cargo feature exposes one one-shot ASTM hook:
`Committed`, after durable frame storage and before ACK generation. Tests
install it only on an explicitly injected loopback listener. It can pause,
resume normal processing, or stop that connection; it cannot modify wire bytes
or bypass storage. Pauses stop on cancellation, dropped control or a five-second
deadline. Without the feature, both the hook and its call site compile out.
`test-hooks` is test-only and must never be enabled for release builds.
Exercise its restart test with
`cargo test -p analyte --all-features --test delivery recording`.

Hook code lives only in its owners: `installation/config/hooks/` (program
setup and loopback destinations), `machine/hooks/` (named session pauses and
LAW order gates), `runtime/hooks/` (destinations, loopback service and
shutdown overrides) and `runtime/test_hooks/` (the bounded gate primitives),
plus `analyte-sink`'s `hooks.rs`. Without the feature each facade is inert and
holds no state; CI's production Clippy and test passes build without it.

The same feature also exposes `Program::stall_store_on_shutdown_for_test`,
restricted to the loopback test service with explicitly injected listeners.
It stalls one store operation at shutdown and sets a 500 ms process deadline;
it deliberately requires a child process because the real watchdog exits 3.
It changes no protocol or storage bytes and cannot be enabled by TOML. Run the
ADR 0003 recovery case with
`cargo test -p analyte --all-features --test delivery forced_exit_preserves_receipts_and_orders`.
The test retains committed raw receipts, confirms identical result retry bytes
and ID with one result at the receiver, and recovers Sending to Uncertain.
`tests/packaging.rs` checks the unit's structure, sandbox settings and stop
margin without requiring systemd or installing anything.


HL7 legacy QRY^Q02 queries can opt into the shared order cache and reservation
path with the explicit [DSP layout](../core/config/examples/hl7_orders.toml).
QCK/DSR replies follow durable raw storage. Trusted empty is NF; untrusted,
stale or late lookup is AE. DSR application receipt is Sent, never Accepted;
negative, missing or disconnected ACKs are Uncertain. HL7 ORM/OML send-ahead
and cancellation require the separate explicit `[orders.hl7_send]` capability. See [ORDERS.md](ORDERS.md).

## Restore and reconciliation (plan §11, ADR 0003 rules 2, 4, 8)

1. Stop the service. Keep the backup and data directory protected as clinical data.
2. Run `analyte --config analyte.toml --actor operator-name restore --from backup.db --reason "Recovery after disk failure"`.
   Integrity is checked and a backup with an older or newer schema version is
   refused before a staged copy is opened.
   Normal store owners prevent restore through an OS file lock.
3. The previous database is retained as `analyte.before-restore`; it is never
   overwritten or deleted. Existing recovery files refuse a further restore.
   Archive the previous store securely before a later restore. Interrupted
   `analyte.restore-pending` files block opening: preserve all files and investigate
   before proceeding; never remove evidence just to start the service.
4. Restart the service. Result receiving and app deliveries continue. Delivery
   retries retain the original ID and bytes; the receiver must accept duplicates.
   Machine order reservations are paused, including query answers. Untrusted
   lookups follow their configured error/silence policy, never "no orders".
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

Operators configure alert policies, named responders and the external HTTPS
webhook relay (ADR 0014) as described in
[Alerts and named responders](../../docs/operations/operate.md#alerts-and-named-responders).
That page owns the example policies, the threshold table, the relay settings,
the webhook body and the rehearsal steps. Notes for developers:

- The running service evaluates policies every `alert_interval_seconds`. A
  configuration without `[[alerts]]` raises none. `analyte status` and the admin
  overview only read durable state; they never evaluate or acknowledge alerts.
- Unknown measurements and clock rollback stay unknown (JSON null in the
  webhook), never zero. A missing, incomplete or untrusted order cache raises
  with an unknown value; the cache policy checks every configured destination.
- `machine_silent` measures the most recent durable received message or ASTM
  frame, not connection state or control bytes.
- Config validation: responders are nonempty, single-line and at most 256 UTF-8
  bytes; at most 128 policies; the interval is 1 through 3600 seconds (default
  30); `restore_paused` rejects a threshold and every other kind requires one.
  Ages raise at or above the threshold, counts strictly above it, and the disk
  margin counts above `store.reserve_bytes`.
- Raise/clear history is retained, and a restart keeps active state without a
  new raise. Removing a policy stops its evaluation but does not hide its
  retained active state.
- Transition logs carry the signal, optional machine ID, active state and
  numeric value, never responder text or clinical payloads. Webhook events carry
  no patient data, sample IDs or raw messages, and every retry reuses the stored
  event bytes and ID.
- Resolving a parked item or reconciling an uncertain order stays an explicit,
  separately audited operator action; alerts never do it.

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
and 32 concurrent connections, and form bodies to 1 KiB (16 KiB on the setup
page, whose API key alone may be 4 KiB). Service shutdown stops
admission and joins the admin owner within five seconds. Cancelled reads may
finish on the storage worker; they do not change machine orders. Cancellation
after submitting a raw-view audit may leave a durable audit without a completed
HTTP response. Retrying creates another audit, never a machine command.

The read-only overview shows machine owner state and link light,
oldest undelivered age, destination order-cache age and trust, Uncertain order
count, restore pause (only while it applies), and free disk bytes against the
configured reserve. It groups them into Machines, Sending to the lab app,
Orders, and Disk and alerts, in plain words (sizes such as "477 GB free", local
times with an age, "Nothing yet" before a machine has sent anything); each label
keeps its technical name, and each value its exact number, in the hover text.
Counts are plain numbers, red only above zero; status pills are kept for states
such as Connected or Waiting. Settings versions show the first 12 hex digits of
the configuration digest, with the full digest on hover.

Every admin page someone has signed in to has one sidebar (`src/admin/view/`):
Overview, Activity, Needs review, Machines, Lab apps and Setup, and a More group
(Machine orders, Cancelled by machine, Deliveries, Data keeping, Settings),
with the current page marked by `aria-current`. The More group is a
`<details>` element, so it works by keyboard and without scripts. The paths
are unchanged (`/parked`, `/analyzer-cancelled`, `/retention` and so on), so
bookmarks keep working. Sign-in and the refusals that have no page of their
own (an expired form, too much text, a request that took too long, an address
with no page) use a plain centred frame with no sidebar, so a page that may be
shown before sign-in names no machine, lab app or count. Refusals and failures
say what happened in plain words, with the exact technical detail under
"Details for support". Every page is an Askama template that loads
`fonts.css`, `analyte.css` and `app.css`, served from the binary under
`/static/` (light first, dark through `prefers-color-scheme`), which the
`default-src 'self'` policy allows; pages use no inline styles, inline scripts
or external URLs (the one script is `live.js`, served the same way). These files hold no data, so they are served before login (the
host check still applies).
The link light comes from the machine runtime, not from receipt age: the
transport reports each session opening, closing and read (never bytes) to a
per-machine status the runtime owns and the admin page only reads. Serial says
only whether the port is open, because an open port proves no cable or
machine. "Last data received" is the later of the runtime's last read and the
last stored receipt, so it survives a restart; `analyte status` runs in its own
process and keeps showing only the stored last receipt. Reads use bounded store
pages and deadlines. If a complete
snapshot cannot be read in time, the page reports failure rather than displaying
an incomplete count as a total. Raw payloads and sample IDs are not shown on
the overview.

The setup page (`src/admin/setup/`) writes a machine's site file and stores
destination keys in the locked credentials file
(`src/installation/credentials.rs`). It validates in memory with the same loader
as `check-config`, then copies each old file to `<name>.<time>.bak`, writes
`<name>.pending`, flushes and renames it, and loads the whole program again,
restoring the backups if that fails. Keys are checked before any file is written
and stored only after the files pass. Changing the definition rewrites only that
entry's `definition` value in the program file, located by its TOML span. The
listen-only test (`listen.rs`) opens one serial port, accepts one connection or
connects to one address and only reads, for at most 60 seconds and 64 KiB; the
bytes live in memory, appear only on the page, and are dropped when the test
ends. It refuses an endpoint a running machine uses. "Restart now" drains the
admin server and returns `RestartRequested`, which the service turns into a
graceful stop and a nonzero exit, so the service manager restarts it. The
setup page names each machine model by the `# ` title on the first line of its
README (for example "Roche cobas e 411"), falling back to the tidied
`vendor/model` key, which stays on hover (`setup/definitions.rs`).

Activity (`/activity`, `src/admin/activity/`) lists, newest first, the
rows the store already keeps: messages stored, items parked for review and
resolved, delivery tries (delivered, will retry, rejected, paused, conflict),
machine orders' new states and audited changes. Each line shows the time stored
with it; an item parked before schema 16 has none and sits last under "Time not
recorded". Machine links and disconnects are not stored, so they are not listed.
It filters by machine and by type (Results, Orders, Problems, Changes) with one
GET form, counts today's lines, and pages 50 lines at a time after the last line
shown. It shows sample IDs and counts, never test codes, values or the reasons
people typed.

Failed tries in a row of one delivery that ended the same way (failed, or cut
off) are one line, at the newest of them: "sending to main-lab failed · tried
35 times, 02:32 – 12:01 · next try after 12:02". The next try is said only
while the delivery still waits for it. A try of that delivery that ended
another way (delivered, rejected, paused, a conflict, or failed the other way)
ends the run; tries of other deliveries in between do not, because while a lab
app is down every waiting delivery retries on its own clock, so their tries
take turns. Two lab apps failing in turn are therefore two lines. The line
links to `/activity?delivery=<id>`, which lists every try of that delivery,
one line each, newest first. The line sits at its newest try's place, so a
run never splits across pages, a page counts lines as shown and today's
counts count lines as shown. Its key for the live script is the delivery and
its first try, so a new try updates the line in place. Every try stays
stored; only the page folds them. The store keeps the runs (schema 18,
`attempt_runs`), so the page reads one row per line, however long an outage.

Live updates (`src/admin/server/live.rs`, `assets/live.js`): signed-in pages load one
script file. It opens `GET /events`, a server-sent event stream that says only
which areas changed (overview, review, machines, deliveries, activity) and the
PC's time, never page content. Each page's `<body data-live>` names its area;
when that area or the Needs review count changes, the script fetches the page
again and swaps the parts marked `data-live-region` (the sidebar's links, and
the main column of a page with an area). It leaves a part alone while it is
under the pointer, holds the focus, an open disclosure, a dialog or something
typed but not sent. The sidebar reads "Live · 10:45:12" while connected and
"Page loaded 10:45" otherwise; the script retries after 1, 2, 4 ... 30 s. Each
stream compares the store's marks (`activity_marks`: the newest row of each
watched table, one index lookup each) and the link lights every 2 s, beats
every 10 s, ends after 5 minutes with a goodbye, holds one of 4 stream permits
(of 32 connection slots) and ends at once at shutdown or sign-out. Without the
script every page is unchanged.

Use the menu for Machines, Needs review (parked items), Machine orders and Deliveries. Lists
show at most 50 entries and may show fewer under the configured store page bound.
Follow Next page to continue. Cursors are nonnegative and bounded; unknown
query fields are refused. Parked lists show category, reason, linked identities
and receipt times without loading payloads. The page labels receipt time and
explicitly reports missing times; the parking time the store records since
schema 16 is shown on Activity.
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

Parked-item detail pages link to **Reprocess**, and the bounded **Deliveries**
page links eligible entries to **Resend**. Each link opens a confirmation page;
only its POST form performs the action. A reason and the authenticated session's
CSRF token are required, and the audit records the session actor. Resending a
superseded delivery additionally requires the explicit checkbox acknowledging
that older delivery. GET requests never change interpretation or delivery state.

Admin reprocessing loads current settings from the service's original program
file and refuses a changed data-directory target. Validate edited files with
`check-config` first. Reprocessing uses those edited files; it does not hot-reload
the running receive sessions. An action timeout can leave a durable effect: inspect
the audit and version or delivery state before repeating it. Resolving and order
reconciliation remain CLI-only.

## Running as a service

Installation, the service accounts, paths, logs and stopping are operator topics:
see [Install the pilot](../../docs/operations/install.md), the
[systemd guide](../../packaging/linux/README.md) and the
[Windows service guide](../../packaging/windows/README.md). Evidence remains
**documented**: synthetic subprocess and SCM seam tests do not establish a real
SCM installation, systemd runtime or machine qualification. Notes for developers:

- Ship a release build without `test-hooks`. The release builds unsigned
  installers and packages; there is no signing or automatic update mechanism.
- A program entry without `definition` is a machine that is not set up yet. The
  loader accepts it only with `[admin]` enabled and without `capture` or
  `overrides`; the service does not run it, `check-config`, `status` and the log
  name it, and the setup page finishes it by adding the `definition` line.
- `analyte service run --config <path>` is the Windows SCM entry point; invoking
  it outside the SCM fails explicitly. `analyte run --config <path>` runs in a
  terminal or under systemd. Neither mode installs a service in Rust; the
  packaging scripts do that.
- Secrets come from the platform's protected service environment, never TOML or
  command arguments.
- Both commands accept `--shutdown-timeout-secs` (30 through 300, default 30).
  A stop request drains admitted receives and deliveries, closes transports,
  finishes captures, then closes the store. Capture workers share the remaining
  time up to five seconds before the process deadline, leaving that allowance
  for store close. A dedicated OS thread
  enforces the overall deadline even if a blocking owner destructor cannot
  finish. At expiry Analyte logs the IDs of the remaining
  components and exits with code **3**, bypassing destructors. This follows ADR
  0003 crash recovery: unacknowledged or uncommitted input can be retried,
  committed receipts survive, deliveries keep their IDs and interrupted Sending
  orders become Uncertain. A graceful stop exits 0; other failures exit nonzero.
- On a requested Windows stop, the watchdog allows up to 250 ms at the deadline
  for the SCM Stopped notification before exiting with code 3. This lets Windows
  show a normal stop without triggering recovery actions. Windows can impose its
  own shorter limit during OS shutdown.
- systemd records exit code 3 as a failure. Its stop timeout must stay above the
  Analyte deadline. The shipped unit's `TimeoutStopSec=35s` keeps a five-second
  margin.

## LAW program integration

The program receives results, answers supported queries and sends/cancels work
on one persistent TCP link in either configured direction, or on independent
directional links: inbound query and result links and an outbound work link.
Evidence remains
**documented**. The [LAW checklist](../../docs/protocols/law/README.md) owns the
current test matrix and remaining actor/simulator qualification gates.

Folder and serial worker shutdown reports worker errors and panics separately,
without including error text or panic payloads in ordinary logs.

Receipt and interpretation config versions are the lowercase SHA-256 digest of
the canonical effective config. Each host connection has a fresh UUID, shared by
its receipts and parked evidence; a watched-folder session has its own UUID.

VIDAS, Autolyser and POCT disconnect and storage-failure dialog effects are
processed before the host returns. Parking retains the reason with the existing
transport evidence; a failed parking write is reported alongside its original error.

## Capturing conversations

Plan §10 and ADR 0009: set `capture = true` on a `[[machines]]` entry in the
**program** TOML, then restart. It defaults to false and does not belong in the
shipped machine definition. Capture-enabled machine IDs use ASCII letters,
digits, underscores or hyphens so each names exactly one directory.

Every successful TCP connection (client or server), serial open and watched-folder
receipt gets a separate UUID-named `.rec.jsonl` file under
`<data_directory>/captures/<machine>/`. The existing transport recorder preserves
exact bytes, directions, chunk boundaries and relative times. Live headers declare
`machine` evidence and `NOT de-identified`; they do not qualify a driver.

These files **contain patient data**. Keep them in the protected store, apply the
installation ACL on Windows, and never put them in ordinary logs (rule 10).
Directories and files use owner-only permissions on Unix. Before sharing, work
on a copy: remove or replace every patient and specimen identifier consistently,
inspect every decoded event (including link metadata), and record the review in
the header de-identification statement. Preserve the protected original. Synthetic
simulator captures may be labelled `simulator` only after reviewing their content;
never relabel a patient capture as synthetic.

Recording is optional evidence, not clinical custody. A recording error disables
that connection's capture; receiving, ACKs and delivery continue. One specific
error is logged without payloads. `analyte status <program.toml>` shows retained
capture failure status; a missing failure is not proof of completeness. Failure
markers live in the protected data directory (`capture-<machine>.failure` and
`<recording-name>.failed`). They remain until an operator archives them after
review. If status cannot be persisted, the log reports that failure too.

Each recorder has a 64-event queue and the ADR 0009 codec's file/event/byte limits.
The host bounds unfinished workers to 16 per machine, including the failure
marker worker, and joins them at shutdown. If a recording worker is still
writing at the capture cutoff, Analyte logs the affected capture and queues
its failure markers. The process watchdog exits with code 3 at the overall
deadline, even if the recorder or the failure marker write is stuck.
Before each write it requires the store's configured reserve plus a 16 MiB margin;
unknown capacity stops recording. A saturated queue, limit or low disk ends capture
visibly rather than silently omitting bytes. Incomplete/failed captures are not
conformance evidence. Capture does not acknowledge data or change replay policy.

### Record only (ADR 0034)

A site file with `[recording]` makes the machine record only: the definition
entry names the model's `machine.toml` or, for a model not ready yet, its
`record.toml`, and the snapshot is a `Stack::Record` built from the shipped
capture profile of its protocol (`crates/core/config/src/recording/profiles/`)
with no lab app, mappings or orders. Capture is always on for it and an
overrides file is refused. Its hosts (`machine/<protocol>/recording.rs`) run
the protocol's own dialog behind the chunk-journaling runner, so every chunk
is a stored receipt (driver namespace `analyte-record`) before any reply. The
ASTM host asks the store for room before ENQ and answers a query with
termination code Q; the HL7 host answers ORU and OUL with AA and every other
message with AE; the VIDAS host acknowledges uploads only; the Autolyser host
sends only R, guarded as on a normal connection, and stops on an unusable
response. Record-only receipts are never interpreted, parked, delivered or
reprocessed, and startup recovery leaves them out. The link status counts the
messages kept and the failed exchanges since start for the pages, `machines
list` and `doctor`. `analyte captures list` and `export` read the capture
folder after an audit row.

## Retention and holds (ADR 0013)

Choosing a period, the retention key, holds and what is purged are operator
topics in [Retention under ADR 0013](../../docs/operations/recovery.md#retention-under-adr-0013);
[ADR 0013](../../docs/adr/0013-retention-of-delivered-data.md) owns the
eligibility rules. Notes for developers:

- Every site file must set `retention`; there is no default. Teaching examples
  choose `keep` so they never enable deletion by accident. `keep` starts no
  purge worker.
- A finite policy requires `ANALYTE_SECRET_RETENTION_DIGEST` (32 random bytes as
  64 hex characters), separate from API and signing keys and never in
  configuration, the database or logs. A missing or different key is refused
  when a fingerprint exists; rotation is unsupported.
- Every store is created with incremental vacuum; startup refuses a finite
  policy on a file without it.
- The supervised worker examines at most sixteen receipt and sixteen order
  candidates per machine every minute, each batch in its own transaction with a
  250 ms SQLite budget. A scan cursor keeps older ineligible receipts from
  starving later candidates. A successful batch commits its audit, tombstones
  and deletions together; a failed transaction rolls back and records a failed
  run, and a deadline is retried at the next interval.
- Backup holds a shared maintenance lock and purge an exclusive one, so a backup
  requested during a purge batch reports busy maintenance. Restore needs
  exclusive ownership of the whole store, and restore reconciliation also pauses
  purging. Retention never replays or resends a machine order.
- Online reclamation frees at most 32 pages per iteration within a 250 ms budget
  and a nonwaiting WAL checkpoint; readers can delay it and a later iteration
  retries. Secure deletion zeros deleted content in SQLite pages; it does not
  promise forensic erasure of older files, WAL snapshots or backups.
- Retention keeps ambiguous cases: parking evidence (including resolved items),
  receipts without a delivery for every interpreted result, and receipts on a
  machine with unfinished order or poll work. Order holds and open operations
  block linked receipts; missing, ambiguous or version-inconsistent evidence
  blocks conservatively, and legacy machine-only polls still block the machine.
  Completed orders are purged only after their final-state and acknowledgment
  periods with no held or unfinished linked work. Keyed order tombstones stop
  replay if the app republishes an old order. Unlinked incomplete frames stay.
- Holds have no expiry and every transition records the actor and reason.
  Purged receipts cannot be reprocessed and their deliveries cannot be resent;
  each refusal names retention as the reason.
