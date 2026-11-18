# 0003. Clinical safety rules

Status: accepted, 2026-11-15

## Context

A lost, duplicated or silently changed result can harm a patient. Common
middleware failures are acknowledging data that was not saved, dropping
messages that fail to parse, treating a correction as a duplicate, and
answering "no orders" when the order source was unreachable.

## Decision

The ten clinical safety rules in [AGENTS.md](../../AGENTS.md) are binding and
keep their numbers here, so "ADR 0003 rule 8" means AGENTS rule 8:

1. Store before acknowledging; refuse visibly when storage fails or the disk
   is nearly full. Each protocol's exact step is in
   [ADR 0010](0010-acknowledgment-boundaries.md).
2. Never drop data: failed parses, unknown tests and undeliverable results
   are parked with a reason for an operator.
3. Keep the original bytes with the driver and config version that read them;
   reprocessing adds a version.
4. Three identities: the receipt, the delivery (same ID on every retry) and
   the result with its revision. Never invent a result identity: when the
   machine sends too little to tell a retransmission, a correction and a new
   run apart, the receipt stays marked unresolved. Different content under the
   same trusted identity is an error.
5. Values and sample IDs stay as written. 6. Units and dilution are never
   guessed. 7. Everything is bounded. 8. Order states stay separate and orders
   are never resent by a retry or replay. 9. A failed lookup is not "no
   orders". 10. No patient data in ordinary logs.

**Revisions** are assigned by the store, in one transaction with the
interpretation, from the identity (code, sub-ID, run, order) and the history:

- no earlier result with that identity: a new result, revision 0, even if the
  source calls it a correction (the original was never seen);
- same identity and identical content: a retransmission, with no new revision
  and no new delivery;
- same identity, different content, and a declared correction code or a
  withdrawal: the next revision ([ADR 0011](0011-result-status-progression.md)
  adds normal status progressions);
- anything else: a conflict, parked for an operator, never guessed.

Changing any of these rules needs a new ADR.

## Why

Byte-hash duplicate detection cannot tell a correction from a repeat; explicit
identities and store-assigned revisions can.

## Consequences

- Storage time must fit each machine's acknowledgment deadline on the target
  PC, and crashes just before and after every positive acknowledgment are
  tested.
- Receiving apps store before answering and treat a repeated delivery ID as
  done; the contract says so.
- The store grows and needs retention, backup and disk-space alerts.
