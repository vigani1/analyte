# fixtures

**Kind: testing.** Shares synthetic peer messages, bounded waits and controlled free-space probes.

**What it does:** Supplies reusable test inputs and controlled I/O probes.

**Where it sits:** Consumers use dev-dependencies; crate-local setup stays in each crate. This crate is not published.

**What it must never do:** Contact real analyzers or lab apps, or contain patient data.

**Main parts:**

- `lib.rs`: Shared synthetic fixtures and test probes.
- `poct`: Synthetic POCT messages shared by protocol tests.
