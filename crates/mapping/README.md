# mapping

**What it does:** Translates machine fields and codes into the common model using explicit settings, and maps outgoing work.

**Where it sits:** Receives structured protocol messages and configuration; hands model evidence and findings to the host.

**What it must never do:** Access connections, make clinical decisions or guess missing information; the host retains receipts and parks findings.

**Main parts:**

- [astm.rs](src/astm.rs), [hl7.rs](src/hl7.rs), [law.rs](src/law.rs), [poct.rs](src/poct.rs), [vidas.rs](src/vidas.rs), [autolyser.rs](src/autolyser.rs): Translations for each protocol.
- [inbound.rs](src/inbound.rs): Incoming translation steps that preserve evidence.
- [orders.rs](src/orders.rs): Outgoing mappings, separate from incoming code tables.

[Technical details and safety guidance](DETAILS.md).
