# astm

**Kind: protocol.** Separates record grammar, named field access, standard codes and order writers.

**What it does:** Reads and writes structured ASTM records within size limits.

**Where it sits:** Receives assembled message bytes from the host and supplies records and named fields to mapping.

**What it must never do:** Access connections or translate records into the common result model.

**Main parts:**

- `syntax`: Record grammar.
- `views`: Named access to record fields.
- `codes`: Standard code tables kept beside the grammar.
- `query`: Reads incoming query records; it is not encoding.
- `encode`: Order records and their header, patient and termination envelope.

[Technical details and safety guidance](DETAILS.md).
