# transport

**Kind: step.** One part per connection kind (network, serial or files), rather than per message protocol; it is the byte-moving step before protocol-specific steps.

**What it does:** Moves bytes through network connections, serial cables or files.

**Where it sits:** Receives bytes from a connection or file and hands them to the host for framing; carries outgoing bytes back.

**What it must never do:** Interpret messages, treat a queued write as peer receipt or put raw recordings in ordinary logs.

**Main parts:**

- `tcp`: Network sessions, reconnection and connection supervision.
- `serial`: Serial port settings and access.
- `folder`: Recoverable file handoff.
- `recording`: Protected, append-only conversation recordings.

[Technical details and safety guidance](DETAILS.md).
