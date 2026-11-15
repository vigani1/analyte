# Framing (documented evidence)

This crate implements LIS01-A2 §§6.3, 6.5.1, 6.6 and their TCP counterparts
§§8.3, 8.5.1, 8.6, and MLLP Release 1 §§1.2–1.3. It has no I/O.

Feed bytes individually and handle each returned event before continuing.
Chunk boundaries have no meaning. Events preserve complete wire frames and
bounded rejected prefixes. A new start byte is retained in the new pending
frame rather than duplicated in the rejection. Call `finish` on disconnect or
an externally enforced deadline to retrieve partial evidence.

The host must journal transport bytes before decoding, including malformed
input. The decoder cannot retain an unlimited malformed stream: after an error
it emits outside bytes individually until the next start. ASTM retains at most
its total frame limit and emits at most limit + 1 bytes of rejected evidence;
MLLP retains at most payload limit + 2 and emits at most payload limit + 3.
Output event ownership and cumulative storage are the host's responsibility.

ASTM defaults to 64,000 total bytes, with seven framing bytes. Smaller configured
limits must allow at least one text byte. The lazy encoder splits one supplied
record into ETB frames and a final ETX frame; callers supply the first number
and retain numbering across records. It does not assemble or interpret records.
Only the control bytes prohibited in §§6.6/8.6 are rejected as text; other raw
bytes, including CR and high-bit values, are preserved. Lowercase checksums
require an explicit compatibility policy and remain unchanged in raw output.

ENQ/ACK/NAK/EOT handling, accepted sequence tracking, retransmissions, timers,
transport errors and store-before-ACK are dialog/host responsibilities. A valid
framing event alone never authorizes acknowledgment.

MLLP requires an explicit payload limit. Outside bytes, end without start,
nested start, invalid payload controls, oversized payloads, and missing CR each
produce specific errors. Nested start begins a new block; other defects reset
to idle. Only CR and bytes above 0x1F are valid payload (Release 1 §1.2).
UTF-16/32 cannot be used; character encoding agreement belongs above framing.

Vendor framing, line framing, persistence, dialog behavior and text decoding are
intentionally excluded. Synthetic tests provide documented evidence only;
there are no real-machine recordings or official simulator results here.
