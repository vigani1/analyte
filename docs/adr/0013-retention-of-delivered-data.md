# 0013. Retention of delivered data

Status: accepted, 2026-11-15; items 6 and 7 added 2026-11-16. Refines
[ADR 0003](0003-clinical-safety-rules.md) rule 3 (keep the original).

## Context

Keeping every raw message for ever makes the store, startup checks and backups
grow without end, and GDPR asks that data be kept no longer than needed. The
lab app holds the clinical record; Analyte's copy is a safety copy for the time
in which a delivery, mapping, correction or dispute can still go wrong.

## Decision

1. **The period is a required site setting:** 1 to 3650 days, or `keep`. There
   is no default; the guide recommends 90 days.
2. **Only finished data is eligible:** a receipt whose results were all
   delivered and acknowledged, with nothing parked, unresolved, open, uncertain
   or held, after the period since its last acknowledgment. Orders become
   eligible the period after they end as done, rejected or cancelled.
3. **A tombstone stays:** raw bytes, parsed content and payloads go; one row per
   item keeps the identities, original digest, machine, driver and config
   version and key times, with no patient data. A late resend of purged data
   is still recognized as a repeat.
4. **Purging is bounded, visible and audited:** small background batches, each
   in its own transaction and time limit, paused during backup and restore,
   with an audit entry per run. The admin page shows the policy, last run and
   oldest item. Space returns to the system without a long lock.
5. **Purged means gone from the live store:** reprocess and resend refuse with a
   specific reason. Older backups keep the data until their rotation.
6. **Tombstones keep keyed digests:** parts of a result identity that could
   point to a patient are kept only as keyed digests. The key is an
   installation secret, required for any period other than `keep`; the store
   records only its fingerprint, and a changed key refuses startup. Key
   rotation is not supported.
7. **Operator holds:** a hold on a receipt or an order (covering its receipts)
   blocks purging until an explicit, audited release; holds never expire.

The operator steps are in [recovery](../operations/recovery.md#retention-under-adr-0013).

## Why

A fixed, explicit period keeps the store bounded while nothing unfinished, and
nothing under investigation, can ever be purged.

## Consequences

- Rule 2 (never drop data) still holds, and the purge itself is audited.
- Before a pilot, each deployment writes down its period with its backup and
  restore targets (plan §11).
