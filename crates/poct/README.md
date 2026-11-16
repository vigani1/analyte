# poct

**Kind: protocol.** One file per job, such as `syntax.rs`, `views.rs` or `version.rs`, where that job exists.

**What it does:** Reads and builds POCT Basic Profile messages while retaining the source values.

**Where it sits:** Receives XML message bytes; supplies structured messages and duplicate comparisons to the host and mapping.

**What it must never do:** Access connections, choose acceptance or resolve trusted identities; the host owns storage and clocks.

**Main parts:**

- `syntax`: Bounded XML grammar.
- `data`: Values with original spelling.
- `message`: Required Basic Profile message fields.
- `structure`: Message structure checks.
- `encode`: Reply message construction and XML escaping.
- `duplicate`: Bounded observation duplicate comparison.

[Technical details and safety guidance](DETAILS.md).
