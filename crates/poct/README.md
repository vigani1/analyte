# poct

**What it does:** Reads and builds POCT Basic Profile messages while retaining the source values.

**Where it sits:** Receives XML message bytes; supplies structured messages and duplicate comparisons to the host and mapping.

**What it must never do:** Access connections, choose acceptance or resolve trusted identities; the host owns storage and clocks.

**Main parts:**

- [syntax.rs](src/syntax.rs): Bounded XML grammar.
- [data.rs](src/data.rs): Values with original spelling.
- [message.rs](src/message.rs): Required Basic Profile message fields.
- [structure.rs](src/structure.rs): Message structure checks.
- [encode.rs](src/encode.rs): Reply message construction and XML escaping.
- [duplicate.rs](src/duplicate.rs): Bounded observation duplicate comparison.

[Technical details and safety guidance](DETAILS.md).
