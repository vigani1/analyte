# store

**Kind: core.** Parts are grouped by role; small parts currently stay in single files and grow into role folders when needed.

**What it does:** Keeps original evidence, result history, pending deliveries and order state durably.

**Where it sits:** Receives host storage requests and returns committed outcomes used before protocol acceptance or delivery.

**What it must never do:** Discard evidence, overwrite originals, log payloads or replay a reserved machine command.

**Main parts:**

- `lib`: Storage operations and specific errors.
- `receipts`: Retained arrivals.
- `revisions`: Result history saved together with its state changes.
- `outbox`: Pending app deliveries.
- `machine_orders`: Machine command reservations.
- `polls`: Saved poll intent and operator reconciliation.
- `restore`: Restore that preserves the displaced store.

[Technical details and safety guidance](DETAILS.md).
