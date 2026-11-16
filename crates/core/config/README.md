# config

**Kind: core.** Parts are grouped by role; small parts currently stay in single files and grow into role folders when needed.

**What it does:** Loads size-limited configuration and checks settings before use.

**Where it sits:** Receives machine definitions and site settings; gives validated choices to the host and mapping.

**What it must never do:** Open machine connections or silently guess unsupported combinations or missing settings.

**Main parts:**

- `schema`: Configuration fields, including protocol-specific settings.
- `validation`: Checks that settings make sense together.
- `stack`: Validated protocol combinations.
- `orders`: Explicit outgoing order settings.
- `schema/law`: Explicit LAW profile selection (re-exported at the crate root).

[Technical details and safety guidance](DETAILS.md).
