# dialog

**Kind: step.** One file per protocol where it has a role here (`astm.rs`, `hl7.rs` and so on).

**What it does:** Tracks each conversation: who speaks next, which reply is allowed and when time runs out.

**Where it sits:** Receives protocol events and host storage outcomes; returns actions for the host to carry out.

**What it must never do:** Read connections, store data or make clinical decisions; acceptance must wait for the required durable storage.

**Main parts:**

- `astm`: ASTM conversation rules.
- `hl7`: HL7 conversation rules.
- `law`: IHE LAW conversation rules.
- `poct`: POCT conversation rules.
- `vidas`: VIDAS conversation rules.
- `autolyser`: Autolyser conversation rules.

[Technical details and safety guidance](DETAILS.md).
