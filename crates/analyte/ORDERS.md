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

Additional machine-specific order profiles and live qualification are deferred.
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


## Durable command items

Every new machine-order reservation retains items atomically in schema v5.
ASTM and vendor records cover all tests in one item; HL7 send-ahead will reserve
one item per test. Store-owned placer identities are `<machine-order-id>-<index>`;
the HL7 configuration must supply their namespace. App order IDs belong in
ORC-4, independently of these identities. Neither identity may be truncated.

Application evidence is recorded per item. All Accepted, all Rejected, all Sent
or all CancelConfirmed produce that aggregate state; mixed or missing answers
produce Uncertain with per-item reasons. Cancellation covers all items. Explicit
refusal restores the preceding order state with a CancelRefused event/history;
missing evidence remains Uncertain. Orders list shows item identities, test
coverage and reasons. Reconciliation stays per order, audited, and never resends.

Migrated v4 orders explicitly mark original test coverage unavailable, because
v4 did not retain it. Their existing reservations continue to prevent replay.
The store API, legacy HL7 queries, and ORM/OML send-ahead and cancellation are
implemented at documented evidence level.

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


## HL7 legacy display queries

Evidence remains **documented** (HL7 2.3.1 section 2.18 and 2.5.1 section 5.10).
The synthetic [HL7 orders example](../config/examples/hl7_orders.toml) explicitly
selects which QRD field carries the sample, its exact trailing components, and
all supported nonempty selectors. Config validation checks a synthetic query
against the selected edition. No QRF filter, continuation, range, wildcard or
undeclared selector is ignored; unsupported queries are retained for review.
The query interface uses original acknowledgment mode. Send-ahead is a separate,
explicit capability.

A complete QRY MLLP block, with driver/config provenance, is durable before any
QCK. Query time starts with the first block byte, including split reads, raw
storage and lookup. Trusted empty yields QAK NF; unavailable, stale, late or
untrusted answers yield AE. HL7 requires `when_untrusted = "error"`. Lookup uses
the same cache freshness and requested/target rules as ASTM and vendor orders.

`orders.hl7.lines` gives the exact DSP-3 layout for each requested test. The
ordered lines repeat per test and per app order, without an implicit header or
vendor positions. Supported sources are sample_id, order_id, test_code,
priority, position (the supplied slot), specimen_type, patient_id and explicit
literal text (including blank padding). Test/priority/specimen translations
are explicit; patient_id also needs a matching patient_namespace. Absent optional
context produces an empty line, never a substitute. Plain ASCII DSP values are
bounded to 300 bytes and cannot inject HL7 delimiters. Unmapped or unrepresentable
orders are rejected with an app event; no partial display is transmitted.

The shared reservation path rechecks every candidate before QCK/DSR issuance.
Each app order has one durable item covering all its tests, still Sending with
no app event. QCK and DSR have distinct control IDs; the DSR is rendered before
reservation so a quantity/size failure cannot transmit partial work. ACK^Q03 AA
for the actual DSR ID becomes Sent (one app event), never Accepted. AE, AR,
invalid/wrong-edition ACK, lost ACK and disconnect become Uncertain with distinct
codes. Incoming ACK bytes are durable before the lifecycle changes. The
acknowledgment_ms timer starts at DSR issuance. Neither query replay nor restart
can reserve the same order again. In-flight reservation keys survive a cancelled
host wait so connection cleanup can find any reservation that committed.

Program tests in `tests/orders/hl7.rs` use the temporary store and reference
receiver for both editions, exact DSP text, store-before-reply, no event while
Sending, trusted empty/untrusted/stale answers, unknown filters, missed query
deadlines, AA/AE/AR/lost/wrong ACKs, disconnect and ordinary result delivery with
queries enabled. Synthetic peers do not raise the evidence level.

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


## HL7 send-ahead and cancellation

The synthetic [send-ahead example](../config/examples/hl7_send.toml) opts into
`send_ahead`, requires `placer_namespace`, and declares MSH applications,
processing mode, specimen placement and the application response deadline.
Edition 2.3.1 selects ORM/O01 and ORR/O02; 2.5.1 selects non-LAW OML/O21 and
ORL/O22. Outbound tests have three explicit CE components (code, text, system).
An absent patient mapping emits no PID; an identifier-only mapping transmits
only that identifier and namespace, never a guessed name. ORM allows an absent
specimen type; OML requires mapped SPM-4 and its coding-system version when the
coding system requires one. Optional ORM slot placement is explicitly OBR-18
or OBR-19, distinct from the configured sample field. OML position output is
not implemented and cannot be configured. Other optional, unmapped data is empty.

The shared store reservation transaction creates one item per test and assigns
EI.1 `<machine-order-id>-<index>` with the configured EI.2. ORC-4 carries the app
order ID. A pure preflight renders the complete message with these actual IDs
before Sending commits. Oversized group/placer identities and unrepresentable
fields produce specific Rejected evidence without a committed Sending state,
truncation or wire output. All reservation variants call the shared restore
pause helper inside the same transaction; restored stores cannot issue work
until audited reconciliation resumes them.

Incoming ORR/ORL blocks are stored before interpreting their correlated item
outcomes. Explicit OK/UA evidence and missing/unknown responses pass through
`finish_order_items`; MSA AA alone never establishes order acceptance. All OK
becomes Accepted, all UA becomes Rejected, and mixed or uncertain outcomes
become Uncertain. Reasons remain per item and in protected event/history.
Timeout, disconnect and restart never repeat an original command.

App cancellation uses a fresh, trusted feed version and a transaction that
rechecks the request before CancelRequested and its event commit. Cancel CA
uses the retained item identities without rebuilding sample/test/patient data.
CR for all items produces CancelConfirmed; UC restores the preceding order
state with a CancelRefused event and per-item reasons. Missing evidence produces
Uncertain. Each app cancellation version is admitted once, including after UC
or restart; a stable cancelled feed entry cannot trigger a polling retry.
An uncertain order still needs audited operator reconciliation.

Synthetic end-to-end cases cover both editions, accepted/rejected/mixed items,
no reply, disconnect, cancel accepted/refused through the reference app feed,
restart while Sending, no resend, actual placer-ID overflow, overlong ORC-4,
and paused restore admission. They do not qualify a physical analyzer.
