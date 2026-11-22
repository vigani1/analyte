# 0016. AWOS status notifications

Status: accepted, 2026-11-16 (approved by the project lead); implemented

## Context

IHE LAW lets an analyzer send a LAB-29 message with no observations to report
that work is scheduled (SC), in progress (IP) or cancelled by the analyzer (CA).
Analyte used to park these as unsupported, burying real problems under normal
progress. The order model has no "in progress" state, and contract v1 has a
closed list of order states ([research](../research/other/law-status-notifications.md)).

## Decision

- A status-only LAB-29 is execution evidence kept beside the order, not a
  result: each order item gets an optional Scheduled, InProgress or
  AnalyzerCancelled projection with the evidence behind it.
- It never changes the order's command state, creates a result or revision, or
  sends anything to the app. Contract v1 stays unchanged.
- Custody before AA: store the exact message, match it to retained order items
  (never a guessed sample, test or time match), then commit the disposition,
  item links and projection in one transaction. Only then answer AA. Unknown or
  contradictory work rejects the whole message with AR; no partial update
  survives.
- An analyzer CA does not confirm or create an app cancellation and never
  cancels sibling items. It blocks new automatic commands for that item and is
  shown to an operator for reconciliation.
- No notice, retry, replay, reprocess or recovery ever sends work or a
  cancellation to a machine.
- Repeats reuse their committed disposition; backwards or unclear ordering is
  parked, never sorted by guessed times.
- These facts appear only on the protected admin page.

The transition table, matching, ordering and retention rules are in
[LAW decisions](../protocols/law/decisions.md#status-only-lab-29-custody).

## Why

Execution status and command state are different facts. Keeping them apart
stops normal progress from parking while never blurring evidence into a false
acceptance, cancellation or result.

## Consequences

- App users cannot see these facts remotely; a LAW deployment needs a named
  admin responder. Sending them to apps needs a separate contract decision.
- These receipts and their linked orders are not purged until a tested
  retention rule covers them ([ADR 0013](0013-retention-of-delivered-data.md)).
- Old `UnsupportedLawStatusOnly` receipts need explicit audited reprocessing.
- No evidence level is raised; an official simulator run covering these notices
  is still required before any LAW machine is enabled.
