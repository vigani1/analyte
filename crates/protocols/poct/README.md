# poct

**Kind: protocol.** Separates message parsing, typed field access and version rules into named files.

**What it does:** Reads and builds POCT Basic Profile messages while retaining the source values.

**Where it sits:** Receives XML message bytes; supplies structured messages to the host and mapping.

**What it must never do:** Access connections, choose acceptance or resolve trusted identities; the host owns storage and clocks.

**Main parts:**

- `syntax`: Bounded XML grammar.
- `views/data`: Values with original spelling.
- `views/message`: Required Basic Profile message fields.
- `syntax/structure`: Message structure checks.
- `messages`: Reply message construction and XML escaping.

[Technical details and safety guidance](DETAILS.md).
