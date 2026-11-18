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

Open `http://127.0.0.1:8080/login` on the host and enter the token. A bearer
header can also authorize reads. Browser sessions expire after one hour.
All sessions share one administrator credential; there are no per-person roles.
Browser audit actors identify sessions, not people. Use explicit `--actor`
identities for accountable CLI recovery work. Do not forward this HTTP listener
to a shared network.

| Page | Use |
| --- | --- |
| Overview | Active alerts, owner state, activity, pending age, cache trust, uncertain orders, restore pause and free space. |
| Machines | Config and activity; connection state is explicitly unknown. A running owner does not prove a connection. |
| Parked items | Reasons and linked evidence. Raw disclosure requires an audited browser POST. |
| Deliveries | Inspect state and confirm eligible resends, with a reason. |
| Machine orders | Inspect lifecycle; reconciliation remains a CLI task. |
| Retention and holds | Policy, last purge, oldest retained receipt and indefinite holds. |

Follow Next page to inspect bounded lists. Reprocess uses retained raw bytes
with current config and creates a new interpretation. Resend queues an existing
delivery with its original ID and bytes. Neither resends machine orders.
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

The version 1 body has only these fields: `version`, `event_id`, `condition_id`,
`episode_id`, `event`, `kind`, `machine_id`, `responder`, `observed_at`, `episode_at`
and `measurement`. `event` is `raised`, `reminder` or `resolved`. Timestamps are
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
is unavailable is shown explicitly as unavailable. Removed endpoint queues stay
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

Use the authenticated overview's **Analyzer-cancelled AWOSs** link for the
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
