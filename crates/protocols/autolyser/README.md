# autolyser

**Kind: protocol.** Reads and writes fixed-width records in the syntax module.

**What it does:** Reads and writes Autolyser fixed-width message records.

**Where it sits:** Receives message content after framing; supplies structured records to mapping and outgoing content to the host.

**What it must never do:** Frame packets, run conversations, map results or normalize identities and numeric text.

**Main parts:**

- `syntax`: Fixed-width record grammar.

[Technical details and safety guidance](DETAILS.md).
