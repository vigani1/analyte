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

A software-version choice can pair a single documented band with
`checked = false` for "Other version (not checked)". A choice that offers
this value cannot select any settings. Use separate, explicitly selected
modes for any settings that vary by version. A wire selector's bands list
only the checked values. Each token in `[software_version.bands]` must
match a `firmware` entry under `[[evidence.capabilities]]` exactly.

See the [machine authoring guide](../../../docs/develop/adding-a-machine.md#automatic-handling-what-to-write-and-how-analyte-uses-it)
for identity recipes, version selectors, departures and lab choices.

[Technical details and safety guidance](DETAILS.md).
