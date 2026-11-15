# HL7 versions

Only **2.3.1 and 2.5.1** are implemented. MLLP Release 1 is the framing
profile. Evidence remains documented. Documents for 2.6, 2.8.2 and 2.9.1
are available and planned; they are not implemented versions.

| Edition | Difference that matters | Source |
| --- | --- | --- |
| 2.3.1 | Legacy laboratory transactions; no OBX-21 observation identifier | Existing scope and field evidence in [checklist](checklist.md) |
| 2.5.1 | Adds the selected OML/OUL/QBP transaction profiles; OBX-21 is reserved, not a trusted result identity | Existing scope and field evidence in [checklist](checklist.md) |
| 2.3, 2.4, 2.5 | Earlier messages fall under the compatibility contract, not separate implemented edition profiles | HL7 2.5.1 section 2.8-2.8.2, pp.2-20-2-21 |
| 2.6, 2.8.2, 2.9.1 | Planned; differences have not been implemented or qualified | [Plan](../../plan.md) scope and ADR 0012 section 3 |

## Compatibility rule

HL7 2.5.1 section 2.8 (pp.2-20 onward) requires newer receivers to understand
older messages and older receivers to tolerate new information. Additions
and changes must preserve existing meanings: extra fields/components extend
the existing structure, and the earlier part of a changed datatype must keep
its earlier interpretation. Changes outside the stated rules cannot be
assumed compatible. Local extensions are a separate agreement (section 2.11).
This covers 2.3, 2.4 and 2.5 within the declared message scope; it does not
promise every transaction from those editions or add version enum variants.
Unexpected information must not by itself cause application failure under
section 2.8.1; Analyte retains original bytes even when it cannot use a constituent.
Unsupported message semantics, invalid headers and unsupported configured
versions remain explicit failures. Compatibility never supplies missing
identity or clinical meaning. See the checklist for current evidence gaps.

## Implemented grammar rule owner

`crates/hl7/src/version.rs` owns edition selection and the grammar difference
table. The refactor preserves existing acceptance, including the shared
timestamp precision forms and the explicit LAW and UTF-8 policies.

| Rule | Sources | Regression coverage |
| --- | --- | --- |
| Response trigger omission and MSG lengths | 2.3.1 section 2.24.1.9; 2.5.1 sections 2.15.9.9, 2.A.44 | control message type components |
| Required message time | 2.3.1 section 2.24.1.7; 2.5.1 section 2.15.9.7 | control requires valid message time |
| Timestamp precision (shared acceptance) | 2.3.1 section 2.8.44; 2.5.1 sections 2.A.22, 2.A.77 | query keeps timestamp precision and offsets |
| Coded component lengths | 2.3.1 section 2.8.3; 2.5.1 sections 2.A.6, 2.A.13 | datatype coded component lengths; LAW CWE cases |
| URI universal ID code | 2.3.1 section 2.8.20; 2.5.1 section 2.A.33, table 0301 | query URI code is edition specific |
| XCN component count and lengths | 2.3.1 section 2.8.49; 2.5.1 section 2.A.86 | query XCN extended components and lengths |
| Family name count, required surname and lengths | 2.3.1 section 2.8.49.2; 2.5.1 section 2.A.30 | query XCN extended components and lengths |
| Name type codes | 2.3.1 section 2.8.49.10; 2.5.1 section 2.A.86.10, table 0200 | query name codes |
| Order sequence required components and lengths | 2.3.1 section 4.4; 2.5.1 section 2.A.50 | datatype order sequence required fields and lengths |
| Interval component lengths | 2.3.1 section 2.8.35; 2.5.1 section 2.A.63 | query TQ component constraints |
| Timing component lengths | 2.3.1 section 4.4; 2.5.1 section 2.A.76 | query TQ component constraints |
