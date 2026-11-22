# 0015. Typed, evidenced departures for individual machines

Status: accepted, 2026-11-16; delivery scope set by the project lead the same
day

## Context

Strict HL7 report checks and LIS01 record boundaries correctly refuse input
that some real analyzers send. [Research](../research/machines/machine-departures.md)
found documented packed-frame modes and a vendor example that omits report
status, but no general license to treat a missing status as final. Loosening
the shared standard code would hide which rules a lab actually enabled.

## Decision

- A machine's departures from a standard live in its config as variants of one
  closed list (`MachineDeparture`). No free field path, pattern, script or
  user-defined status mapping can select behavior.
- The list holds two missing-status rules for generic HL7 patient result
  uploads (absent OBR-25, absent OBX-11 means final), packed ASTM records in one
  frame, and the existing HL7 and ASTM compatibility settings.
- Each declaration names its exact scope (model, firmware, mode, edition,
  direction, message kind), its evidence and its evidence level. Config loading
  refuses anything missing, unresolved or contradictory. Nothing is enabled by
  default.
- Every other check stays strict. A departure never changes a source status,
  never weakens a clinical safety rule and never infers release, normality, QC
  acceptance or completion.
- Every interpretation records which departures were enabled and which actually
  applied. The admin page shows both and labels a final-from-documentation
  status apart from a received final.
- **Delivery (project lead, 2026-11-16):** packed ASTM records deliver on
  contract v1 unchanged. A status derived from an HL7 departure parks with
  `departure-derived status requires a contract version that carries departure evidence`
  until a future contract can carry that evidence.

The variant table and load rules are in
[adding a machine](../develop/adding-a-machine.md#declare-departures-do-not-program-around-them);
the protocol behavior is in the [HL7](../protocols/hl7/decisions.md#named-missing-status-interpretations-and-contract-v1-parking)
and [ASTM](../protocols/astm/decisions.md#evidenced-inbound-record-packing)
decisions.

## Why

A closed, typed list with evidence per declaration lets labs support documented
differences without weakening the standard defaults for everyone else.

## Consequences

- Some machines stay parked until a document establishes what their omission
  means; a recording alone cannot.
- Standard tests stay unchanged; each rule has paired strict and departure
  tests. Synthetic tests raise no evidence level.
- No plugin system, new protocol family or transformation language is added.
