# astm

**What it does:** Reads and writes structured ASTM records within size limits.

**Where it sits:** Receives assembled message bytes from the host and supplies records and named fields to mapping.

**What it must never do:** Access connections or translate records into the common result model.

**Main parts:**

- [syntax.rs](src/syntax.rs): Record grammar.
- [views.rs](src/views.rs): Named access to record fields.
- [codes.rs](src/codes.rs): Standard code tables kept beside the grammar.

[Technical details and safety guidance](DETAILS.md).
