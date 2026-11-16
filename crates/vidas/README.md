# vidas

**Kind: protocol.** One file per job, such as `syntax.rs`, `views.rs` or `version.rs`, where that job exists.

**What it does:** Reads and writes tagged messages for the VIDAS PC Literal format.

**Where it sits:** Receives message content after framing; supplies structured tags to mapping and outgoing content to the host.

**What it must never do:** Frame packets, run conversations, access connections or map results; never guess missing identity.

**Main parts:**

- `syntax`: Tagged message fields.

[Technical details and safety guidance](DETAILS.md).
