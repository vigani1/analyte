# sink

**What it does:** Delivers results to lab apps over HTTP and polls apps for order changes.

**Where it sits:** Receives pending deliveries and saved order progress from the store; sends app requests and retains their outcomes.

**What it must never do:** Talk to analyzers, make clinical decisions or change a delivery identity when retrying.

**Main parts:**

- [delivery.rs](src/delivery.rs): Delivery work with stable retry identity.
- [destination.rs](src/destination.rs): Validated destinations and credentials.
- [polling.rs](src/polling.rs): Order polling from the lab app.

[Technical details and safety guidance](DETAILS.md).
