# 0011. Result status progression

Status: accepted, 2026-11-15. Refines the revision rule of [ADR 0003](0003-clinical-safety-rules.md).

## Context

ADR 0003 assigns revisions from the result identity and its history: identical
content is a retransmission; different content is the next revision only with a
declared correction code or a withdrawal, otherwise a conflict that is parked.

Laboratory standards also define a normal life cycle for one result that is not
a correction. In IHE LAW (PaLM TF-2b Rev 11, Appendix C.7, Figure C.7-1) a
result may be preliminary and revised while preliminary, move from preliminary
or unverified to final, and only a change after final is a correction. HL7 v2
and ASTM result status codes describe the same kind of progression. Treating a
preliminary-to-final step as a conflict would park every normal final result
that followed a preliminary one.

## Decision

- Each syntax declares its result status life cycle once, in its own crate:
  which source statuses exist and which moves from a stored status to a newly
  received one are permitted progressions. The store applies that table; it
  does not know protocol codes itself.
- Under the same result identity, different content whose status move is a
  permitted progression (for LAW: P to P, P to R, P to F, R to R, R to F, C to
  C) is the next revision, delivered with its status. No correction code is
  needed for these moves.
- A change after a final status is a revision only with that standard's
  correction status (LAW: F to C), as ADR 0003 already requires.
- Any other move with different content (for example F to P, or a changed final
  without correction evidence) stays a conflict, parked for an operator.
- A status that its standard defines as not reportable (LAW: R, "not a
  candidate to be reported upstream") is stored and shown to operators but not
  delivered as a result; its later final version is delivered normally.
- Where a standard allows only one run of a test to reach final or corrected,
  a second run reaching final for the same order and test is a conflict, parked;
  Analyte never chooses between runs.

## Consequences

- Every syntax crate carries a status table with tests taken from its standard;
  a machine that deviates declares it as a named deviation in its config.
- Existing stored results keep their revisions; the rule applies to new
  receipts from the release that implements it.
