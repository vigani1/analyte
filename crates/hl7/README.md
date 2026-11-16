# hl7

**Kind: protocol.** One file per job, such as `syntax.rs`, `views.rs` or `version.rs`, where that job exists.

**What it does:** Reads and writes HL7 messages, including the IHE LAW laboratory profile.

**Where it sits:** Receives message bytes from the host; supplies structured fields to mapping and message builders for replies.

**What it must never do:** Access connections, map results or mistake a parsed acknowledgment for proof of safe storage.

**Main parts:**

- `syntax`: Message grammar.
- `views`: Named field access.
- `version`: Explicit edition rules.
- `ack`: Acknowledgment message syntax.
- `order`: Order messages and responses.
- `query`: Legacy display query messages.
- `law`: LAW queries, work and results.

[Technical details and safety guidance](DETAILS.md).
