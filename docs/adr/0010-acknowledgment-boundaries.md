# 0010. Acknowledgment boundaries for every protocol

Status: accepted, 2026-11-15. Extends [ADR 0003](0003-clinical-safety-rules.md).

## Context

ADR 0003 says to store before acknowledging. Analyte speaks six protocols, and
each says "got it", or takes a destructive step, in its own way.

## Decision

No protocol takes its positive step before the data named here is durably
kept:

| Protocol | Durable before | Positive step it guards |
| --- | --- | --- |
| ASTM (LIS01/LIS02) | Free capacity reserved; each data frame committed with its transfer identity | ENQ accepted; each frame's ACK |
| HL7 v2 over MLLP | The complete raw message; its interpretation for AA | AA (original mode); CA then AA (enhanced mode) |
| IHE LAW LAB-29 | The raw result and its interpretation | ACK^R22 AA |
| POCT01-A2 | The complete raw receipt and its interpretation outcome | The observation acknowledgment |
| VIDAS Literal | The complete transfer | The data acknowledgment |
| Autolyser | The complete, validated poll response | The destructive read command (R); an uncertain R is kept for reconciliation, never repeated automatically |

A storage failure or nearly full disk at any of these steps refuses visibly in
that protocol's own terms and never sends the positive step.

## Why

One table makes the safety boundary of every protocol checkable in one place.

## Consequences

- Each row has a named regression test, listed in
  [docs/protocols/README.md](../protocols/README.md#durable-custody-boundaries).
- A new protocol or vendor format adds its row here, through a new ADR, before
  it is wired into the program.
