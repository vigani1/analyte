# config

**Kind: core.** Groups files by the data and operations they handle.

**What it does:** Loads size-limited configuration and checks settings before use.

**Where it sits:** Receives machine definitions and site settings; gives validated choices to the host and mapping.

**What it must never do:** Open machine connections or silently guess unsupported combinations or missing settings.

**Main parts:**

- `schema`: Configuration fields, including protocol-specific settings.
- `validation`: Checks that settings make sense together.
- `stack`: Validated protocol combinations.
- `orders`: Explicit outgoing order settings.
- `choices`: The choices a definition declares and the lab's selections of them (ADR 0032).
- `site`: The lab's site-file settings: retention, time zone and repeated results (ADR 0035).
- `schema/law`: Explicit LAW profile selection (re-exported at the crate root).

A software-version choice can pair a single documented band with
`checked = false` for "Other version (not checked)". A choice that offers
this value cannot select any settings. Use separate, explicitly selected
modes for any settings that vary by version. A wire selector's bands list
only the checked values.

[Technical details and safety guidance](DETAILS.md).
