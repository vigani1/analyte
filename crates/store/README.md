# store

**What it does:** Keeps original evidence, result history, pending deliveries and order state durably.

**Where it sits:** Receives host storage requests and returns committed outcomes used before protocol acceptance or delivery.

**What it must never do:** Discard evidence, overwrite originals, log payloads or replay a reserved machine command.

**Main parts:**

- [lib.rs](src/lib.rs): Storage operations and specific errors.
- [receipts.rs](src/receipts.rs): Retained arrivals.
- [revisions.rs](src/revisions.rs): Result history saved together with its state changes.
- [outbox.rs](src/outbox.rs): Pending app deliveries.
- [machine_orders.rs](src/machine_orders.rs): Machine command reservations.
- [polls.rs](src/polls.rs): Saved poll intent and operator reconciliation.
- [restore.rs](src/restore.rs): Restore that preserves the displaced store.

[Technical details and safety guidance](DETAILS.md).
