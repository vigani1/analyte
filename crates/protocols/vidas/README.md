# vidas

**Kind: protocol.** Reads tagged message fields in the syntax module and writes them in the encode module.

**What it does:** Reads and writes tagged messages for the VIDAS PC Literal format.

**Where it sits:** Receives message content after framing; supplies structured tags to mapping and outgoing content to the host.

**What it must never do:** Frame packets, run conversations, access connections or map results; never guess missing identity.

**Main parts:**

- `syntax`: Tagged message fields.
- `encode`: Outgoing tagged messages, checked by the same parser.

[Technical details and safety guidance](../../../docs/protocols/vidas/README.md), in the protocol notes.
