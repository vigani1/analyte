# dialog

**What it does:** Tracks each conversation: who speaks next, which reply is allowed and when time runs out.

**Where it sits:** Receives protocol events and host storage outcomes; returns actions for the host to carry out.

**What it must never do:** Read connections, store data or make clinical decisions; acceptance must wait for the required durable storage.

**Main parts:**

- [astm.rs](src/astm.rs): ASTM conversation rules.
- [hl7.rs](src/hl7.rs): HL7 conversation rules.
- [law.rs](src/law.rs): IHE LAW conversation rules.
- [poct.rs](src/poct.rs): POCT conversation rules.
- [vidas.rs](src/vidas.rs): VIDAS conversation rules.
- [autolyser.rs](src/autolyser.rs): Autolyser conversation rules.

[Technical details and safety guidance](DETAILS.md).
