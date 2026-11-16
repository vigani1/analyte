# 0015. Typed, evidenced departures for individual machines

Status: accepted, 2026-11-16; delivery scope clarified by the project lead

## Context

Strict HL7 report context and LIS01 record boundaries correctly reject or
park input that some real analyzers send. [Research](../research/machine-departures.md)
finds documented packed-frame modes and a vendor example omitting report
status. It does not establish a general license to treat missing statuses
as final. Changing shared standards to tolerate these differences would hide
which acceptance contract a lab actually enabled.

[ADR 0002](0002-config-first.md), [ADR 0007](0007-standards-first.md) and
[ADR 0012 §5](0012-uniform-layout-and-naming.md#5-machines) already place
departures in machine configuration. Today's `Deviation { name, key, reason }`
is attached to an allowlist of compatibility settings, not arbitrary behavior.
It lacks typed status/packing departures and mandatory evidence per departure.
This decision strengthens that existing seam. It qualifies no real machine.

## Decision

### One closed vocabulary

Use one exhaustive `MachineDeparture` enum, with bounded, validated associated
data. Its stable serialized tag names the rule. No arbitrary field path,
replacement text, script, regular expression, user-defined status mapping or
`ignore_validation` variant may select behavior. Human-readable names and
explanations may describe a declaration but never drive execution.

The initial new behaviors are the two families below. Separate HL7 variants
prevent an OBR exception from implicitly relaxing OBX, or the reverse.

| Variant | Exact rule relaxed | Fixed substitute meaning | Required qualifying evidence |
| --- | --- | --- | --- |
| `Hl7AbsentObr25MeansFinal` | Presence of OBR-25 in the selected generic result-report profile: HL7 2.3.1 §7.3.2, later supported editions §7.4.1.25 | Only an absent or empty OBR-25 in the declared report scope is interpreted as a final consolidated report. Source OBR-25 remains absent. This does not assign observation status. | A machine manual or vendor clarification identifying the exact omission and guaranteeing complete, final reports in the declared mode. A recording alone is insufficient. |
| `Hl7AbsentObx11MeansFinal` | Required OBX-11 in the selected generic result profile: HL7 2.3.1 §7.3.2, later supported editions §7.4.2.11 | Only an absent or empty OBX-11 in the declared observation scope has the documented meaning final. Source OBX-11 remains absent. This does not establish panel completion. | A machine manual or vendor clarification explicitly guaranteeing final-only observations when this field is absent in that mode. A table saying the field is always populated with F is insufficient by itself. |
| `AstmPackedRecordsInFrame` | LIS01-A2 §§3.3.2, 6.3.1.1, 8.3.1.1, requiring LIS02 records to begin in separate frames | A verified frame's text may contain several CR-terminated LIS02 records in wire order. Frame and record boundaries remain distinct; no record or semantic content is synthesized. | A manual section naming the packed mode, or an ADR 0009 recording that demonstrates its exact boundaries, segmentation and direction. |

Each declaration carries its own `EvidenceLevel` from ADR 0005 and evidence
references. Manual-only support is `documented`; official simulator/vendor
sample exercises can support `simulated`; real machine recordings support
`proven` only for exercised capabilities. Synthetic tests do not raise levels.
The two status variants always retain the required documented meaning,
including at simulated or proven level. Level alone never establishes safety.

The declaration's scope contains the confirmed model, explicit firmware
applicability, installed protocol mode, edition, direction and closed message
kind selection. No wildcard may silently cover a firmware upgrade. Unknown
firmware prevents activation unless a vendor statement explicitly covers that
scope. Initially status departures apply only to generic HL7 ORU^R01 patient
result uploads; no LAW, QC, queries, order traffic or other report types inherit
them. Both declarations are required if both status fields are absent.

### Existing compatibility settings

Keep one declaration mechanism, not parallel old and new lists. Before an
implementation replaces the schema, explicitly represent existing allowed
settings in that same exhaustive enum: `Hl7Utf8InV231`,
`Hl7CrLfTerminators`, `Hl7LfTerminators`, `Hl7PreserveUnsupportedEscapes`,
`AstmFrameLimit`, `AstmReceiveTimeout`, `AstmSendTimeout`,
`AstmHostContentionPriority` and `AstmLowercaseChecksumHex`.
Their exact behavior and limits remain those of the current validated
settings; unsupported escapes still cannot acquire semantic identity and
lowercase checksum hex still requires the correct checksum. They gain the
same evidence/provenance requirements, not additional tolerance. This is the
complete existing-settings inventory in `validation/stack.rs` at proposal time.

Do not silently migrate a free-text name/reason into qualifying evidence.
Schema/version migration must require missing evidence and reject ambiguous
old declarations before activation. Config and contract migration details
belong in the later implementation review; this document introduces neither
a second stored format nor a new dependency.

### Validate once, fail specifically

At config load, after shipped/site/override composition, reject:

- Unknown tags/fields, duplicate rule declarations, empty evidence, conflicting
  variants, inactive declarations that disagree with selected settings, and
  protocol, edition, direction or message scope mismatches.
- Missing rule-specific evidence, unresolved recording references, mismatched
  digests, missing manual title/revision/section, unsupported applicability or
  contradictory vendor statements. File references are bounded and local;
  config loading never fetches a remote manual.
- A claimed level without the corresponding capability evidence. Recordings
  identify ADR 0009 file/digest and exchange; manual references identify an
  operator-resolvable protected reference, not a committed PDF. Validate
  reference structure/resolution and level prerequisites mechanically; a
  reviewer must verify what the cited manual actually means.

The resulting validated type is the only way to enable a departure. Default
configuration selects none. Continue the existing immutable effective-config
snapshot and between-conversation activation rules. Reprocessing explicitly
chooses a validated snapshot and creates a new interpretation. An override
cannot remove evidence while retaining behavior.

### Preserve all other checks

For HL7, absence means omitted or empty, never explicit null, malformed or
unsupported status text. Populated statuses are validated and retained; never
overwrite P, R, C, D, X or another status with F. A present non-final status
within a declared final-only scope contradicts its guarantee: park that report
with a specific departure-conflict reason. A partial or failed observation
cannot prove a final consolidated report. Do not derive panel completion by
counting received OBX segments. Other required context, including OBR-4,
OBX-3, value type and edition-specific report time, remains strict. A normal
standard profile without the departure retains its current parking behavior.

For ASTM, verify the original frame's checksum, sequence, envelope and limits
before accepting its packed text. Reassemble any evidenced cross-frame record
continuation within existing bounds; split only at actual CR record endings,
never TCP read boundaries. Require every new record and hierarchy transition
to pass LIS02 rules. Retain byte spans linking records to frames. Frame custody
is committed once for the entire frame before its ACK, never once per derived
record. A later syntax/mapping failure parks retained evidence; it does not
erase custody or turn a frame ACK into clinical acceptance. Outbound host
orders remain standard record-per-frame; inbound packing does not enable
outbound packing. A wrong/missing checksum still receives NAK.

Apply machine behavior through explicit typed settings at the owning layer:
record boundary policy at framing/dialog, missing-status interpretation at
report mapping. Standard parsing retains raw absence and strict diagnostics;
an evidenced machine interpretation may discharge only its named diagnostic.
Shared defaults and standard conformance fixtures do not change. No vendor-name
branches inside shared code.

### Interpretation, delivery and admin evidence

Every receipt interpretation must retain the enabled declaration snapshot and
an applied-departure list, even when that list is empty or the interpretation
is parked. Existing driver/config versions and digests remain. Each applied
entry identifies the stable enum tag, scope, rule/section, evidence references
and level, source byte/segment/field locations, original absence or packing,
and exact derived meaning. Enabled and actually applied are separate facts.
The original source status is never rewritten as though the analyzer sent F.

Delivered result evidence must include the applied tags, source/derived
distinction, scope, evidence level and stable evidence references alongside
interpretation/config identity. Do not send raw messages, protected manual
paths or secrets. References must let authorized operators find protected
evidence. The later implementation must make the existing model/contract
carry this explicitly; config digest alone is insufficient. Retry bytes and
delivery identity remain stable. No implementation may silently omit this
evidence because an older receiver cannot represent it.

The admin machine page shows each enabled rule, substitute meaning, scope,
evidence/level and active config digest. The receipt page shows which rule
actually applied and links to protected evidence; parked conflicts show the
exact failed condition. Show a conspicuous final-from-documentation label
separately from a received final status. Keep patient content out of ordinary
logs. The machine README Departures section mirrors these declarations.

### Rules that can never be departures

None of clinical safety rules 1–10 may be weakened through configuration:

1. Durable storage before acknowledgment, including each ASTM frame.
2. Retention and visible parking of failed or unresolved data.
3. Immutable original bytes and versioned reprocessing.
4. Distinct receipt/delivery/result identities, revisions and reruns; no
   invented result identity or sender identity standing in for evidence.
5. Original values and string sample IDs, including case and leading zeros;
   no invented sample ID or replacement measurement.
6. Original units and explicit unit conversions/dilution provenance; no
   guessed units or dilution state.
7. Size/time limits and bounded waits.
8. Distinct order states and no resend caused by replay or delivery retry.
9. Lookup failure remains unknown, never a successful no-orders response.
10. No patient data in ordinary logs.

Nor may a departure infer clinical release, normality, QC acceptance or a
final status without the machine's documented omission meaning. If a machine
cannot satisfy these constraints, retain evidence and stop; do not broaden
the enum to conceal uncertainty.

### Candidates, not accepted behavior

- Missing OBX-3 coding system: investigate an explicitly documented fixed
  local namespace and exact code mapping, preserving wire absence. Never
  label an arbitrary local code LOINC. Existing selectors may already suffice.
- Other missing report context, OBR-only QC layouts and vendor value types:
  require separate evidence and narrow variants only if existing config
  cannot express them. No blanket missing-field policy.
- Missing MSH-10: unresolved; no candidate generator of a sender ID. Any
  future accommodation needs independently evidenced correlation and identity
  guarantees and cannot waive rule 4.
- Bare LIS02 TCP without checksums: a separately selected documented stack,
  if justified later; never a fallback or checksum-off LIS01 option. Missing
  or wrong checksums in the selected LIS01 stack remain errors.

CRLF/LF terminators and lowercase checksum digits are already explicit
settings, covered above, not new candidates for auto-detection.

## Acceptance evidence

Keep standard tests unchanged. Add paired strict/departure replay cases for
each rule, citing its standard section and vendor evidence. Test absent versus
null/unknown/present statuses, partial panels, contradictions, corrections,
reruns, identity uncertainty and exact delivered provenance. Test packed
records with split/merged reads, ETB continuation, checksums, frame rollover,
malformed hierarchy, limits, lost ACK, disk failure and restart after commit
before ACK. Verify admin enabled/applied distinctions and config-load rejection
cases. Synthetic fixtures exercise failure paths without raising evidence.

## Consequences

Labs can support documented differences without weakening standard defaults.
Some machines remain parked until their meaning is established; a recording
alone cannot supply universal clinical semantics. The implementation spans
config, interpretation evidence, delivery and admin display, but adds no plugin
system, new protocol family or general transformation language. ADR 0003's
safety guarantees remain unchanged. Synthetic tests establish implementation
behavior, not qualification of any installed machine.


**Delivery clarification, project lead, 2026-11-16:** Contract v2 is outside this
task. Framing-only ASTM packing keeps contract-v1 delivery unchanged; its applied
ledger remains in the receipt interpretation and admin page. Delivery of
**departure-derived HL7 final status** waits for a future contract version under
ADR 0008 that carries this evidence explicitly. Until then these interpretations
park with `departure-derived status requires a contract version that carries departure evidence`,
keep original bytes and source absence, and create no delivery. This deliberate
v1 restriction supersedes the general delivered-evidence requirement above for
the current implementation; it is not permission to drop status provenance.

New ledgers distinguish enabled declarations from actual applications. Legacy
interpretations without a ledger remain visibly unrecorded. Bounded ledger
failures are explicit, retain the original and prevent delivery. Patient-only
status scope requires explicit classification, not inference from an ORU name.
Implementation and synthetic acceptance cases are linked from the HL7 and ASTM
decisions files. No driver or machine evidence level is raised by this acceptance.
