# 0011. Result status progression

Status: accepted, 2026-11-15. Refines the revision rule of [ADR 0003](0003-clinical-safety-rules.md).

## Context

ADR 0003 makes different content under one result identity a conflict unless
it carries a correction code. But standards define a normal life cycle for one
result, for example preliminary to final, that is not a correction. Treating it
as a conflict would park every normal final result that followed a preliminary
one.

## Decision

- Each syntax crate declares its result status life cycle once: its source
  statuses and which moves from a stored status to a new one are normal
  progressions. The store applies that table without knowing protocol codes.
- Under one result identity, different content with a permitted progression is
  the next revision, delivered with its status; no correction code is needed.
- A change after final is a revision only with that standard's correction
  status, as ADR 0003 requires. Any other move with different content is a
  conflict, parked.
- A status its standard marks as not reportable is stored and shown to
  operators but not delivered; its later final version is delivered normally.
- Where a standard allows only one run of a test to reach final, a second final
  run for the same order and test is a conflict, parked; Analyte never chooses
  between runs.

The LAW table and its sources are in
[LAW decisions](../protocols/law/decisions.md#result-status-progression).

## Why

The standards themselves separate progression from correction; following them
keeps normal results flowing while every real conflict still stops.

## Consequences

- Every syntax crate carries a status table with tests from its standard; a
  machine that deviates declares a named departure in its config.
- Stored results keep their revisions; the rule applies to new receipts.
