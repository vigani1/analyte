# Adding a machine

You do not need to know Rust to prepare a machine definition. You do need
confirmed interface settings and evidence that identifiers, values and orders
survive the journey unchanged. Analyte moves data; the lab app makes clinical
decisions. Unknown information is a reason to stop and investigate.

Read the [plan](../plan.md), [config reference](../../crates/core/config/README.md) and
[config-first decision](../adr/0002-config-first.md) alongside this guide.

For an interface requiring new protocol code, follow
[adding a vendor protocol](adding-a-vendor-protocol.md).

## In short

1. **Confirm the protocol** the machine really speaks, from its interface
   manual and its host settings screen ([choose the stack](#choose-the-stack)).
2. **Copy the protocol's example** into `machines/<vendor>/<model>/machine.toml`
   and fill in only confirmed settings: connection, deadlines, size limits,
   test codes, units and where the sample ID is
   ([gather confirmed settings](#gather-confirmed-settings)). Lab addresses
   and keys stay in each lab's site file, never in this folder. The folder
   key `vendor/model` becomes the model id in every delivery's
   `source.model`, so choose it once
   ([ADR 0033](../adr/0033-machine-model-and-published-schema.md)).
3. **Write its `README.md`** from the [template](../../machines/README.md#readme-template):
   what is known, the sources in your own words, what is still open and the
   evidence level.
4. **Add recordings** of its conversations in `recordings/`, synthetic or
   de-identified ([capture, de-identify and replay](#capture-de-identify-and-replay)).
5. **Run `cargo run -p analyte-publish`, then `cargo test --workspace`.** The
   first adds the model to `docs/contract/machines.json`; the tests find every
   `machine.toml`, load it the way a lab PC would, check the README's
   evidence level, replay every recording through the running service and
   check the catalog is current. No test code is needed.
6. **Open a pull request** with the folder ([contribute the definition](#contribute-the-definition)).

A machine without real recordings stays at `documented`, however complete its
settings are ([state the evidence honestly](#state-the-evidence-honestly)).

## Choose the stack

A stack is the connection, wire framing, conversation and message layout.
Confirm the exact model, firmware, installed interface option and protocol
edition; a family name or an Ethernet socket does not establish a protocol.

| Machine interface | Read first | Starting point |
| --- | --- | --- |
| ASTM LIS01/LIS02 | [ASTM scope](../protocols/astm/README.md) | [ASTM example](../../crates/core/config/examples/astm.toml) |
| HL7 v2 legacy over MLLP | [HL7 scope](../protocols/hl7/README.md) | [HL7 example](../../crates/core/config/examples/hl7.toml) |
| IHE LAW Analyzer Manager | [LAW scope and remaining gaps](../protocols/law/README.md) | [LAW example](../../crates/core/config/examples/law.toml) plus [query/work settings](../../crates/core/config/examples/law_orders.toml); persistent TCP client/server or independent directional links |
| POCT01-A2 | [POCT scope](../protocols/poct/README.md) | [POCT example](../../crates/core/config/examples/poct.toml), Observation Reviewer role |
| VIDAS Literal or Autolyser | [Protocol index](../protocols/README.md), then vendor interface rules | [VIDAS](../../crates/core/config/examples/vidas_literal.toml) or [Autolyser](../../crates/core/config/examples/autolyser.toml) |

There are no vendor-specific pages in that protocol index yet. Use the
[config reference](../../crates/core/config/README.md) and [order guide](../../crates/analyte/ORDERS.md)
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
([standards-first decision](../adr/0007-standards-first.md)).

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
| Shipped definition | Protocol, mappings, named departures and evidence; for serial, the manual's usual cable settings as a suggestion; the choices the lab makes, each with its allowed values and the manual's suggestion |
| Site settings | Device endpoint, app destination with credential names only, the serial cable settings the lab chose and the lab's value for each choice |
| Local overrides | Explicit replacement sections for this installation |

All layers use TOML, a text format with named sections. Unknown keys fail.
Sections and lists replace in full; overrides do not merge individual list
entries and cannot change evidence.

A serial definition's `[transport.serial]` is the **suggestion** from the
manual, not the lab's setting: each lab chooses the values its analyzer's LIS
screen shows in Setup step 2 (or with `analyte machines set --baud ...`), and
Save keeps them in the site file's `[serial]`
([ADR 0031](../adr/0031-lab-chooses-serial-cable-settings.md)). Write the
values the manual states; leave out a value it does not state (for example an
unknown flow control) rather than guessing. Analyte then refuses to run the
machine until the lab chooses it. Only the standard values are accepted: baud
rates 300 to 115200 from the list in
[connect](../operations/connect.md#serial-cable-settings), 5 to 8 data bits,
parity none/odd/even, 1 or 2 stop bits, flow control none/hardware/software. See the [schema](../../crates/core/config/src/schema/mod.rs)
and [validation rules](../../crates/core/config/src/validation/mod.rs) before editing.

### Complete synthetic example

Save the following three files together. This is the repository's synthetic
ASTM teaching definition and site example with a serial endpoint. It describes
no vendor machine and is not qualified for clinical use. Its empty run evidence
is intentional: it cannot establish a fresh run. These numbers are teaching
settings, not installation recommendations.

`machine.toml`:

```toml
# Synthetic teaching configuration. DOCUMENTED ONLY; not qualified for clinical use.
schema_version = 2
protocol = "astm"
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
limit = 64000
checksum_hex = "strict"
[dialog]
receive_timeout_ms = 30000
send_timeout_ms = 15000
contention_priority = "instrument"
[syntax]
encoding = "ascii"

bytes = 1048576
records = 4096
record_bytes = 65536
fields = 128
repetitions = 128
components = 128
[mappings]
# The machine's own code system; codes pass through as sent (ADR 0029).
test_system = "synthetic"
test_component = "manufacturer"
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

`site.toml` (replace the port only with confirmed site values; the lab app is
set once in `lab-apps.toml`, see
[connect](../operations/connect.md#lab-apps-set-once-picked-by-each-machine)):

```toml
# Synthetic installation; no addresses or keys.
schema_version = 1
retention = 90 # recommended; requires the retention-digest host credential
endpoint = "COM9" # or usb_serial = "A10K3XYZ"; see operations/connect.md
lab_app = "demo"
```

`local.toml`:

```toml
schema_version = 1
```

### Store startup budget

`[store].startup_timeout_seconds` bounds opening, full integrity verification
and migration together. It defaults to 600 seconds; validation accepts 30 through
3600 seconds. Ordinary store requests retain their separate short deadlines.
Startup logs contain the active step,
migration versions and elapsed milliseconds, never stored payloads. A timeout
names its step. SQLite cancellation rolls back the active migration transaction;
earlier committed versions remain valid and the next startup resumes from them.
A commit already completed when cancellation arrives remains fully committed.

Set this program-level `[store]` setting in `analyte.toml`, not in the machine's
`site.toml`. Measure opening and upgrade startup on the deployment's storage;
choose a budget within the supported range that leaves headroom for store
growth, disk contention and future migrations.

### Map received results

Analyte does not translate or name tests: the lab app owns test names and
test lists ([ADR 0029](../adr/0029-lab-app-owns-test-names.md)). Each result
carries the machine's own code exactly as sent. `mappings.test_system` names
the machine's code system: the namespace of every code whose message names
none, and the only one its orders accept. Pick a stable name for the machine
model (for example `cobas-e411`) and tell the lab app makers; for HL7 and LAW
use the coding system the machine writes in OBX-3.3. For ASTM,
`test_component` says which R.3 component holds the code (LIS02 §5.6.1):
`manufacturer` (component 4, `^^^ALB`, what most analyzers send) or
`universal` (component 1). HL7, LAW and vendor formats have one fixed place
and refuse the setting. A result whose code is empty is parked; any other
code is delivered, and the lab app refuses codes it does not know.

`mappings.units` translates exact `original` spellings to `ucum`; it never
changes numbers or supplies absent units. Values retain their original text.
Mapping failures stay parked with reasons.

Selectors use one-based `field`, `component` and `repetition` positions inside
the declared `group` and `record`. Confirm them from source and recordings.
`mappings.sample.source` describes how the machine got the ID; the field
selector determines where Analyte reads it. Sample IDs remain strings.

### Declare a result identity recipe

Add `evidence = { manual = "title and revision", section = "section and table" }`
under `[mappings.identity]` for the installed model, firmware and mode. When
choices select the whole mapping, put the recipe in each applicable mapping.
Choose `order_id`, its `order_namespace` and `sub_id` from their source fields.
Use `sub_id = "none"` when the profile has no sub-ID. The recipe also uses the
configured sample ID and the machine's namespaced test code.

List the required `run` selectors in order. For example, when a manual
establishes ASTM R.13 as stable completion evidence, its selector is
`{ group = "observation", record = "R", field = 13, component = 1, repetition = 1 }`.
Confirm that two measurements of the same sample and test have different keys,
that resends repeat the key, and that corrections keep it. Use this recipe
for sources with one measurement per observation record.

Declare the manual's exact `correction_status_codes` and `rerun_status_codes`.
A rerun code overrides the generic status meaning and needs source evidence
that distinguishes the run from earlier runs. Select the raw marker in `run`
only if its documented stability and distinctness make it part of the key.
Check resends, multiple reruns and missing fields with synthetic tests and a
de-identified recording before qualification.

For VIDAS use `[codes.vidas_identity]` with `sample_namespace` and the same
`evidence` citation. It selects the source sample, assay, completion date and
completion time. Verify the date and time have sufficient precision for the
installed profile. See [ADR 0035](../adr/0035-machine-result-identity-recipes.md).

For ASTM only, an optional section can split one R.3 component into exact parts:

```toml
[mappings.component_parts]
separator = "/"
count = 2
code = 1
dilution = 2
```

It splits the `test_component` component. This synthetic extension expects
exactly two parts, including empty ones. The code part is the test code.
Dilution text is retained;
ratios are not interpreted. Application state stays unknown unless `applied`
selects a source `field` with distinct exact `yes` and `no` codes. Do not infer
that state from the factor. Wrong part counts park the receipt.

Optional `mappings.value_kinds` records what kind of value the machine sends
for one of its codes, for example `value_kinds = [{code = "ALB", kind =
"numeric"}]`; add `system` when the message names another code system than
`test_system`. Numeric requires a parsed number; coded requires nonempty
qualitative text or a coded value, not a plain number. A code without an
entry is not checked. A mismatch parks the original value; it does not make a
clinical decision. Vendor/POCT definitions use `[codes]` (`test_system` and
`units`) rather than field-position `[mappings]`.

### Enable orders separately

Receiving results does not enable queries or work lists. Append the complete
[synthetic ASTM orders section](../../crates/core/config/examples/orders.toml) only
when that behavior is confirmed. Order tests arrive from the lab app in the
machine's own codes: Analyte sends a test only when its namespace is exactly
`mappings.test_system` (or `codes.test_system`), writes its code as given, and
rejects any other test with a specific order event. Specimen types follow
the same rule ([ADR 0030](../adr/0030-lab-app-owns-specimen-codes.md)):
`[orders] specimen_system` names the machine's specimen code system, and the
lab app sends the machine's own specimen code in it. Pick a stable name (for
HL7 and LAW the coding system the machine expects in SPM-4.3, for example
`HL70487`) and tell the lab app makers. `[orders] priorities` stays an
explicit translation: it maps the contract's three priorities to the
machine's wire codes. `send_ahead` explicitly permits work
lists. `[orders.query]` requires `deadline_ms` and `when_untrusted`; unavailable,
stale or late lookup must produce error or silence, never "no orders".

For ASTM, `[orders.astm]` names the specimen, test, priority and action
locations; the `test` component is where each code goes (4 for `^^^CODE`),
and optional `test_components` keeps empty components after it (5 writes
`^^^CODE^`). Optional `position` sends the supplied slot in O field 4;
`specimen_type` is restricted to O field 16 component 1 and goes together
with `[orders] specimen_system`. A configured destination requires that
context: no default slot or specimen type is invented.

For legacy HL7 queries, start from [hl7_orders.toml](../../crates/core/config/examples/hl7_orders.toml).
It declares QRD filters and DSP layout, with an explicit ACK deadline; it requires
`when_untrusted = "error"`. Send-ahead is a separate
[hl7_send.toml](../../crates/core/config/examples/hl7_send.toml) interface using
`placer_namespace` and `[orders.hl7_send]`; it writes OBR-4 as the code, the
lab app's optional name and `test_system`, and editions with CWE also need
`test_version`. ORM and OML have different specimen and position support; do
not treat them as LAW.

Vendor work lists require `download = true` in their vendor section as well as
`send_ahead = true`. `patient_namespace` must match the app patient's assigning
system; Autolyser also needs explicit `clone`, `specimen_system` (orders
then send `S` or `U` in it) and cup slot. VIDAS cannot transmit specimen type
and refuses `specimen_system`. The current ASTM order layout has no
patient field and rejects `patient_namespace`. Follow the
[order guide](../../crates/analyte/ORDERS.md) for exact per-stack restrictions.
Sent, accepted, done and cancelled are different outcomes. A lost reply or
restart never authorizes an automatic resend to a machine.

### Declare the lab's choices

Some settings are the lab's to choose because the analyzer offers them on its
own screen (a host dialect, "send results only", VIDAS Literal or Alternate)
or because they follow the installed software version (the VIDAS sample ID
limit). Declare each as a choice instead of writing a second definition
([ADR 0032](../adr/0032-machine-definitions-declare-lab-choices.md)); Setup
step 2 then asks exactly these, and `analyte machines set --choice
NAME=VALUE` sets them. Everything else in the definition stays a fixed fact
of the model: where the sample ID sits, QC rules, correction codes, deadlines
and size limits are never choices.

```toml
# A mode the analyzer offers; the manual suggests one value.
[[choices]]
name = "protocol_mode"
kind = "mode"
label = "Protocol on the host link screen"
suggested = "literal"
[[choices.values]]
value = "literal"
label = "Literal (acknowledge after the checksum)"
vidas = {alternate = false}
[[choices.values]]
value = "alternate"
label = "Alternate (acknowledge after ETX)"
vidas = {alternate = true}

# The installed software version: only the lab knows it, so no suggestion.
[[choices]]
name = "software_version"
kind = "software_version"
label = "Software version on the About screen"
[[choices.values]]
value = "v1"
label = "Version 1 (sample IDs up to 12 characters)"
vidas = {sample_limit = 12}
[[choices.values]]
value = "v2"
label = "Version 2 or later (sample IDs up to 17 characters)"
vidas = {sample_limit = 17}
```

The whole synthetic example is
[`vidas_choices.toml`](../../crates/core/config/examples/vidas_choices.toml).
The rules the loader checks:

| Rule | Why |
| --- | --- |
| `name` and `value` are 1 to 64 lower-case letters, digits, `-` or `_` | The same key in the site file, on the command line and in the form |
| `label` is plain words naming where the analyzer shows it | Setup shows it as the question and the value's words |
| `kind` is `mode` or `software_version`, at most one `software_version` | A version band is read on the About screen, not set |
| `suggested` is one of the choice's values, or absent when only the lab can know | Without a suggestion the lab must choose; nothing is assumed |
| 2 to 16 values, at most 16 choices and 64 combinations | Every combination is validated as a whole stack when the definition loads |
| A value selects only these blocks: `mappings` (a whole table: a host dialect's field positions), `orders = "on"` or `"off"` (keep or turn off the definition's `[orders]`), `vidas.alternate`, `vidas.sample_limit` | A choice selects behaviour the protocol code already has; no field path or expression |
| Every value of a choice selects the same blocks, and a block belongs to one choice | Each value states its whole effect |
| A block a choice selects is written only in the choice; leave the `[mappings]` table or the `[vidas]` key out of the definition | One place decides it |
| A choice cannot write `evidence`, `deviations` or `departure_scope` | Departures stay the evidenced list below (ADR 0015) |

The lab's values go in the site file's `[choices]`, which Setup writes (see
[connect](../operations/connect.md#analyzer-settings)). A new kind of building
block needs a change in the config crate with its tests, not a config
feature; ask the maintainers.

### Declare departures, do not program around them

Keep `deviations = []` for the strict example. Under
[ADR 0015](../adr/0015-named-machine-departures.md) each entry is one variant of a
closed list, with its own scope, evidence references and evidence level. No
field path, pattern, script or free status mapping can select behavior.

| Variant | What it allows | Evidence it needs |
| --- | --- | --- |
| `Hl7AbsentObr25MeansFinal` | An absent or empty OBR-25 in generic ORU^R01 patient uploads means a final report | A manual or vendor statement naming the omission and guaranteeing final reports; a recording alone is not enough |
| `Hl7AbsentObx11MeansFinal` | An absent or empty OBX-11 means a final observation | A manual or vendor statement guaranteeing final-only observations in that mode |
| `AstmPackedRecordsInFrame` | Several CR-ended LIS02 records in one verified inbound frame | A manual section naming the packed mode, or an ADR 0009 recording showing it |
| `Hl7Utf8InV231`, `Hl7CrLfTerminators`, `Hl7LfTerminators`, `Hl7PreserveUnsupportedEscapes`, `AstmFrameLimit`, `AstmReceiveTimeout`, `AstmSendTimeout`, `AstmHostContentionPriority`, `AstmLowercaseChecksumHex` | The existing compatibility settings, with their current limits | The same scope and evidence rules as above |

The scope names the confirmed model, exact firmware, installed mode, edition,
direction and message kind; no wildcard covers a firmware upgrade. Both HL7
variants are needed when both status fields are absent. What each variant does
inside its protocol is in the [HL7](../protocols/hl7/decisions.md#named-missing-status-interpretations-and-contract-v1-parking)
and [ASTM](../protocols/astm/decisions.md#evidenced-inbound-record-packing) decisions.

Loading refuses unknown tags, duplicates, conflicting variants, scope
mismatches, empty or unresolved evidence, wrong recording digests, a manual
reference without title, revision and section, and a level the evidence does
not support. File references are local and bounded; nothing is fetched. A
reviewer still checks what the cited manual actually says. An override cannot
remove evidence and keep the behavior.

No departure can weaken a clinical safety rule, infer clinical release,
normality or QC acceptance, or invent a sample ID, result identity, unit or
status. Missing MSH-10, a missing OBX-3 coding system, OBR-only QC layouts and
bare LIS02 over TCP are candidates only: each needs its own evidence and a
narrow new variant, never a blanket tolerance. The service can still reject
choices the config library accepts (it currently requires strict ASTM checksum
hex). If the list cannot express a quirk, stop
and ask maintainers. A reviewed
code change belongs in the owning layer; do not weaken a shared standard or
invent a config key. A new subsystem, protocol family, format, dependency or
unplanned ADR requires agreement before building it.

## Validate before connecting

The command checks a **program file**, not `machine.toml` directly. Start from
[the program example](../../crates/analyte/examples/analyte.toml), following the
[service guide](../../crates/analyte/README.md), and point its machine entry at the
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

## From a recording to a definition

A model without a definition can still be connected to record only
([ADR 0034](../adr/0034-record-only-connections.md),
[First visit: record only](../operations/connect.md#first-visit-record-only)).
Give its folder a `record.toml` with only what its documents already say:

```toml
# How the model talks, for a record-only connection (ADR 0034).
recording_schema_version = 1
protocol = "astm"            # astm, hl7, vidas or autolyser
[transport]
kind = "serial"              # serial, tcp_server or tcp_client
[transport.serial]           # the manual's suggestion; leave out what it does not state
baud = 9600
```

Everything else comes from the protocol's shipped capture profile in
`crates/core/config/src/recording/profiles/`, which holds only the standard's
own link rules, each with its source section. The machine tests load every
`record.toml` through the real loader; a folder never holds both it and
`machine.toml`.

If the analyzer's own screen offers a mode that changes how it frames records
and its manual says one mode packs several ASTM records in one frame, the file
may ask that mode and declare the packing departure for that mode only (ADR
0034 decision 10). [The cobas e 411's](../../machines/roche/cobas-e411/record.toml)
is the example: `[analyzer_mode]` with a label and its values (no
suggestion), then one `astm_packed_records_in_frame` entry in `deviations`
with the full [ADR 0015 scope and evidence](#declare-departures-do-not-program-around-them),
the scope's `model` being the folder key and its `mode` one of the values. No
other departure can be declared here; the lab answers the mode in Setup.

Then, from the visit's recordings:

1. Export each capture with `analyte captures export` to a protected folder.
2. Make a de-identified copy as described below, and review it.
3. Read the facts the definition needs from the conversation: field
   positions, identity, QC markers, deadlines actually used. Cite the manual
   section for each rule; a recording shows what the machine did, not what it
   must do.
4. Write `machine.toml` at the documented level, delete `record.toml`, and put
   the reviewed copy in `recordings/` so the replay test runs it.
5. Set the machine up normally. Messages recorded before are never delivered.

## Capture, de-identify and replay

Follow [ADR 0009](../adr/0009-recording-format.md) and the
[simulator guide](../../crates/testing/sim/README.md). Capture both directions and timing
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

`cargo test --workspace --all-features` discovers every `machines/**/machine.toml`, validates
it with the real config loader, and replays every `.rec.jsonl` in its
`recordings/` folder through the service. Use the fixed README sections in
[machines/README.md](../../machines/README.md), including one `Level: documented`,
`Level: simulated` or `Level: proven` line under Evidence matching the TOML.
An empty machine inventory passes and reports that it found no definitions.

The automatic test uses temporary stores and loopback peers, without live
credentials or patient data. Physical serial/TCP I/O is replaced for replay;
the configured framing, dialog, syntax and mapping run in the real service.
Host bytes must match the recording, subject only to its declared located
exceptions. Complete-file recordings contain one payload split across inbound
events and no stream link transitions or host replies; replay checks the exact
stored bytes. Serial break events remain unsupported by the replay library.

Run the discovery test alone with:

```text
cargo test -p analyte --test delivery every_machine_validates_and_replays -- --nocapture
```

For additional custody, delivery and failure assertions, follow the existing
tests in [delivery](../../crates/analyte/tests/delivery/mod.rs). A
recording replay does not establish that every clinical workflow is qualified.
Adding a definition or test also requires the workspace checks.

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

[ADR 0005](../adr/0005-evidence-levels.md) applies separately to each driver and config.

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
[ADR 0012](../adr/0012-uniform-layout-and-naming.md) describes. Keep site
addresses, credentials, patient data and original captures out of the PR.

Include the definition, supported model/software and capabilities, honest evidence
level, open questions, source sections in your own words, reviewed recordings,
replay tests/results and named departures with reasons. Explain expected sample,
result and order identity behavior and unsupported options. Do not paste manual
text, tables or examples or commit standards/vendor documents. Use original
synthetic or reviewed de-identified fixtures only. Follow
[CONTRIBUTING.md](../../CONTRIBUTING.md): signed-off Conventional Commits and AI
assistance disclosure where applicable. A config PR is not permission to go live.

## Final checklist

- [ ] Exact model/software, interface and supported scope confirmed.
- [ ] Connection, encoding, limits and deadlines verified; unknowns recorded.
- [ ] Tube ID source/field, test codes, units and dilution confirmed by the lab.
- [ ] Retransmission, correction and rerun evidence reviewed; no invented identity.
- [ ] Three config layers separated; no secrets; all keys supported.
- [ ] Outbound namespaces, context and failure policy explicitly agreed if enabled.
- [ ] Settings the analyzer offers on its screen or ties to its software version declared as choices with the manual's suggestion; everything else fixed.
- [ ] Named departures justified; unsupported quirks referred to maintainers.
- [ ] Program passes `analyte check-config`.
- [ ] Recordings reviewed for privacy and replay tests cover applicable failures.
- [ ] Evidence level matches only the exercised capabilities.
- [ ] PR contains sources, evidence and verification results without protected data.
- [ ] Lab validation through the final report completed before activation.

## Retention

Every site file must explicitly set `retention = 90` (recommended), another
period from 1 to 3650 days, or `retention = "keep"`. There is no default.
A finite period requires the host credential `retention-digest` through the
installation secrets path. Keep that credential with the installation secrets
when restoring; rotation is not supported. Older backups retain purged data
until the deployment removes them through its backup rotation.

Machine definitions use `schema_version = 2`: set `protocol` and
`transport.kind`, and the loader derives framing, dialog and syntax, so those
sections carry no `kind` key. Any other machine schema version is refused as
unsupported.
