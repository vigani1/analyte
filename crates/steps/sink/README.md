# sink

**Kind: step.** Groups delivery, polling and alert operations by role.

**What it does:** Delivers results to lab apps over HTTP, polls apps for order changes and sends operational alert webhooks.

**Where it sits:** Receives pending deliveries and saved order progress from the store; sends app requests and retains their outcomes.

**What it must never do:** Talk to analyzers, make clinical decisions or change a delivery identity when retrying.

**Main parts:**

- `delivery`: Delivery work with stable retry identity.
- `destination`: Validated destinations and credentials.
- `polling`: Order polling from the lab app.
- `webhook`: Validated alert URLs and bounded HTTPS attempts; the program owns scheduling and the store owns immutable events.

[Technical details and safety guidance](DETAILS.md).
