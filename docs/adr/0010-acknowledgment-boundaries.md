# 0010. Acknowledgment boundaries for every protocol

Status: accepted, 2026-11-15. Extends [ADR 0003](0003-clinical-safety-rules.md).
Extended by [ADR 0034](0034-record-only-connections.md) (the record-only row).
Amended 2026-11-26: HL7 v2 accepts a stored upload whose interpretation is held.

## Context

ADR 0003 says to store before acknowledging. Analyte speaks six protocols, and
each says "got it", or takes a destructive step, in its own way.

## Decision

No protocol takes its positive step before the data named here is durably
kept:

| Protocol | Durable before | Positive step it guards |
| --- | --- | --- |
| ASTM (LIS01/LIS02) | Free capacity reserved; each data frame committed with its transfer identity | ENQ accepted; each frame's ACK |
| HL7 v2 over MLLP | The complete raw message; its interpretation outcome, accepted or held, for AA | AA (original mode); CA then AA (enhanced mode) |
| IHE LAW LAB-29 | The raw result and its interpretation | ACK^R22 AA |
| POCT01-A2 | The complete raw receipt and its interpretation outcome | The observation acknowledgment |
| VIDAS Literal | The complete transfer | The data acknowledgment |
| Autolyser | The complete, validated poll response | The destructive read command (R); an uncertain R is kept for reconciliation, never repeated automatically |

A storage failure or nearly full disk at any of these steps refuses visibly in
that protocol's own terms and never sends the positive step.

## Why

One table makes the safety boundary of every protocol checkable in one place.

## Amendment 2026-11-26: held HL7 uploads are accepted

Owner decision. Once Analyte has durably stored an HL7 v2 upload and the
interpretation outcome for it, it answers AA in original mode, or CA and then
AA in enhanced mode, even when that outcome is held for an operator: missing
identity evidence, a mapping problem such as an observation without OBX-3, a
conflicting result, a QC row without a measurement identity, or an
interpretation too large for the store. MSA-2 still echoes MSH-10. The held
item stays parked with its specific reason and appears in Needs review as
before.

Why: in HL7 v2 a positive acknowledgment says the receiver has taken
responsibility for the message, and enhanced mode even separates "safely
stored" (CA) from the application answer. Lab middleware and interface engines
acknowledge once a message is safe and work through problems in their own
review queue. Answering AE for a message we already hold makes analyzers
resend it and show it as not received, so staff could rerun tests for no
reason.

What still refuses, unchanged:

- storage failure or a nearly full disk: AR in original mode, CE in enhanced
  mode, never AA or CA;
- bytes the HL7 parser rejects (no usable MSH, a bad escape, an unsupported
  application acknowledgment channel): kept and parked with no reply;
- an HL7 syntax or data-type error first found while reading a parsed
  message: AE with the ERR location;
- a message type or event the result connection does not read (anything but
  ORU^R01, or OUL^R22 in 2.5.1): AE located at MSH-9;
- a full queue, an unmatched or stale exchange, and a host phase that times
  out: their current refusals.

IHE LAW (LAB-29) keeps its own acknowledgment rules and is not changed by this
amendment. Record-only connections keep their row from ADR 0034.

## Consequences

- Each row has a named regression test, listed in
  [docs/protocols/README.md](../protocols/README.md#durable-custody-boundaries).
- A new protocol or vendor format adds its row here, through a new ADR, before
  it is wired into the program.
