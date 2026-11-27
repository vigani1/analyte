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
- `schema/law`: Explicit LAW profile selection (re-exported at the crate root).

The machine guide gives exact TOML keys and synthetic examples for identity
recipes, lab choices, software versions and departures. The loader validates
all offered choice combinations. A software version choice with
`checked = false` for "Other version (not checked)" identifies software
only. Keep any settings that vary by version in separate mode choices.
With a wire selector and a software version choice, `software_version.bands`
has one entry per checked value. Every exact wire token must also appear
as `firmware` in `evidence.capabilities`.
[Automatic handling](../../../docs/develop/adding-a-machine.md#automatic-handling-what-to-write-and-how-analyte-uses-it)

[Technical details and safety guidance](DETAILS.md).
