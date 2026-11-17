# Machine definitions

Each definition lives in `machines/<vendor>/<model>/`, with lower-case,
hyphenated names. It contains `machine.toml`, `README.md` and `recordings/`.
Keep site settings and credentials outside this tree. Recordings follow
[ADR 0009](../docs/adr/0009-recording-format.md) and contain only synthetic
or reviewed de-identified data. Real machines must have their own evidence;
the example below is not a real machine.

`cargo test --workspace` discovers every `machine.toml`, validates it through
the real loader, compares the README evidence level, and replays every recording
through the running service. A model folder with a README must also contain
`machine.toml`, unless its README explicitly says "a blocked machine definition,"
(line wrapping is allowed). This catches accidentally deleted definitions.
The README uses these sections in this order. In Evidence, write exactly one
`Level: documented`, `Level: simulated` or `Level: proven` line matching TOML.
Synthetic recordings never raise the evidence level.

## README template

```markdown
## Machine
Describe the confirmed facts, sources and unresolved details.

## Connection
Describe the confirmed facts, sources and unresolved details.

## Settings on the machine
Describe the confirmed facts, sources and unresolved details.

## Supported workflows
Describe the confirmed facts, sources and unresolved details.

## Departures
Describe the confirmed facts, sources and unresolved details.

## Evidence
Level: documented

## Open questions
Describe the confirmed facts, sources and unresolved details.

## Sources
Describe the confirmed facts, sources and unresolved details.
```

## Typed departures (ADR 0015)

The `deviations` list contains closed `MachineDeparture` declarations. Its
historical TOML key remains; free-text `name`, `key` and `reason` entries are
rejected with an explicit migration error. Supply the actual scope and evidence;
old explanations are not converted to evidence. Empty lists select no departures.
An override replaces the whole list and must retain evidence for every selected
nonstandard setting. Both shipped and composed configurations are checked.

Each entry has `rule` and `declaration`; the declaration contains `scope`, `level`
and one through sixteen `evidence` references. At most twelve rules are allowed,
without duplicates. The root `departure_scope` records installed applicability;
each declaration must match it exactly. Scope names the confirmed `model`, exact
`firmware`, installed `mode`, protocol `edition`, `direction` and closed `message`
kind. Wildcards do not select firmware upgrades. `firmware="unknown"` requires
an explicit vendor guarantee covering unknown firmware for that rule.

| Rule tag | Meaning |
| --- | --- |
| `hl7_absent_obr25_means_final` | Documented absent OBR-25 meaning, separately from observation status |
| `hl7_absent_obx11_means_final` | Documented absent OBX-11 meaning, separately from report completion |
| `astm_packed_records_in_frame` | Inbound verified frames may hold several CR-ended LIS02 records |
| `hl7_utf8_in_v231` | Selected later UTF-8 code in HL7 2.3.1 |
| `hl7_cr_lf_terminators` | Selected CRLF segment endings |
| `hl7_lf_terminators` | Selected LF segment endings |
| `hl7_preserve_unsupported_escapes` | Preserve unsupported escapes without semantic identity |
| `astm_frame_limit` | Selected nondefault, bounded frame limit |
| `astm_receive_timeout` | Selected nondefault, bounded receive deadline |
| `astm_send_timeout` | Selected nondefault, bounded send deadline |
| `astm_host_contention_priority` | Selected host contention priority |
| `astm_lowercase_checksum_hex` | Selected lowercase checksum digits; checksum remains mandatory |

HL7 declarations select `{protocol="hl7",edition="v251"}` (or the exact supported
edition), `direction="machine_to_host"`, `message="oru_r01_patient"`. ASTM uses
`{protocol="astm"}` and `message="astm_results"` or `"astm_link"`. Packing is
restricted to machine-to-host result traffic over LIS01. The two HL7 terminator
variants conflict. Existing-setting declarations must agree with active settings.

A manual reference contains `kind="manual"`, a stable protected `reference` ID,
`title`, `revision`, `section`, `guarantee`, `covers_unknown_firmware` and
`contradicts`. Reference IDs are not local paths or URLs. The closed guarantee is
`absent_obr25_final`, `absent_obx11_final`, `packed_records` or `compatibility`.
Reviewers must verify the cited meaning and applicability. Conflicting vendor
statements block activation. Manuals remain protected and are never fetched.

Recording references contain `kind="recording"`, `file`, `sha256`, `first_event`
and `last_event` (one-based, inclusive). Paths are relative to the shipped
machine definition's directory, including after override composition. Only local
`.rec.jsonl` files inside that directory are resolved; traversal, symlink escapes,
unresolved references and digest mismatches are refused. The existing ADR 0009
reader enforces file, line, event and payload bounds. Model, firmware, protocol
and exchange direction must agree. The exchange must include actual wire bytes.

Each declaration has its own evidence level. Status omissions always require the
specific manual/vendor guarantee; recordings alone never establish final-only
meaning. Packing can instead use a qualifying recording. Synthetic recordings
do not qualify packing or raise a level. Simulated/proven claims also require a
matching machine capability whose `dialogs` lists the rule tag and whose
`evidence_files` lists the recording. No evidence qualifies another capability.

Mirror enabled rules, their substitute meanings, exact scope, references and
levels in the machine README's Departures section. The synthetic example selects
none; it is not evidence for a real analyzer.

Status interpretation also requires an explicit patient classification from the
configured equality rules; unknown, control and calibration classifications do
not inherit patient-only scope. Any final status derived from either omission
parks under contract v1, with its specific contract-version reason and no
outbox delivery. Received statuses stay untouched. Framing-only ASTM packing may
use v1; its application remains in the protected interpretation ledger.

Every declaration snapshot is bounded to 16384 UTF-8 bytes. New interpretations
retain enabled and applied lists explicitly. An incomplete ledger is flagged and
prevents delivery; a legacy absent ledger means unrecorded, not empty. The admin
machine list links to receipt interpretation versions and shows active digests.
Elevated departure evidence also requires a tested config digest, driver and
platform in the matching capability, never `untested` or `unknown` placeholders.

Machine definitions use [schema 2 and its schema 1 migration rule](../docs/config-migration.md).
