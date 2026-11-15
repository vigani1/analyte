# 0010. Acknowledgment boundaries for every protocol

Status: accepted, 2026-11-15. Extends [ADR 0003](0003-clinical-safety-rules.md).

## Context

ADR 0003 fixes the store-before-acknowledge boundary for ASTM and HL7. Analyte
now also speaks IHE LAW, POCT01-A2, VIDAS Literal and the Autolyser format.
Each needs the same rule stated at its own protocol step, so that no protocol
can tell a machine "got it", or take a destructive step, before the data is
durably kept.

## Decision

The boundary per protocol is:

| Protocol | Durable before | Positive step it guards |
| --- | --- | --- |
| ASTM (LIS01/LIS02) | Free capacity reserved; each data frame committed with its transfer identity | ENQ accepted; each frame's ACK |
| HL7 v2 over MLLP | The complete raw message committed; interpretation stored for AA | AA (original mode); CA then AA (enhanced mode) |
| IHE LAW LAB-29 | The raw result and its interpretation | ACK^R22 AA |
| POCT01-A2 | The complete raw receipt and its interpretation outcome | The observation acknowledgment |
| VIDAS Literal | The complete transfer | The data acknowledgment |
| Autolyser | The complete, validated poll response | The destructive read command (R); an uncertain R is kept for reconciliation, never repeated automatically |

Storage failure or a nearly full disk at any of these steps refuses visibly in
that protocol's own terms; it never sends the positive step.

## Consequences

- Each row has a named regression test; the current list is in
  [docs/protocols/README.md](../protocols/README.md#durable-custody-boundaries).
- A new protocol or vendor format states its boundary here (by a new ADR)
  before it is wired into the program.
