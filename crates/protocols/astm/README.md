# astm

**Kind: protocol.** Separates message parsing, typed field access and version rules into named files.

**What it does:** Reads and writes structured ASTM records within size limits.

**Where it sits:** Receives assembled message bytes from the host and supplies records and named fields to mapping.

**What it must never do:** Access connections or translate records into the common result model.

**Main parts:**

- `syntax`: Record grammar.
- `views`: Named access to record fields.
- `codes`: Standard code tables kept beside the grammar.

[Technical details and safety guidance](DETAILS.md).
