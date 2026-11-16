# analyte

**What it does:** Runs the bridge on the lab PC and connects its building blocks.

**Where it sits:** Receives machine bytes and app orders; coordinates storage, replies, app delivery and operator actions.

**What it must never do:** Interpret clinical results, acknowledge unstored data or automatically resend uncertain machine orders.

**Main parts:**

- [main.rs](src/main.rs): Program entry and operator commands.
- [machine.rs](src/machine.rs): One connection owner per machine.
- [orders.rs](src/orders.rs): Order lookup, send reservations and reconciliation.
- [runtime.rs](src/runtime.rs): Startup, delivery workers and shutdown.
- [admin.rs](src/admin.rs): Protected local administration.
- [operator.rs](src/operator.rs): Alerts, status and recovery.
- [installation.rs](src/installation.rs): Installation settings and credentials.
- [evidence.rs](src/evidence.rs): Protected evidence processing.

[Technical details and safety guidance](DETAILS.md).
