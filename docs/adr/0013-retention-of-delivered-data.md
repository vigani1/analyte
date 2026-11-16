# 0013. Retention of delivered data

Status: accepted, 2026-11-15. Refines [ADR 0003](0003-clinical-safety-rules.md)
rule 3 (keep the original).

## Context

ADR 0003 keeps every raw message with the driver and config version that read
it, so a result can be reprocessed or disputed. Without an end, the store grows
for ever: a lab with six machines collects gigabytes a year, and startup checks
and backups grow with it. GDPR asks for data to be kept no longer than needed,
and the plan leaves the period to each deployment. The lab app, not Analyte,
holds the clinical record. Analyte's copy is a safety copy for the time in which
something can still go wrong: a failed delivery, a mapping error found later, a
correction or a dispute.

## Decision

1. **The period is a required site setting.** A deployment states either a
   number of days (1 to 3650) or `keep` (never purge). There is no default; a
   config without the setting fails validation. The installation guide
   recommends 90 days.
2. **Only finished data is eligible.** A receipt can be purged when every result
   it produced has been delivered and acknowledged by the lab app, nothing from
   it is parked or unresolved, no order or poll intent that refers to it is in
   an open or uncertain state, no operator hold covers it, and the period has
   passed since its last acknowledgment. Orders are eligible once they are done,
   rejected or cancelled and the period has passed since that final state.
3. **A tombstone stays.** Purging removes raw bytes, frames, parsed content and
   delivery payloads. It keeps one row per item with the receipt, delivery and
   result identities, the original digest, the machine, the driver and config
   version, and the received, acknowledged and purged times. Duplicate
   detection keys survive as keyed digests, so a late resend of purged data is
   still recognised as a repeat, never as a new result. A tombstone holds no
   patient data.
4. **Purging is bounded, visible and audited.** It runs in the background in
   small batches, each in its own transaction and within a time limit, and it
   pauses during backup and restore. Every run writes an audit entry with the
   policy, the counts and the identity range. The operator page shows the policy,
   the last run and the oldest retained item. Freed space is returned to the
   operating system without a long exclusive lock.
5. **After purging, the original is gone from the live store.** Reprocessing or
   resending a purged item is refused with a specific reason. Older backups
   still hold it until the deployment's backup rotation removes them; the
   installation guide says so.

6. **Tombstone identities are keyed digests (amended 2026-11-16).** Receipt and
   delivery identities are UUIDs that Analyte generates; tombstones keep them
   as they are. A result identity contains source order, sample and run
   identifiers, which can be linked to a patient, so the tombstone keeps only
   a keyed digest of each such component plus the revision number. Duplicate
   detection for purged data compares keyed digests. The digest key is a
   host credential held in the installation secrets, never in the store; the
   store records only the key's fingerprint. A configured period other than
   `keep` requires the key. If the key's fingerprint differs from the one
   recorded with existing tombstones, startup refuses with a specific error,
   because a changed key would make a late resend look like a new result
   (rule 4). Key rotation is not supported in this version.
7. **Operator holds (amended 2026-11-16).** An operator can place a hold on a
   receipt or on an order. A hold on an order also covers every receipt
   linked to it. Placing and releasing a hold each write an audit entry with
   the actor and a reason, from the operator page or the command line. A held
   item is never purged. Holds have no expiry; only an explicit release ends
   them.

## Consequences

- Rule 2 (never drop data) still holds: nothing unfinished is ever purged, and
  the purge itself is audited.
- The store schema gains tombstones and keyed duplicate digests; migrations
  convert existing rows.
- Before a pilot, each deployment writes down its period with its backup and
  restore targets (plan §11).
