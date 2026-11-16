# analyte

**Kind: program.** Role folders group the work, with per-protocol files inside where needed; small roles currently stay in single files.

**What it does:** Runs the bridge on the lab PC and connects its building blocks.

**Where it sits:** Receives machine bytes and app orders; coordinates storage, replies, app delivery and operator actions.

**What it must never do:** Interpret clinical results, acknowledge unstored data or automatically resend uncertain machine orders.

**Main parts:**

- `main`: Program entry and operator commands.
- `machine`: One connection owner per machine.
- `orders`: Order lookup, send reservations and reconciliation.
- `runtime`: Startup, delivery workers and shutdown.
- `admin`: Protected local administration.
- `operator`: Alerts, status and recovery.
- `installation`: Installation settings and credentials.
- `intake`: Receipt storage, mapping, interpretation, revision and delivery commits, and failure parking.

[Technical details and safety guidance](DETAILS.md).
