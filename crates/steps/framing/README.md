# framing

**Kind: step.** One file per protocol where it has a role here (`astm.rs`, `hl7.rs` and so on).

**What it does:** Finds message or packet boundaries and checks checksums where required.

**Where it sits:** Receives bytes retained by the host and gives bounded frame events to the dialog.

**What it must never do:** Access connections, interpret message meaning or log payloads; framing alone never authorizes acceptance.

**Main parts:**

- `astm`: ASTM frames and checksums.
- `hl7`: MLLP Release 1 boundaries, the byte envelope used around HL7.
- `poct`: POCT XML message boundaries.
- `vidas`: VIDAS packet boundaries.
- `autolyser`: Autolyser checksum envelopes.

[Technical details and safety guidance](DETAILS.md).
