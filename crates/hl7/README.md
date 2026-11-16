# hl7

**What it does:** Reads and writes HL7 messages, including the IHE LAW laboratory profile.

**Where it sits:** Receives message bytes from the host; supplies structured fields to mapping and message builders for replies.

**What it must never do:** Access connections, map results or mistake a parsed acknowledgment for proof of safe storage.

**Main parts:**

- [syntax.rs](src/syntax.rs): Message grammar.
- [views.rs](src/views.rs): Named field access.
- [version.rs](src/version.rs): Explicit edition rules.
- [ack.rs](src/ack.rs): Acknowledgment message syntax.
- [order.rs](src/order.rs): Order messages and responses.
- [query.rs](src/query.rs): Legacy display query messages.
- [law.rs](src/law.rs): LAW queries, work and results.

[Technical details and safety guidance](DETAILS.md).
