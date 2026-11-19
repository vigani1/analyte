# Operate and respond

## Admin page and operator token

Add this optional table to the program file to enable local administration:

```toml
[admin]
port = 8080
```

Only IPv4 `127.0.0.1` is accepted. Omitting `[admin]` disables the page.
After protecting the data directory, provision the token before startup:

```text
analyte --config analyte.toml admin token --rotate
```

This writes `<data_directory>/admin-token` and prints the token once. Treat the
output as a secret; do not capture it in logs. On Windows the file and data
directory may grant only SYSTEM, Administrators, the running account and
`NT SERVICE\Analyte`; anything else is refused. Unix requires mode 0600.
Preserve and investigate any `admin-token.pending` file after an interrupted
rotation. Restart after rotation to invalidate the loaded token and all sessions.

Open `http://127.0.0.1:8080/login` on the host and enter the token (the page
calls it the admin key). A bearer
header can also authorize reads. Browser sessions expire after one hour.
All sessions share one administrator credential; there are no per-person roles.
Browser audit actors identify sessions, not people. Use explicit `--actor`
identities for accountable CLI recovery work. Do not forward this HTTP listener
to a shared network.

The menu has four tabs, Overview, Needs review, Machines and Setup, and a
**More** dropdown with Machine orders, Cancelled by machine, Deliveries and Data
keeping. The current page is marked. Page addresses did not change when the
pages were renamed (Needs review is still `/parked`, Cancelled by machine
`/analyzer-cancelled`, Data keeping `/retention`), so bookmarks keep working.

| Page | Use |
| --- | --- |
| Overview | Four sections: Machines (connection light, last data, service), Sending to the lab app (results waiting, order list up to date, oldest result waiting, results still being delivered for removed machines), Orders (orders with unknown outcome, orders cancelled by a machine, and "Order sending paused after restore" only while it applies) and Disk and alerts (free space, space always kept free, active alerts). Counts are plain numbers, red when above 0. Hovering a label or value shows its exact technical name or value. |
| Needs review | Items Analyte kept but could not process (parked items): reason, linked records and "Process again with current settings". Showing the original bytes requires an audited browser POST. |
| Machines | Config, link light and activity. TCP server: Waiting or Connected. TCP client: Connected or Not connected (retrying). Serial: only Port open or Port not open, because an open port proves no cable or machine. Watched folder: Watching or Not watching. Last data received ("Nothing yet" before anything arrived) and the time of the last change show as a local date and time with an age. The settings version shows its first 12 digits; hover for the full digest. |
| Setup | Add or remove machines; choose a machine model, its port or address and the lab app; test by listening; save; restart. See [Setup page](#setup-page). |
| Machine orders (More) | Inspect lifecycle; reconciliation remains a CLI task. |
| Cancelled by machine (More) | Orders a machine cancelled on its own, to check with the lab and the lab app. See [LAW execution](#law-execution-and-analyzer-cancellation). |
| Deliveries (More) | Inspect status and confirm "Send again" for eligible deliveries, with a reason. |
| Data keeping (More) | How long data is kept, the last clean-up, the oldest message kept, and holds (retention and holds). |

Follow Next page to inspect bounded lists. "Process again" (reprocess) uses
retained raw bytes with current config and creates a new interpretation. "Send
again" (resend) queues an existing delivery with its original ID and bytes. Neither resends machine orders.
Raw views contain protected clinical evidence. Keep them out of screenshots
and support tickets. If an action times out, inspect state before repeating it;
it may already have committed.

For local checks from a protected operator terminal:

```text
analyte status analyte.toml
analyte --config analyte.toml --actor pilot-it parked list
analyte --config analyte.toml --actor pilot-it orders list
analyte --config analyte.toml --actor pilot-it deliveries list --reason "Pilot review"
```

`status` takes its config as a positional argument. Other commands above use
the global `--config`. Recovery operators need OS access to the protected store.
Lists are bounded; use `--after` with the last listed local ID/sequence where
offered. CLI output can contain protected evidence.

## Setup page

The **Setup** page lets a lab manager set up an installed machine without
editing files. It needs a browser login (not just the bearer header), and the
service needs write access to the folder that holds the program file and the
site files (see [Install](install.md)). Each machine listed in the program file
has a **Set up** link with five steps and a **Remove** button. The table shows
each machine's model by name (for example "Roche cobas e 411", from the first
line of the model's README) and the short settings version. A machine
listed without a `definition` (the installers' `machine-1`, or one added with
**Add machine**) shows as **Not set up yet**: it does not run until you choose
its definition in step 1 and save; the save adds the `definition` line to its
program entry, and the restart starts it. A machine saved or added since the
last start shows as not running until the restart; a removed one shows as
running until the restart.

**Add machine** asks for a name and suggests the next free `machine-N`. A name
uses letters, digits, `-` and `_`, because it also names the machine's site file
(`<name>.site.toml` beside the program file); a name already listed (ignoring
case) or one the page uses itself (`add`, `listen`, `restart`) is refused before
anything is written. The page adds one `[[machines]]` entry without a definition
and writes the same starter site file the installers write (retention `keep`
and the key names; no address, connection or key). Analyte keeps results,
flagged items and history by machine name, so a name that belonged to a removed
machine joins that history again: the page asks you to confirm it is the same
analyzer, and a different analyzer should get a new name.

**Remove machine** first shows what is still unfinished for that machine:
results or order messages the lab app has not accepted, machine orders sent and
not finished (sending, sent, accepted, cancel requested or uncertain), received
messages not yet read, and unresolved flagged items linked to it (items linked
to no single machine are not counted). If anything is open, the button becomes
**Remove anyway**. The page then says in one sentence what happens to waiting
results, for example "Its 3 waiting results will still be sent to lab-app." Removing takes only the entry out of the program file and
moves the site file to a dated copy (`<name>.site.toml.<time>.bak`); the store is
never changed, so the machine's stored results, flagged items and history stay
visible on the other pages.

Stored results and order messages keep being delivered after a removal, even
when no other machine uses the same lab app. Removing keeps the machine's lab app
destination (its name, address, deadlines and key names, never the keys) in the
program file as a `[[retired_destinations]]` table, in the same checked write. At
each start, Analyte runs a delivery-only worker for every kept destination that
no set-up machine uses: it sends what is waiting with the usual retries and
shutdown limits, and stops when nothing is left. It never asks that lab app for
orders. Items paused, parked or conflicting keep it running, so an operator's
release or resend is still sent; anything added after it stopped is sent after
the next restart. A set-up machine that uses the same destination name wins:
its own settings are used, as before. While anything waits, the Overview and the
Setup page say so, for example "Removed machine machine-2: 3 results or order
messages still being delivered to lab app lab-app"; the line goes
away when the count reaches 0. Remove a `[[retired_destinations]]` table by hand
only when nothing waits for it.

If a destination's settings cannot be read when its machine is removed (for
example a broken site file), nothing is kept and the page says so. Results that
wait for a destination that is configured nowhere stay stored and are never
deleted; the pages show "Cannot send to lab app X: no machine uses it any more", the
service log says so at each start, and an `undeliverable_destination` alert, if
configured, raises for them with the reason and the fix (see
[Alerts](#alerts-and-named-responders)). Set up a machine that uses that
destination to deliver them. If a kept destination's keys are missing, its
deliveries pause with "destination credentials require operator repair", the
service keeps running, and the same alert raises with that reason. The
Overview and the Setup page then say "Cannot send to lab app X: its API key
or signing key is missing" with the same fix, not "still being delivered".
Removing the last machine is allowed: Analyte then serves only the admin page
(and delivers what still waits) until a machine is added.

Saving, adding and removing all use the same steps: check first, keep a dated
copy of each file they change, write safely, load the whole program again as
`check-config` does, and put every file back if that fails. A machine may not
use a serial port, USB serial number, network address or folder that another
machine uses; the page refuses it before writing, and `check-config` refuses a
program written that way by hand.

1. **Machine model.** Choose a model from the `machines` folder beside the
   program file. Each shows its name, how it was checked (its evidence level, on
   hover) and whether it is ready; blocked definitions (no `machine.toml` yet)
   show as **Not ready yet** and cannot be chosen.
2. **Connection.** For a serial machine, choose a detected port. The list shows
   each port's name, the adapter's own description and its USB serial number.
   When the adapter has its own serial number, the site file gets `usb_serial`,
   so Analyte finds the machine again if the COM number changes; otherwise it
   gets the port name as `endpoint`. A number Windows made up (it contains `&`)
   is not used. For a network machine, enter the IP address and port: the
   address Analyte listens on when the machine connects to Analyte, or the
   machine's address when Analyte connects to it.
3. **Test the connection (listen only).** Analyte opens the chosen port or
   address and shows, as hex and readable text, what the machine sends. It sends
   nothing, not even an acknowledgment, so the machine may retry or report a
   link error; that is expected. The test runs at most 60 seconds and keeps at
   most 64 KiB; its bytes stay in memory only, are shown only on this page, and
   are discarded when the test ends or is stopped. They may contain patient data:
   keep them out of screenshots. The test refuses a port or address that a
   running machine uses; stop that machine's service first or test a free port.
4. **Lab app.** Enter the lab app address (HTTPS, checked with the same rules as
   `check-config`) and the destination name. Enter the API key and signing key
   only to set or replace them. They are stored in the [locked credentials
   file](connect.md#credentials) and never shown again; the page shows only
   "set on <date>", or that an environment variable sets the key (it wins).
5. **Save.** Analyte first checks everything with the `check-config` rules. Only
   then does it keep the old site file as `<name>.<time>.bak`, write the new one
   and store the keys. If you chose a different definition, the program file's
   `definition` line for that machine changes too (with its own backup); nothing
   else in it changes. It then loads the whole program again; if that fails,
   the previous files are put back and the page says why. The page shows the new
   settings version (the first 12 digits of the configuration digest; hover for
   all of it).

Saved settings apply only after a restart. **Restart now** stops Analyte
gracefully (like a normal stop) and exits with a failure code, so the service
manager starts it again: the Windows recovery action after 5 seconds (15, then
60 seconds for further restarts within a day), or systemd `Restart=on-failure`
after 5 seconds. A foreground `analyte run` simply stops;
start it again by hand. Log in again afterwards; the Setup page shows the running
configuration digest, which should match the one shown after saving. Saves,
added and removed machines and restarts are audited (session and machine name)
without key values. A save writes only the site file, that one program line and
the locked credentials file; retention, time zone, credential names and other
site keys are kept as they were. Adding and removing change only that machine's
entry in the program file and its site file; removing also adds or updates the
`[[retired_destinations]]` table for its lab app destination.

## Alerts and named responders

Alerts are opt-in. No policies means no newly evaluated alerts. Put
`alert_interval_seconds = 30` at the top level of the program file, before
tables. Append policies such as these synthetic examples:

```toml
[[alerts]]
kind = "parked_items"
threshold = 0
responder = "Lab duty officer"

[[alerts]]
kind = "disk_reserve"
threshold = 1073741824
responder = "Lab IT on-call"

[[alerts]]
kind = "restore_paused"
responder = "Recovery lead"

[[alerts]]
kind = "undeliverable_destination"
threshold = 0
responder = "Lab app support"
```

| Kind | Threshold meaning | Response to agree |
| --- | --- | --- |
| `oldest_undelivered` | Age in seconds, at or above | App support checks outage, rejection and credentials. |
| `machine_silent` | Seconds, at or above; requires installed `machine` ID | Operator checks expected activity and cabling. Enable only for normally busy machines. |
| `disk_reserve` | Free bytes below reserve plus this margin | IT restores headroom without deleting evidence. |
| `order_cache_stale` | Age in seconds, at or above | App support checks the feed and trust. |
| `uncertain_orders` | Count strictly above | Operator reconciles with machine and app. Never blindly resend. |
| `parked_items` | Count strictly above | Duty officer investigates the recorded reason. |
| `restore_paused` | No threshold allowed | Recovery lead completes reconciliation. |
| `undeliverable_destination` | Count strictly above, per lab app destination | See below: the alert names the destination, the count, the reason and the fix. |

`undeliverable_destination` raises one alert per lab app destination that holds
results or order messages this run cannot deliver. They stay stored and are never
deleted. Its two reasons, as status and the Overview show them:

| Reason | Means | Fix |
| --- | --- | --- |
| `not_configured` | No machine and no kept lab app setting uses destination X (for example its machine's settings were unreadable at removal, or a machine's destination was renamed). | Set up a machine with lab app destination X on the setup page, or restore its settings; the results are kept and will be sent once it is configured. |
| `credentials_missing` | Destination X is kept, but its API key or signing key could not be loaded; its deliveries are paused. | Enter the API key and signing key for destination X on the setup page (on a machine that uses it) or in the service environment, then restart; the results are kept and will be sent once the keys are set. |

It clears when the items are delivered or, after a restart, when the destination is
configured again with its keys. Keys that the lab app rejects are not this alert:
they pause deliveries for repair and `oldest_undelivered` covers them by age.

Zero count thresholds catch any item. Unknown measurements raise rather than
silently clear a policy. Set ages and margins from the agreed outage budget.
Each policy requires a nonempty named role/person, with cover recorded outside
Analyte. The evaluation interval accepts 1–3600 seconds; default is 30.

Alerts appear in status and the admin overview. Raise/clear transitions are
stored durably and logged without responder text or patient data. External
notification is opt-in through the HTTPS webhook setup below. Removing a policy
does not clear its retained active state or invent a resolution.

### Configure the external relay

The lab owns the relay to email, chat or SMS and its staffed responder cover.
Analyte sends operational JSON, not clinical results. Use only operational names
for configured machine IDs and responders; never put patient or sample details
in these settings. SMTP, SMS providers and escalation chains are not built in.

Append this synthetic example to the program TOML, outside the `[[alerts]]`
policy tables:

```toml
[alert_delivery]
reminder_interval_seconds = 3600
queue_capacity = 256
request_timeout_seconds = 10
backoff_min_seconds = 5
backoff_max_seconds = 300
max_attempts = 10

[[alert_delivery.endpoints]]
id = "lab-relay"
url = "https://relay.example.invalid/analyte-alerts"
secret = "alerts-key"
```

Replace the URL with the lab's relay. Each endpoint receives all alert policies,
with the responder from each policy. Multiple endpoints have independent queues
and workers; at most eight can be configured. No endpoints means no external
sender. A new endpoint starts at retained transition history, so qualification
must account for historical raises and resolutions as well as current conditions.

Provision `ANALYTE_SECRET_ALERTS_KEY` in the service account's protected installation
environment. The `secret` reference uses the existing name convention: uppercase
ASCII letters/digits, other characters replaced with underscores, prefixed with
`ANALYTE_SECRET_`. The value is a nonempty single-line ASCII bearer token of at
most 4096 bytes. It is never a TOML value or repository file. Missing or invalid
credentials fail visibly in the endpoint queue; they do not stop clinical intake.
After rotating the service environment, restart the service to load that environment.

URLs must use HTTPS with a verified certificate and have no embedded credentials,
query or fragment. Requests go to the exact configured path; Analyte does not
append `/alerts`. Redirects and proxies are disabled. A request carries
`Authorization: Bearer ...`, `Content-Type: application/json`, and `webhook-id`
matching the body's `event_id`. A 2xx response means the relay accepted the event.
Other statuses, transport failures and timeouts consume the bounded attempt budget.
Response bodies are not read into alerts or ordinary logs. Events for one
condition are delivered in queue order; a failed raise holds its later reminders
and resolution until recovery. Other conditions can still make progress.

The version 2 body has only these fields: `version`, `event_id`, `condition_id`,
`episode_id`, `event`, `kind`, `machine_id`, `destination_id`, `reason`, `responder`,
`observed_at`, `episode_at` and `measurement`. Version 2 added `destination_id` (the
lab app destination name) and `reason` (`not_configured` or `credentials_missing`);
both are null except for `undeliverable_destination`, so a relay can say which
destination and which fix from the table above. Events queued before an upgrade
keep their version 1 bytes, which lack those two fields, because every retry
reuses the stored bytes. `event` is `raised`, `reminder` or `resolved`. Timestamps are
Unix seconds; observation time is the last actual evaluation, not a guessed fresh
measurement. `measurement` has `unit` (`seconds`, `bytes`, `count` or `paused`) and
`value`; an unknown value is JSON null. No patient data, sample IDs, raw messages,
clinical values, URLs, credentials or parking explanations are included.

The relay must store `event_id` and its effects before returning 2xx. An identical
retry must have one effect; reject conflicting bytes under that ID. A crash after
acceptance can repeat the same ID and bytes. `condition_id` groups a condition
across repeats; `episode_id` distinguishes a later raise after resolution. Do not
deduplicate solely by condition: that would hide reminders and later episodes.
Restoring an older backup can repeat events, so retain relay deduplication records
according to the site's backup and outage policy.

| Setting | Accepted range |
| --- | --- |
| Reminder interval | 60–86400 seconds; missed intervals do not create a burst. |
| Queue capacity | 1–4096 pending events per endpoint, including exhausted events. |
| Request timeout | 1–300 seconds. |
| Retry minimum/maximum | 1–86400 seconds, minimum no greater than maximum. |
| Attempt budget | 1–100 attempts per event, including interrupted reserved attempts. |

Attempts reserve their retry deadline before sending. Delay doubles from the
minimum and is capped at the maximum. The queue has at most one outstanding
reminder per condition and endpoint. Full queues stop admission, preserve the
retained transition cursor, and show `queue_full=true`; they never delete pending
notifications or stop clinical workers. Admission catches up when space returns.

### Rehearse and respond

1. Use an isolated synthetic installation and the lab's test relay. Enable a safe
   synthetic condition such as parked-item count. Verify the named responder
   actually receives a raise, one reminder at the configured interval and a
   resolution. A relay HTTP success alone does not prove a person was contacted.
2. Stop the relay, observe `failed`, `next_attempt`, `exhausted` and `queue_full`
   in admin overview or `analyte status analyte.toml`. Restore the relay before
   the budget expires and verify retry identity and bytes remain unchanged.
3. Exercise queue exhaustion and a slow relay. Confirm intake and app delivery
   continue and queued transition evidence remains. Rehearse restart and restore
   with relay deduplication, and record the results in the pilot sign-off.
4. If attempts are exhausted, fix the relay or credentials, check possible prior
   acceptance, then explicitly authorize a fresh attempt budget:

   ```text
   analyte --config analyte.toml --actor pilot-it retry-alerts lab-relay --reason "Relay repaired; acceptance checked"
   ```

   This audited command retries exhausted events with their original IDs and
   bytes. It does not resend any clinical delivery or machine order. Repeating
   the command with no exhausted work refuses. Inspect status after an uncertain
   command outcome rather than assuming the reset did not commit.

`oldest_age_seconds` uses the oldest queued observation; a clock rollback is
unknown. Status also shows pending count, last success, next retry, failed and
exhausted counts and queue-full state. A configured endpoint whose durable state
is unavailable is shown explicitly as unavailable. An alert delivery failure
never raises another alert. Removed endpoint queues stay
visible; restoring the same identity resumes their progress. Up to 128 endpoint
identities can be retained over the installation lifetime. Do not rename an
endpoint merely to bypass failure or erase queued evidence.

Agree a staffed checking schedule, cover and fallback escalation route before
pilot use. Independently monitor stopped services, unreadable stores and failed
disks; Analyte cannot guarantee delivery of an alert about its own total failure.

## Storage and refusal

The live database is `<data_directory>/analyte.db`. Protect that directory,
WAL/SHM files, raw evidence, logs, handoff folders and backups. Ordinary logs
hold diagnostics and component IDs, not message payloads.

| Program `[store]` setting | Meaning |
| --- | --- |
| `reserve_bytes` | Free-space floor kept for safe operation. |
| `database_bytes` | Database size budget. |
| `record_bytes` | Per-record bound; must fit machine input budgets. |
| `page_rows` | Bound on store query pages. |
| `busy_timeout_ms` | Bounded SQLite contention wait. |
| `startup_timeout_seconds` | Opening, integrity and migration budget; 30–3600 seconds, default 600. |

Choose budgets from measured traffic, outage length and retention; the example
values are not a qualified sizing recommendation. The disk probe must work
under the service account. Probe failure also refuses admission.

“Refuse visibly” means no positive acknowledgment for unstored data. Depending
on protocol, the peer receives a negative response or the connection closes.
ASTM stores each frame before its ACK; Autolyser stores the poll response before
the destructive read. See [ADR 0010](../adr/0010-acknowledgment-boundaries.md).
Diagnostics report the storage failure. A failed machine owner can stop while
other owners run; store/delivery failures can stop the service. A failed disk
does not guarantee a durable local alert, so use the independent host checks.

Stop affected work, tell the lab operator and follow the agreed downtime
procedure. Keep results on the analyzer where possible. Restore space through
approved backup/log rotation or more capacity; never delete the database,
journals or parked evidence to make startup pass. Verify the cause is fixed,
then inspect retained and machine-side work before resuming.


## LAW execution and analyzer cancellation

Use the authenticated overview's **Review cancelled orders** link, or the
**Cancelled by machine** page under More (analyzer-cancelled AWOSs), for the
reconciliation list. Machine orders show execution alongside command state;
open an item's execution history to review receipt/interpretation links,
receive and host-recorded times, original estimate and zone evidence, scoped
notes and rejected proposals. History is paginated. Missing execution means
unknown; an absent estimate does not erase an earlier supplied estimate.

Analyzer CA is an analyzer fact, not confirmation of the lab app's cancellation
request. Check pending intent with the lab app and the original LAB-28 exchange.
Do not resend work or create a cancel to clear the display. A UC alongside CA
is parked for reconciliation and both facts remain visible. No sibling is
cancelled automatically. Contract v1 has no execution-status event, so the
assigned admin responder owns this review; clinical decisions remain with the
lab and lab app. `status` reports only an aggregate cancellation count.

LAW custody receipts and linked work remain retained conservatively until a
specific eligibility rule supports their disposition/history records. Include
that growth in disk planning; the usual reserve refusal still stops receiving
visibly. A restart interprets pending raw LAW receipts before connections open;
a receipt from a changed configuration is parked for explicit audited
reprocessing. Previously parked status-only notices are never bulk-accepted.
