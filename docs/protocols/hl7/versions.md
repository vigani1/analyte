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
