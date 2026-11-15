# Orders in the program

Evidence level: **documented**. Tests use synthetic TCP peers and the repository's
reference app receiver. No real analyzer has qualified this implementation.

## Enable a machine explicitly

Append an optional `[orders]` section to the machine definition. The synthetic
[orders section](../config/examples/orders.toml) shows every required table.
It is separate from incoming result mappings. Without this section the host
answers no order queries and sends no work lists. `send_ahead` is explicit;
the presence of `query` enables queries and requires both a deadline and an
untrusted-cache policy.

Each outbound test maps an exact app `{namespace, value}` pair to explicit
ASTM universal-test components. Priorities also require explicit translations.
Specimen IDs retain their spelling. A required position missing from the app
order causes rejection; neither the sample ID nor a made-up slot substitutes
for it. Contract v1 position and specimen type are forwarded only to explicit
outbound destinations.

The first outbound repertoire uses standard ASTM delimiters and ASCII without
reserved delimiter characters in data. Unrepresentable input is rejected,
never trimmed or changed. ASTM field destinations are restricted to their
standard O-record roles; optional instrument ID/position uses O field 4.

## Query scope

The supported LIS02 §11 subset requests orders with information status `O`,
an explicit specimen ID in the second starting-ID component, optional repeated
specimen IDs, and an empty or `ALL` test selector. Ranges, patient/vendor IDs,
historical/date filters and other request types follow the error/no-answer
policy. Status `A` cancels the pending lookup. One query is outstanding per
connection; a new transfer supersedes its pending answer.

The query deadline starts at the transfer's ENQ, before storage admission and
frame journaling. Answers use only a complete cache younger than 30 seconds,
the same freshness policy used by the app poller. Trusted empty answers use
no-information termination. Untrusted or late lookups produce query error or
silence according to the explicit policy. Mapping failures never produce a
partial order answer; the rejected order also gets a protected lifecycle event.

## Send-ahead and recovery

The host maps an order, then atomically reserves the current version in the
store immediately before starting its ASTM sender. The transaction rechecks
cache trust, version, withdrawal, expiry, requested state and analyzer targets.
It writes the internal `Sending` state and append-only history, with no app event.
Mapping rejections write `Rejected`, history and an event without claiming a
send occurred. App events use the existing immutable delivery outbox.

A reservation prevents another automatic send for that app order and machine,
including after a version change or restart. A link timeout, interrupted transfer
or disconnect before transfer confirmation becomes `Uncertain`. On restart,
only remaining `Sending` entries become `Uncertain` with an app event; confirmed `Sent` entries remain `Sent`.

LIS01 frame acknowledgments establish transfer completion, not application
acceptance of a work order. Successful transfers advance from `Sending` to `Sent` with one app event; they do not
invent an `Accepted` event. The lifecycle transition API supports explicit
machine acceptance/rejection evidence and operator reconciliation.

## Operator commands

```text
analyte --config analyte.toml --actor operator-name orders list
analyte --config analyte.toml --actor operator-name orders list --after 100 --machine analyzer-id
analyte --config analyte.toml --actor operator-name orders reconcile 101 --as accepted --reason "Verified in the machine work list"
```

Reconciliation accepts `accepted`, `rejected` or `cancelled` for an uncertain
entry. `cancelled` means cancellation was confirmed, not merely requested.
The local ID comes from `orders list`. Listing is bounded and audited;
reconciliation commits its state, history, operator audit and app event in one
transaction. Neither command sends or retries machine work.

## Deferred

HL7 QRY/QCK/DSR query handling and ORM/OML send-ahead are a separate round.
Live-machine validation, vendor-specific acceptance evidence and wider ASTM
query selectors are not claimed by these synthetic tests.

## Allowed lifecycle transitions

| From | Allowed next states |
| --- | --- |
| Requested | Sending, CancelRequested, Rejected |
| Sending (internal only) | Sent, Uncertain |
| Sent | Accepted, Rejected, CancelRequested, Uncertain |
| Accepted | Done, Rejected, CancelRequested, Uncertain |
| CancelRequested | CancelConfirmed, Done, Rejected, Uncertain |
| Uncertain | Accepted, Done, Rejected, CancelRequested, CancelConfirmed |
| Done, Rejected, CancelConfirmed | None |

All other transitions, including every attempt to return to Sending, are refused.
Contract conversion explicitly rejects Sending; only real transfer outcomes become
app events. The same reservation and completion rules apply to query answers.


## VIDAS Literal work lists

Evidence remains **documented**, from the existing VIDAS research notes (BCI A
sections 3-5 and PC sections 10-24 through 10-26), not machine recordings.
Enable both `[vidas] download = true` and `[orders] send_ahead = true`, with
`patient_namespace` naming the accepted patient assigning system. Use the same
explicit outbound test table, with exactly one 1-4 byte assay code in each
`machine_components` entry. Omit `[orders.astm]`. The priority table admits only
explicitly listed app priorities; Literal has no priority tag to transmit.

The shared candidate/mapping/reservation path maps every assay before reserving
one app order as Sending. VIDAS transmits one request per assay through its
existing download dialog. Only completion of every request produces one Sent
event (`VIDAS-transfer-confirmed`), never Accepted. A failed assay stops the
entire reserved order as Uncertain, including any already transferred assays;
it is never automatically retried. Restart recovery uses the shared Sending
recovery path; Sent stays Sent.

Missing patient, mismatched patient namespace, and patient/sample IDs exceeding
17 bytes or the installed sample limit respectively are rejected before any
transmission. IDs are copied exactly. Literal has no specimen-type tag: every
supplied specimen type is rejected as `unmapped-specimen-type`, rather than
being silently lost or placed in `so` (origin) or `si` (empty structural marker).
The request includes only `mt`, `pi`, `si`, `ci` and `rt`; no demographics,
position, dilution or other context is invented.

Uncertain transfer codes distinguish acknowledgment/interbyte/session deadlines,
disconnection, shutdown cancellation, write/input failures, framing/suffix
failures and refusal/unexpected input. Raw incoming bytes are still journaled
before the dialog receives them. Patient identifiers occur only in protected
order evidence and wire data, not ordinary logs.

`tests/orders/vidas.rs` drives the program with a temporary store and the
reference receiver: multi-assay completion, timeout, mid-send disconnect,
restart during Sending, Sent surviving restart, no replay, unmapped test and
specimen type, wrong/missing patient context, over-long IDs and cancellation
before sending. Autolyser insertion follows the same shared lifecycle below.


## Autolyser internal work lists

Evidence remains **documented** (Rev18 chapter 23, pages 175-179; synthetic
local peers, no hardware qualification). The host initiates R polling and
work-list insertion through the existing vendor dialog. A completed, stored
poll response opens the next idle interval for an insertion. An unanswered
initial poll never reserves an app order. Poll-custody guards remain separate
from order reservations: uncertain R cursor movement still blocks a new R
connection until reconciled; insertion recovery never advances that cursor.

Enable `[autolyser] download = true` and `[orders] send_ahead = true`, with
required `patient_namespace` and explicit `clone = true` or `false`. Test
mappings contain one exact four-byte analysis code, including any padding.
Priority mappings explicitly select `T` (routine list) or `R` (STAT list).
Supply specimen-medium mappings separately, for example:

```toml
specimen_types = [{app={namespace="synthetic",value="SERUM"},machine="S"}]
```

Only `S` and `U` are supported by this internal format. Missing specimen type
or an unmapped namespace/value is rejected, never defaulted. Patient values
must match the configured namespace, use printable ASCII and fit fifteen bytes;
the documented fixed-width space padding is applied without truncation. The
original ID is retained in protected order evidence. This format has one
patient-code field and no separate sample-ID field; the bridge never substitutes
the app sample ID for a missing patient. Incoming results still have unresolved
specimen/run identity, as before.

An order must supply its cup slot, numeric 1-99; `00` is not silently used to
ask the machine to autoassign. The shared position type retains kind, carrier
and exact slot in protected order evidence; only the addressed slot is encoded
by this protocol. ASTM uses the same position type, with independent explicit destinations.

Every mapping is complete before `reserve_send` commits Sending, with no app
event. The host sends STX, waits for ACK and then sends the fixed-width work
list plus checksum and EOT. Only a complete, durably stored `Y` plus position
reply becomes Sent, with one event retaining the exact binary position in its
source code. It never becomes Accepted automatically. A partial reply, deadline,
disconnect, bad ACK, negative reply or unknown reply becomes Uncertain with a
specific code and closes the insertion connection for reconciliation. Park
finalizes the active order in the same run; pending store/interpretation effects
are consumed before Close. Restart recovers only Sending and never replays.

`tests/orders/autolyser.rs` exercises both clone settings on the wire, confirmed
and partial replies, same-round Park, timeout, disconnect, restart, no replay,
unmapped codes/medium, missing/invalid cup slot, wrong/missing patient context,
over-long patient ID and cancellation before insertion. VIDAS also has an
explicit mid-send Park test asserting Uncertain and one app event before any
restart. Both suites use the program, temporary storage and reference receiver.


## ASTM optional order context (LIS02 section 8)

Send-ahead and query replies use the same outbound mapper. `[orders.astm]`
may name `position` in O field 4 (instrument specimen ID, section 8.4.4), with
an explicit component, and `specimen_type` in O field 16 component 1 (specimen
descriptor/type, section 8.4.16.1). The position slot keeps its exact spelling,
including leading zeros. Kind and carrier remain protected order evidence;
this selector sends only the slot, never substitutes a sample ID or another
position component.

The optional `[orders] specimen_types` table translates exact app namespace/value
pairs to machine text. The specimen destination cannot point to specimen source
(component 2), a user field or another record. Invalid/ambiguous tables and wire
delimiter injection are rejected at configuration validation.

No destination means no wire value: unmapped app specimen context does not
trigger a guessed code or an extra destination. A configured destination requires
its app context; missing position/specimen type is rejected specifically. With
a specimen destination, an unknown namespace/value produces
`unmapped-specimen-type` before reservation. A query mapping failure returns
only the configured query-error reply, never a partial order or no-orders answer;
the rejected order gets its app event through the shared lifecycle outbox.

For example, append these selectors inside an existing `[orders.astm]` table:

```toml
position = {group="order",record="O",field=4,component=2,repetition=1}
specimen_type = {group="order",record="O",field=16,component=1,repetition=1}
```

Add translations inside `[orders]`, separately from those selectors:

```toml
specimen_types = [{app={namespace="synthetic",value="SERUM"},machine="SER"}]
```

LIS02 section 8 tests in the mapper and `tests/orders/cases.rs` assert exact
O-record and complete H/P/O/L payload bytes for both send-ahead and query
answers, including configured context, absent destinations, unmapped and missing
specimen context. Existing minimal O records stay byte-for-byte unchanged when
these destinations are absent. Evidence remains documented, not machine-proven.


## Vendor end-to-end coverage

The required vendor regression matrix is covered below. These tests run the
program, a temporary protected store and the reference receiver. Shared assertions check
exact wire data, no app event for `Sending`, and one delivered event with the
specific final state/code. Startup validation rejects absent namespace settings;
missing or mismatched patient context is rejected at the order boundary.

| Required case | VIDAS | Autolyser |
| --- | --- | --- |
| Normal send; Sent survives restart | [All assays](tests/orders/vidas.rs#L84) | [Both clone values](tests/orders/autolyser.rs#L108) |
| Timeout | [Covered](tests/orders/vidas.rs#L131) | [Covered](tests/orders/autolyser.rs#L163); [partial reply](tests/orders/autolyser.rs#L182) |
| Disconnect mid-send | [Covered](tests/orders/vidas.rs#L155) | [Covered](tests/orders/autolyser.rs#L221) |
| Restart during Sending; no replay | [Covered](tests/orders/vidas.rs#L175) | [Covered](tests/orders/autolyser.rs#L241) |
| Unmapped test | [Covered](tests/orders/vidas.rs#L219) | [Covered](tests/orders/autolyser.rs#L281) |
| Unmapped specimen type | [Covered](tests/orders/vidas.rs#L228) | [Covered](tests/orders/autolyser.rs#L291) |
| Missing required position | Not applicable: Literal has no position field | [Covered](tests/orders/autolyser.rs#L307) |
| Wrong patient namespace | [Covered](tests/orders/vidas.rs#L234) | [Covered](tests/orders/autolyser.rs#L322) |
| Missing namespace configuration | [Covered](tests/orders/vidas.rs#L295) | [Covered](tests/orders/autolyser.rs#L363) |
| Missing patient | [Covered](tests/orders/vidas.rs#L240) | [Covered](tests/orders/autolyser.rs#L328) |
| Over-long ID | [Sample](tests/orders/vidas.rs#L246); [patient](tests/orders/vidas.rs#L252) | [Patient](tests/orders/autolyser.rs#L334) |
| Cancelled before send | [Covered](tests/orders/vidas.rs#L258) | [Covered](tests/orders/autolyser.rs#L340) |
| Park mid-send: same-round Uncertain and event | [Covered](tests/orders/vidas.rs#L326) | [Covered](tests/orders/autolyser.rs#L201) |
| Unanswered result poll cannot send an order | Not applicable: Literal does not poll for results | [Covered](tests/orders/autolyser.rs#L146) |

The suites contain 13 VIDAS and 18 Autolyser tests. They also cover Autolyser's
invalid cup slots, missing specimen medium and negative/unknown insertion replies.
Evidence remains **documented**; the table is synthetic regression coverage,
not qualification against hardware or an official simulator.


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
