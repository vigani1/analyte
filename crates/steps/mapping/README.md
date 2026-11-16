# mapping

**Kind: step.** One file per protocol where it has a role here (`astm.rs`, `hl7.rs` and so on).

**What it does:** Translates machine fields and codes into the common model using explicit settings, and maps outgoing work.

**Where it sits:** Receives structured protocol messages and configuration; hands model evidence and findings to the host.

**What it must never do:** Access connections, make clinical decisions or guess missing information; the host retains receipts and parks findings.

**Main parts:**

- `astm`, `hl7`, `law`, `poct`, `vidas`, `autolyser`: Translations for each protocol.
- `read`: One receipt reader per protocol, calling the framing and syntax owners
  before the existing mapper. Live intake and reprocessing share these readers;
  the program supplies durable POCT Hello context and commits the output.
- `inbound`: Incoming translation steps that preserve evidence.
- `orders`: Outgoing mappings, separate from incoming code tables.

[Technical details and safety guidance](DETAILS.md).
