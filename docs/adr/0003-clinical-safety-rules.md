# 0003. Clinical safety rules

Status: accepted, 2026-11-15

## Context

A lost, duplicated or silently changed result can harm a patient. Common
middleware failures include acknowledging data that was not saved, dropping
messages that fail to parse, treating a corrected result as a duplicate, and
answering "no orders" when the order source was unreachable.

## Decision

The ten clinical safety rules in [AGENTS.md](../../AGENTS.md) are binding.
The core ones:

- **Store before acknowledging.** A machine is told "got it" only after the
  data is durably stored. If storage fails or the disk is nearly full, Analyte
  refuses visibly.
- **Never drop data.** Failed parses, unknown tests and undeliverable results
  are parked with a reason and shown to an operator, never deleted.
- **Keep the original.** Raw bytes are stored with the driver and config
  version that read them. Reprocessing creates a new version.
- **Three identities** instead of byte-hash duplicate detection: the receipt
  (what arrived from the machine), the delivery (same ID on every retry) and
  the result (a stable observation identity plus its revision). A correction
  is a new revision; a rerun is a new result.
- **Lookup failure is not "no orders".**

Changing any of these rules needs a new ADR.

## Consequences

- Storage latency must fit inside each machine's acknowledgment deadline,
  measured on the target PC.
- Receiving apps must store before answering and treat a repeated delivery
  ID as already done; the contract says so.
- The store grows and needs retention, backup and disk-space alerts.
