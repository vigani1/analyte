# config

**What it does:** Loads size-limited configuration and checks settings before use.

**Where it sits:** Receives machine definitions and site settings; gives validated choices to the host and mapping.

**What it must never do:** Open machine connections or silently guess unsupported combinations or missing settings.

**Main parts:**

- [schema.rs](src/schema.rs): Configuration fields.
- [validation.rs](src/validation.rs): Checks that settings make sense together.
- [stack.rs](src/stack.rs): Validated protocol combinations.
- [orders.rs](src/orders.rs): Explicit outgoing order settings.
- [law.rs](src/law.rs): Explicit LAW profile selection.

[Technical details and safety guidance](DETAILS.md).
