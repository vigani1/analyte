# Named machine departures: research

Research date: 2026-11-16. Later note: ADR 0015, called proposed below, was
accepted on 2026-11-16. Documents and public source only; no analyzer,
simulator or patient data was used. **Verified** means read in the linked
source, not reproduced. **Inferred** identifies our recommendation. Public
mirrors establish what a particular manual says, not that it applies to an
installed model or firmware. No vendor examples are reproduced here.

## Recommendation

**Inferred:** Keep standard profiles strict. Permit a machine to select only
reviewed, typed departures with narrow scope, explicit meaning and retained
evidence. A transport accommodation such as record packing is different from
assigning clinical meaning to an absent status. The latter needs an explicit
machine statement; repeated final-looking values in a capture cannot prove
that all future results are final. See [proposed ADR 0015](../adr/0015-named-machine-departures.md).

## What public sources establish

These examples establish recurring kinds of interoperability differences,
not their prevalence. An implementation guide may deliberately narrow HL7;
an omitted field is a departure only where the selected edition/profile
requires it. A family manual cannot qualify a related model automatically.

| Point | Verified public example and source section | Consequence for Analyte (inferred) |
| --- | --- | --- |
| Missing report status | YHLO LIS Protocol Manual V1.1, §2.4.1 patient result example, leaves OBR-25 empty; §2.3.5 specifies final OBX-11 and the example supplies it. [YHLO manual][yhlo] | This supports researching an OBR omission, not treating absent OBX-11 as final or proving applicability to iFlash 1200. |
| Missing observation status | No named analyzer with an explicitly documented absent-OBX-11 meaning was verified in this search. Solana instead documents an always-final OBX-11 in its test-result OBX definition. [Solana bulletin, OBX tables][solana] | A statement that a field is always sent as F does not alone authorize replacing its absence: absence could indicate truncation. Require a documented omission contract. |
| Local observation codes | YHLO §2.3.5 defines OBX-3 as a configurable channel identifier; its §2.4.1 example uses local codes without a coding-system component. It also uses OBX-9 for qualitative calls and a vendor BOTH value type. [YHLO][yhlo] | A local code is not automatically LOINC. Qualify a fixed source namespace and exact mappings separately; the proposed status departure must not enable the other differences. |
| Several records in one ASTM frame | Roche cobas e 411 Host Interface Manual v2.0, part D, Communication data structure, D-1/D-2, distinguishes Elecsys mode from cobas mode; only the latter permits packed records. [Roche manual][roche] | Bind packing to the confirmed protocol mode; do not enable it for every Roche instrument. This public revision is not the v2.3 manual named in Analyte's plan. |
| Packing as a driver option | Siemens Atellica Data Manager LIS Interface Guide, 11314645 Rev.05, §4 ASTM 1394 Protocol, p.28: driver option `-l` changes upload packing in v4.5.0; behavior differs in v4.4.2. [Siemens guide][atellica] | Record the driver version, direction and selected setting. A checkbox name is insufficient evidence across releases. |
| No ASTM envelope/checksum | ThunderBolt User Manual, TB-0001 Rev D, §8 LIS Interface, describes a separate clean TCP mode without control characters or checksums. §8.6 LIS Configuration describes frame pooling into TCP packets. [ThunderBolt manual][thunderbolt] | Bare LIS02 over TCP is a configured alternative stack, not permission to ignore missing LIS01 checksums. TCP packet coalescing is also distinct from several records within one LIS01 frame. Neither is included in this proposal. |
| CR versus LF handling | Connect 4.5.2 `ER7Serializer` has configurable segment delimiters, line-break conversion and strict parser/validation settings. An older report, issue #649, describes embedded LF being mistaken for a segment ending. [Connect source][connect], [issue report][lf-issue] | Select one evidenced terminator, retain original bytes and reject unexpected mixtures. Global line-break replacement can change report text. The issue is historical, not proof of a current defect. |
| Missing MSH-10 | No named analyzer omission was verified. HL7 defines message control identity separately from result identity; Solana's MSH table requires a unique message ID. [HL7 chapter 2, MSH-10][hl7-control], [Solana][solana] | Do not generate a replacement sender ID. Receipt IDs are local custody identities and cannot make a missing sender identity valid. |

**Verified:** HL7 v2.5.1 chapter 7 §7.4.1.25 describes consolidated report
status, while §7.4.2.11 describes observation status and its transitions.
These are separate facts, not interchangeable defaults. The historical IHE
LAW working page records discussion of OBR-25 and a resolution using ORC-5
in that profile. That is profile-specific history, not a waiver for generic
ORU messages. [HL7 chapter 7][hl7-results], [IHE LAW, Correct OBR-25 Usage][law]

**Verified:** CLSI's public LIS02 description separates record content from
low-level transfer and marks the edition archived. The public LIS01 sample
identifies its edition and archival status. Neither public catalog summary
is enough to derive packed-frame acceptance rules; Analyte's strict decision
cites LIS01 §§3.3.2, 6.3.1.1 and 8.3.1.1 from its standard work.
[CLSI LIS02, Abstract/Additional Details][clsi-lis02], [LIS01 sample][clsi-lis01],
[Analyte ASTM decision](../protocols/astm/decisions.md#strict-records-and-semantic-selectors-standards-pass-3)

## How middleware accommodates differences

**Verified:** The OpenELIS Analyzer Bridge separates analyzer profiles,
connection settings and field extraction. Its pinned authoring guide has
explicit result-part selectors, QC rules, test/component mappings and
connection-specific code overrides. Its `default` and `value_defaults`
entries describe initial mapping exclusions, not permission to invent a
missing result status. This distinction matters when borrowing a mechanism
called a default. [Bridge authoring guide, Tests, Reading a result and
Connection settings][bridge]

**Verified:** Connect's cited serializer can disable validation independently
of choosing the strict parser. Thus successful deserialization alone does
not establish conformance. Siemens's guide demonstrates explicit
version-specific driver settings rather than a global tolerant parser.
[Connect, constructor and toXML][connect], [Siemens, §4 p.28][atellica]

**Inferred:** Adopt the per-machine configuration and visible mapping
provenance, not a general scripting engine or a validation-off switch.
Transforms are safe only when their inputs, meaning, scope, failure behavior
and evidence are explicit, and the original remains recoverable. A transport
ACK is custody, not verification or clinical release. A documented final-only
device mode still does not authorize Analyte to perform autoverification.

## Safety evidence and limits

**Verified incident evidence:** Pennsylvania's 2018 advisory, System Interface
Issues, reports results and orders failing to cross systems. Its output/display
examples include a result autoverified before autodilution; Didn't Do reports
include order defaults left unchanged. Those are distinct categories in the
report. It does not establish that missing OBX-11 or Mirth caused an incident.
[Beyond the Lab, respective subsections][safety]

**Verified defect report:** Connect #649 describes text damage from treating LF
as CR; it is a reporter's account, not an independently reproduced clinical
incident. [Issue #649, impact and suggested action][lf-issue]

**Inferred hazard:** Silently supplying F can promote preliminary, unverified,
failed or corrected results into the wrong lifecycle state. No primary incident
report specifically attributing patient harm to an absent-OBX-11-to-F default
was verified here. The HL7 status distinctions support the hazard analysis;
they do not supply an incident rate. A safe policy parks uncertainty and tests
corrections, reruns, partial panels and restart behavior end to end.

## What Analyte has today

Inspected on the task's main-based checkout, 2026-11-16:

- [ADR 0012 §5](../adr/0012-uniform-layout-and-naming.md) already
  requires named departures in `machine.toml` and a Departures README section.
  [machines/README.md](../../machines/README.md) provides that template.
- [schema/mod.rs](../../crates/core/config/src/schema/mod.rs) has
  `Deviation { name, key, reason }`, machine `deviations`, and override
  replacement of that list. Names/reasons are descriptive, not executable.
- [validation/stack.rs](../../crates/core/config/src/validation/stack.rs)
  allowlists compatibility keys and requires a declaration for selected
  nonstandard options: UTF-8 in 2.3.1, ASTM frame limit/checksum hex case,
  deadlines/contention, HL7 terminator and escape preservation. It rejects
  arbitrary behavior keys and duplicate names. It does not enforce the
  proposed per-departure evidence contract or a closed variant for each rule.
- `Terminator` already selects CR, CRLF or LF. `ChecksumHex` permits strict
  hex or lowercase hex; neither permits a missing or wrong checksum.
  [stack/hl7.rs](../../crates/core/config/src/stack/hl7.rs) and
  [stack/astm.rs](../../crates/core/config/src/stack/astm.rs) expose validated
  settings, not a generic bypass.
- Neither requested new behavior is available. The [HL7 decision](../protocols/hl7/decisions.md#park-reports-until-required-context-is-established)
  parks missing context; the ASTM decision above refuses packing. Existing
  effective-config storage supplies provenance but is not an applied-departure
  ledger. Dedicated admin/delivery visibility is a proposed requirement, not
  an implemented feature established by this research.

## Evidence needed before enabling a departure

**Inferred:** Each declaration must identify the exact standard clause,
machine/model, firmware and mode, direction and message types; the exact
condition relaxed; the fixed replacement meaning; and what remains strict.
Use a manual title/revision/section or an ADR 0009 recording reference with a
digest and relevant exchange. Status substitution additionally requires the
machine's documented statement about omission and meaning. Examples alone
cannot establish a universal final-only guarantee. Conflicting evidence blocks
activation. Manuals remain outside Git; reference them without copying them.

The admin machine page should show enabled departures and their evidence
levels, while the receipt interpretation shows which actually applied, at
which source locations, with original absence and derived meaning separated.
Delivered evidence must carry the same applied identifiers and provenance
without exposing raw patient messages or local reference-file paths. Keep
strict identities, sample IDs, units, values, custody, bounds and lifecycle
rules. A capture can establish packing; it cannot prove omitted status is
always final. Evidence levels retain ADR 0005's meanings.

## Sources

All technical sources are vendor-authored documents, standards material,
maintainer code or maintainer-hosted issue reports. Mirrors are labeled by
their host in the links. Forum searches produced no stronger qualifying
evidence for the unresolved omissions. Nothing here qualifies a machine.

[yhlo]: https://www.scribd.com/document/510324625/YHLO-LIS-Protocol-ManualV1-1
[solana]: https://connectme.quidel.com/files/HL7-LIS-Specs-Solana-TechnicalBulletin-TB20278102EN00.pdf
[roche]: https://cimbrasleon.siaeweb.com/img/archivos/242.pdf
[atellica]: https://manuals.plus/m/dfe5f3305d6b1a31026e6d23e3aab494178664e0c9edef6a8443a6582a230a9b.pdf
[thunderbolt]: https://artekinmedikal.com.tr/wp-content/uploads/2019/03/ThunderBoltUsersManual.pdf
[connect]: https://github.com/nextgenhealthcare/connect/blob/4.5.2/server/src/com/mirth/connect/plugins/datatypes/hl7v2/ER7Serializer.java
[lf-issue]: https://github.com/nextgenhealthcare/connect/issues/649
[bridge]: https://github.com/DIGI-UW/openelis-analyzer-bridge/blob/5a3b8a361f873af566bfaae8a549d3b80a4d958e/docs/profile-authoring.md
[hl7-control]: https://hl7.eu/HL7v2x/v251/std251/ch02.html
[hl7-results]: https://hl7.eu/HL7v2x/v251/std251/ch07.html
[law]: https://wiki.ihe.net/index.php/Laboratory_Analytical_Workflow
[clsi-lis01]: https://clsi.org/media/2422/lis01a2e_sample.pdf
[clsi-lis02]: https://clsi.org/shop/standards/lis02/
[safety]: https://patientsafety.pa.gov/ADVISORIES/Pages/201810_HealthITandLaboratory.aspx
