# poct

**Kind: protocol.** Separates bounded XML grammar, named field access and reply builders.

**What it does:** Reads and builds POCT Basic Profile messages while retaining the source values.

**Where it sits:** Receives XML message bytes; supplies structured messages to the host and mapping.

**What it must never do:** Access connections, choose acceptance or resolve trusted identities; the host owns storage and clocks.

**Main parts:**

- `syntax`: Bounded XML grammar.
- `views/data`: Values with original spelling.
- `views/message`: Required Basic Profile message fields.
- `syntax/structure`: Message structure checks.
- `encode`: Reply message construction and XML escaping.

[Technical details and safety guidance](DETAILS.md).
