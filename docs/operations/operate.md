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
output as a secret; do not capture it in logs. Windows protection comes from
the data-directory ACL. Unix requires mode 0600. Preserve and investigate any
`admin-token.pending` file after an interrupted rotation. Restart after rotation
to invalidate the loaded token and all sessions.

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
durable and logged without responder text or patient data. Naming a responder
does not contact them. **There is no email, chat, SMS or notification webhook
sender.** Agree a staffed checking schedule and escalation route before pilot
use, including independent detection of a stopped service or unreadable store.
Removing a policy does not clear its retained active state.

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
