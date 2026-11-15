# Declarative result mapping

Evidence: **documented**, with original synthetic fixtures. No real-machine or
simulator qualification is claimed.

`map_astm` takes one parsed LIS02 message. `map_hl7` takes an ORU R01 (2.3.1 or
2.5.1) or specimen-oriented OUL R22 (2.5.1). Both require an immutable validated
config and host-supplied receipt, receive time and revision. They perform no I/O.
The host retains raw bytes before mapping and parks errors or outputs whose
`requires_parking()` returns true. Only production content can be delivered;
training/debug content remains available for inspection.

The output retains common-model replacement units, correction evidence aligned
with their parts, positioned findings, and every note with its parent path.
Source nodes include their message occurrence separately from source sequence
numbers. An observation without a representable source code/system remains in
`unmapped`, with its original values and a finding. Nothing is silently dropped.

Values keep exact source text, including escape spellings, decimal precision,
qualitative/error tokens, repeated values and locale punctuation. Plain decimals
are interpreted conservatively; ASTM comparator prefixes remain beside their
original text. HL7 CE/CWE and SN use parsed component boundaries; unsupported or
malformed structured types are retained as Unsupported. Numeric syntax is never
expanded to locale decimals, exponents or inferred units. Unit mappings compare
the entire original unit field exactly. No numeric conversions occur.

Identity comes from source test code/system and explicit order, namespace,
sub-ID and run selectors. Missing selected evidence is unresolved. `sub_id =
"none"` deliberately supplies an empty sub-ID. Empty run declarations mean no
run evidence: different content under that identity is a conflict for the store,
not evidence for guessing a rerun. Ordered run components use UTF-8 byte-length
prefixes (`length:value` concatenated), so component boundaries cannot collide.
Correction status membership is independent of both the standard summary and
the caller's revision. No predecessor is required to map a correction.

Same-key OBXs within one order/specimen occurrence form a replacement unit;
source parts, statuses and repeated values stay ordered. Separate source orders
are not silently merged. Source notes remain attached to their parents, even
when a parent has no observation. QC classification comes only from config;
otherwise it is Unknown. Conflicting QC declarations require parking.

Sources: LIS02-A2 sections 5.6, 6.12, 8-10; HL7 2.3.1 chapter 7 sections 7.2.1, 7.3.2;
HL7 2.5.1 chapter 7 sections 7.3.1, 7.3.7, 7.4.2 and chapter 2A CE/CWE/SN/TS;
project safety rules 4-7 and plan section 6. Tests name these sources and use synthetic
messages, including corrections without originals, U/D/W status-only records,
missing evidence, different runs, mixed orders, notes, limits and property tests.

This is mapping, not full message/profile conformance validation. Unknown or
misplaced segments fail visibly. The implemented hierarchy handles MSH, PID,
ORC, OBR, SPM, SAC, OBX, NTE, PV1/PV2 and OUL TCD/SID; other segments require
review rather than guessed ownership. Original optional source fields stay in
the host's raw receipt. Encapsulated data interpretation, clinical decisions,
delivery, reconciliation, conflict detection and revision allocation belong
outside this crate. Carrier position remains absent because config declares no
selector for it. No default is invented.

Bounds: config message/record ceilings apply before source adaptation. Model
text and evidence limits apply to every constructed field/list. At most 256
parts belong to one replacement unit. A separate 64 MiB accounting budget
charges each observation/note 64 KiB plus sixteen times its ancestor/attached
note bytes before copying, bounding repeated context expansion. Budget exhaustion
returns a positioned error and no partial success; retain and park the receipt.

ASTM component decomposition and per-test value expectations are optional;
see [configuration](../config/README.md). Decomposition uses parsed R.3 component
boundaries, requires an exact part count and checks the actual message delimiters
as well as config validation. A located `ComponentParts` finding parks malformed
observations, retaining their values in `unmapped`; the protected raw receipt
retains the entire original identifier. Valid dilution text stays in
`Dilution.original`, with an exact-decimal factor only and independent application
evidence. Per-test `ValueKindMismatch` findings park values without replacing their
original text or changing protocol interpretation. Omitted settings preserve the
existing ASTM and HL7 paths. These features remain documented evidence only.
