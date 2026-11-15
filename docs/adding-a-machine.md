# Adding a machine

You do not need to know Rust to prepare a machine definition. You do need
confirmed interface settings and evidence that identifiers, values and orders
survive the journey unchanged. Analyte moves data; the lab app makes clinical
decisions. Unknown information is a reason to stop and investigate.

Read the [plan](plan.md), [config reference](../crates/config/README.md) and
[config-first decision](adr/0002-config-first.md) alongside this guide.

## Choose the stack

A stack is the connection, wire framing, conversation and message layout.
Confirm the exact model, firmware, installed interface option and protocol
edition; a family name or an Ethernet socket does not establish a protocol.

| Machine interface | Read first | Starting point |
| --- | --- | --- |
| ASTM LIS01/LIS02 | [ASTM scope](protocols/astm/README.md) | [ASTM example](../crates/config/examples/astm.toml) |
| HL7 v2 legacy over MLLP | [HL7 scope](protocols/hl7/README.md) | [HL7 example](../crates/config/examples/hl7.toml) |
| IHE LAW Analyzer Manager | [LAW scope and remaining gaps](protocols/law/README.md) | [LAW example](../crates/config/examples/law.toml) plus [query/work settings](../crates/config/examples/law_orders.toml); persistent TCP client/server or independent directional links |
| POCT01-A2 | [POCT scope](protocols/poct/README.md) | [POCT example](../crates/config/examples/poct.toml), Observation Reviewer role |
| VIDAS Literal or Autolyser | [Protocol index](protocols/README.md), then vendor interface rules | [VIDAS](../crates/config/examples/vidas_literal.toml) or [Autolyser](../crates/config/examples/autolyser.toml) |

There are no vendor-specific pages in that protocol index yet. Use the
[config reference](../crates/config/README.md) and [order guide](../crates/analyte/ORDERS.md)
for their implemented policies. LAW supports two explicit topologies. Use
`law.connection_mode = "persistent"` for one TCP client/server link carrying all
three transactions. For the B.6 directional topology, select
`transport.kind = "tcp_server"`, set the site endpoint to Analyte's LAB-27/LAB-29 listening address,
and set these fields inside the existing `[law]` table:

```toml
connection_mode = "independent_per_transaction"
outbound_endpoint = "192.0.2.11:7001" # replace with the confirmed analyzer LAB-28 listener
```

The analyzer initiates queries/results; the host opens a separate outbound
connection for each LAB-28 work/cancel transaction. The analyzer may close either
socket between transactions. Query follow-up work survives closure of the query
socket. A failed outbound attempt remains reserved/uncertain and is never retried
by reconnect or restart; use operator reconciliation when its outcome is unknown.
Confirm both listening addresses and the analyzer's topology with its manual.

Declare peer options explicitly. Queries support exact specimen/container ID
(SampleId) and all pending work for this analyzer; append the query/work settings
example to enable queries and follow-ups, and select `orders.send_ahead = true`
for unsolicited work. Both modes use the same restore-pause and reservation rules.

Isolate/parent and rack/tray queries remain unsupported and receive an error,
never no-work. Originating-work checks have local tests; remaining enhanced
workflows and a recorded Gazelle run remain open in the LAW checklist. Evidence
is documented, without real-machine qualification. A LAW definition uses the
LAW profile rather than a legacy HL7 layout. A new format needs review
([standards-first decision](adr/0007-standards-first.md)).

## Gather confirmed settings

Keep a short worksheet with each answer, its source/version, who confirmed it
and what remains unverified. Ask the vendor these questions for the installed
machine, and ask the lab app team about the matching app identifiers.

| Ask about | Questions to resolve |
| --- | --- |
| Connection | Serial connector/pinout, baud, bits, parity, stop bits and flow control? For TCP, which side connects, numeric address and port? For files, how are complete files handed over? |
| Tube identity | Scanned, host-supplied or typed? Which exact field carries it back? Length, case and leading zeros preserved? Is position separate from identity? |
| Tests and units | Actual enabled host codes and their assigning systems? Exact unit spelling? Numeric, qualitative or either? Are dilution factors already applied? |
| Result identity | What distinguishes retransmission, correction and a fresh run? Which order ID, namespace, sub-ID, run fields and correction codes provide evidence? |
| Orders | Query, polling or work lists sent ahead? Exact query filters, reply layout, priorities, specimen types, patient namespace and required positions? What proves acceptance or cancellation? |
| Timing and limits | Reply, acknowledgment and interbyte deadlines? Frame/message limits, character encoding and escape rules? What happens after timeout or reconnect? |

Commonly unverified: flow control, TCP roles, actual sample field, installed
software limits, host test tables, dilution state and query deadlines. Never
borrow these from another model or guess them from a successful connection.
Never manufacture a sample ID, unit, run key, patient namespace or cup position.
Record unresolved items explicitly and withhold activation until resolved.

## Keep three config parts

| Part | Owns |
| --- | --- |
| Shipped definition | Protocol, mappings, named departures and evidence |
| Site settings | Device endpoint and app destination, with credential names only |
| Local overrides | Explicit replacement sections for this installation |

All layers use TOML, a text format with named sections. Unknown keys fail.
Sections and lists replace in full; overrides do not merge individual list
entries and cannot change evidence. See the [schema](../crates/config/src/schema.rs)
and [validation rules](../crates/config/src/validation.rs) before editing.

### Complete synthetic example

Save the following three files together. This is the repository's synthetic
ASTM teaching definition and site example with a serial endpoint. It describes
no vendor machine and is not qualified for clinical use. Its empty run evidence
is intentional: it cannot establish a fresh run. These numbers are teaching
settings, not installation recommendations.

`machine.toml`:

```toml
# Synthetic teaching configuration. DOCUMENTED ONLY; not qualified for clinical use.
schema_version = 1
name = "generic-synthetic"
deviations = []
[transport]
kind = "serial"
operation_timeout_ms = 10000
[transport.serial]
baud = 9600
data_bits = 8
parity = "none"
stop_bits = 1
flow_control = "none"
[framing]
kind = "astm_lis01"
limit = 64000
checksum_hex = "strict"
[dialog]
kind = "astm_link"
receive_timeout_ms = 30000
send_timeout_ms = 15000
contention_priority = "instrument"
[syntax]
kind = "astm_lis02"
encoding = "ascii"

bytes = 1048576
records = 4096
record_bytes = 65536
fields = 128
repetitions = 128
components = 128
[mappings]
tests = [{machine_code="SYN", machine_system="synthetic", app_code="DEMO", app_system="demo"}]
units = [{original="mg/L", ucum="mg/L"}]
qc = []
[mappings.sample]
source = "barcode"
[mappings.sample.field]
group = "order"
record = "O"
field = 3
component = 1
repetition = 1
[mappings.identity]
order_id = {group="order",record="O",field=3,component=1,repetition=1}
order_namespace = {fixed="synthetic-orders"}
sub_id = "none"
run = []
correction_status_codes = ["C"]
[evidence]
level = "documented"
[[evidence.capabilities]]
model = "synthetic"
firmware = "unknown"
driver = "0.1.0"
config_digest = "untested"
transport = "serial"
dialogs = []
failure_cases = []
platform = "untested"
evidence_files = []

```

`site.toml` (replace the port and destination only with confirmed site values):

```toml
# Synthetic installation; credential names only.
schema_version = 1
endpoint = "COM9"
[destination]
name = "demo"
base_url = "https://lab.example.invalid"
api_key_name = "demo-api"
signing_key_name = "demo-signing"

```

`local.toml`:

```toml
schema_version = 1
```

### Map received results

In `mappings.tests`, `machine_code` and `machine_system` select an exact source
code; `app_code` and `app_system` name its agreed app equivalent. Do not assume
that an assay name is a host code or assign a LOINC code without lab agreement.
`mappings.units` translates exact `original` spellings to `ucum`; it never
changes numbers or supplies absent units. Values retain their original text.
Unknown tests and mapping failures stay parked with reasons.

Selectors use one-based `field`, `component` and `repetition` positions inside
the declared `group` and `record`. Confirm them from source and recordings.
`mappings.sample.source` describes how the machine got the ID; the field
selector determines where Analyte reads it. Sample IDs remain strings.

`mappings.identity.order_id`, `order_namespace`, `sub_id`, `run` and
`correction_status_codes` declare evidence, not a recipe for guessing identity.
A namespace names the assigning system: use a confirmed fixed system or a
source field. `sub_id = "none"` deliberately declares no sub-ID; `run = []`
provides no run evidence. Missing evidence remains unresolved; changed content
under one trusted identity is a conflict, not an inferred rerun. Receipt,
delivery and result/revision identities stay separate. See [mapping behavior](../crates/mapping/README.md).

For ASTM only, an optional section can split one R.3 component into exact parts:

```toml
[mappings.component_parts]
component = 4
separator = "/"
count = 2
code = 1
dilution = 2
```

This synthetic extension expects exactly two parts, including empty ones.
The code part replaces normal test-code lookup. Dilution text is retained;
ratios are not interpreted. Application state stays unknown unless `applied`
selects a source `field` with distinct exact `yes` and `no` codes. Do not infer
that state from the factor. Wrong part counts park the receipt.

Each test entry may add `value_kind = "numeric"`, `"coded"` or `"either"`.
Numeric requires a parsed number; coded requires nonempty qualitative text or
a coded value, not a plain number. Either is the default. A mismatch parks the
original value; it does not make a clinical decision. Vendor/POCT definitions
use `[codes]` test/unit tables rather than field-position `[mappings]`.

### Enable orders separately

Receiving results does not enable queries or work lists. Append the complete
[synthetic ASTM orders section](../crates/config/examples/orders.toml) only
when that behavior is confirmed. `[orders]` has independent outbound `tests`
and `priorities`; it does not reverse the result table automatically.
Each test maps an exact app `namespace`/`value` to `machine_components`,
including significant empty components. `send_ahead` explicitly permits work
lists. `[orders.query]` requires `deadline_ms` and `when_untrusted`; unavailable,
stale or late lookup must produce error or silence, never "no orders".

For ASTM, `[orders.astm]` names the specimen, test, priority and action
locations. Optional `position` sends the supplied slot in O field 4;
`specimen_type` is restricted to O field 16 component 1. `[orders] specimen_types`
translates exact app namespace/value pairs. A configured destination requires
that context: no default slot or specimen type is invented.

For legacy HL7 queries, start from [hl7_orders.toml](../crates/config/examples/hl7_orders.toml).
It declares QRD filters and DSP layout, with an explicit ACK deadline; it requires
`when_untrusted = "error"`. Send-ahead is a separate
[hl7_send.toml](../crates/config/examples/hl7_send.toml) interface using
`placer_namespace` and `[orders.hl7_send]`. ORM and OML have different specimen
and position support; do not treat them as LAW.

Vendor work lists require `download = true` in their vendor section as well as
`send_ahead = true`. `patient_namespace` must match the app patient's assigning
system; Autolyser also needs explicit `clone`, specimen medium and cup slot.
VIDAS cannot transmit specimen type. The current ASTM order layout has no
patient field and rejects `patient_namespace`. Follow the
[order guide](../crates/analyte/ORDERS.md) for exact per-stack restrictions.
Sent, accepted, done and cancelled are different outcomes. A lost reply or
restart never authorizes an automatic resend to a machine.

### Declare departures, do not program around them

Keep `deviations = []` for the strict example. A supported compatibility choice
needs a unique `name`, the exact affected `key` and a `reason` tied to evidence.
For example a changed ASTM frame limit requires a declaration for `framing.limit`.
A declaration does not implement arbitrary behavior. The service can reject
choices the config library accepts (it currently requires strict ASTM checksum
hex). If the schema cannot express a quirk, stop and ask maintainers. A reviewed
code change belongs in the owning layer; do not weaken a shared standard or
invent a config key. A new subsystem, protocol family, format, dependency or
unplanned ADR requires agreement before building it.

## Validate before connecting

The command checks a **program file**, not `machine.toml` directly. Start from
[the program example](../crates/analyte/examples/analyte.toml), following the
[service guide](../crates/analyte/README.md), and point its machine entry at the
three files above. Paths are relative to the program file. Keep credentials
in protected host storage, never in TOML. Then run:

```text
analyte check-config analyte.toml
```

Fix every reported error. A pass checks structure, compatible settings and
budgets; it does not certify a machine or the clinical meaning of mappings.
Only validated config can activate, between conversations. Receipts retain
the effective config and digest; activation does not change queued deliveries
or resend orders. Reprocessing creates another interpretation of retained bytes.

## Capture, de-identify and replay

Follow [ADR 0009](adr/0009-recording-format.md) and the
[simulator guide](../crates/sim/README.md). Capture both directions and timing
from a machine trace, lab-PC sniffer or hardware tap. Keep original captures
with patient data protected, outside Git and ordinary logs.

Make a reviewed de-identified copy or an original synthetic recording. Inspect
decoded bytes and metadata: base64 is not de-identification. Replace identifiers
and patient details consistently, preserving lengths/layout where needed;
recalculate checksums affected by replacements. Document changes. Preserve
intentional faults, directions and split/merged read boundaries.

Store one UTF-8 `.rec.jsonl` with LF endings: a version-1 header naming stack,
machine/software, evidence origin, source and de-identification statement,
then timed direction/base64-byte events. Use the bounded reader/writers
specified in the simulator guide. Variable host control IDs/times need named,
fixed byte-location exceptions, never broad masks.

`analyte-sim` is a library, not an executable. Ask a contributor to add a
source-named replay test following the existing recording tests in
[delivery.rs](../crates/analyte/tests/delivery.rs). The test uses a temporary
store, loopback machine peer and reference receiver, with no live credentials.
It calls `analyte_sim::replay`, compares host bytes and checks durable records
and delivered results. Deadlines come from the protocol/config, not capture gaps.
Replay does not reproduce recorded waiting times; serial break events are not
supported by the current replay implementation. Run the focused test as described
in the simulator guide. Adding test code also requires the workspace checks.

Cover the applicable failure cases, not just one successful upload:

| Area | Cases and required observation |
| --- | --- |
| Wire | Bad checksum, split/merged reads, encoding/escapes, oversize input, timeout and disconnect: retain evidence, refuse visibly |
| Custody | Lost ACK, crash before/after ACK, restart after store before ACK, disk full: never ACK unstored data |
| Identity | Leading zeros/case, repeated result, correction, fresh run, same-ID conflict, missing evidence: retain identities or park unresolved |
| App delivery | Lost app reply after storage, outage and retry: same delivery ID and bytes |
| Orders | Incomplete/stale feed, late lookup, unmapped code/context, cancellation race, lost command reply and restart: no false empty answer or automatic resend |
| Operations | Clock jumps, file handoff where used, backup/restore reconciliation and interrupted shutdown |

Before go-live, the lab must validate the full route through to the report the
clinician sees, including values, units, flags, sample identity and corrections.

## State the evidence honestly

[ADR 0005](adr/0005-evidence-levels.md) applies separately to each driver and config.

| Level | Required basis |
| --- | --- |
| documented | Built from documents only; synthetic tests do not raise it |
| simulated | Tested with an official simulator or vendor sample traces |
| proven | Tested against the real machine, with reviewed de-identified recorded conversations as fixtures |

A recording never raises a level by itself. The capability list must name model,
firmware, driver version, tested config digest, transport, dialogs, failure cases,
platform and evidence files. A result-upload recording says nothing about orders
or cancellation. Review provenance and actual exercised scope before changing
`evidence.level`; file references are declarations, not verified qualification.

## Contribute the definition

Contribute a machine as one `machines/<vendor>/<model>/` folder with
`machine.toml`, `README.md` and `recordings/`, as
[ADR 0012](adr/0012-uniform-layout-and-naming.md) describes. Keep site
addresses, credentials, patient data and original captures out of the PR.

Include the definition, supported model/software and capabilities, honest evidence
level, open questions, source sections in your own words, reviewed recordings,
replay tests/results and named departures with reasons. Explain expected sample,
result and order identity behavior and unsupported options. Do not paste manual
text, tables or examples or commit standards/vendor documents. Use original
synthetic or reviewed de-identified fixtures only. Follow
[CONTRIBUTING.md](../CONTRIBUTING.md): signed-off Conventional Commits and AI
assistance disclosure where applicable. A config PR is not permission to go live.

## Final checklist

- [ ] Exact model/software, interface and supported scope confirmed.
- [ ] Connection, encoding, limits and deadlines verified; unknowns recorded.
- [ ] Tube ID source/field, test codes, units and dilution confirmed by the lab.
- [ ] Retransmission, correction and rerun evidence reviewed; no invented identity.
- [ ] Three config layers separated; no secrets; all keys supported.
- [ ] Outbound namespaces, context and failure policy explicitly agreed if enabled.
- [ ] Named departures justified; unsupported quirks referred to maintainers.
- [ ] Program passes `analyte check-config`.
- [ ] Recordings reviewed for privacy and replay tests cover applicable failures.
- [ ] Evidence level matches only the exercised capabilities.
- [ ] PR contains sources, evidence and verification results without protected data.
- [ ] Lab validation through the final report completed before activation.
