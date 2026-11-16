# framing

**What it does:** Finds message or packet boundaries and checks checksums where required.

**Where it sits:** Receives bytes retained by the host and gives bounded frame events to the dialog.

**What it must never do:** Access connections, interpret message meaning or log payloads; framing alone never authorizes acceptance.

**Main parts:**

- [astm.rs](src/astm.rs): ASTM frames and checksums.
- [mllp.rs](src/mllp.rs): MLLP boundaries, the byte envelope used around HL7.
- [poct.rs](src/poct.rs): POCT XML message boundaries.
- [vidas.rs](src/vidas.rs): VIDAS packet boundaries.
- [autolyser.rs](src/autolyser.rs): Autolyser checksum envelopes.

[Technical details and safety guidance](DETAILS.md).
