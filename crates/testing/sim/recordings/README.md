# Synthetic protocol recordings

All recordings here contain invented data. They do not raise any protocol or
machine evidence level above documented.

## O20 generation and replay

Regenerate the twelve LAW, POCT, VIDAS and Autolyser fixtures from the repository root:

```text
cargo test -p analyte --test vendor_receive generate_protocol_recordings -- --ignored --nocapture
```

The explicitly ignored authoring test in
`crates/analyte/tests/vendor_receive/recordings/` drives a local program with
synthetic peers, checks expected acknowledgments and durable outcomes, and uses
`analyte_transport::recording::write_header` and `write_event`. It reads each
file back through the bounded codec before writing it. No base64 or JSONL is
hand-edited. Machine messages use existing POCT builders, VIDAS and Autolyser
framing encoders, and a synthetic LAW OUL message assembled from named fields.
Host bytes come directly from the local program. Event times are authored
10 ms sequence markers, not measured performance or delays used by replay.

The ordinary `vendor_receive` integration binary reads these files and calls
the existing `analyte_sim::replay` engine against a fresh program per recording.
It compares host bytes, verifies retained incomplete prefixes and operator
parking, and checks one LAW delivery even after retransmission. Vendor result
identities remain unresolved and parked, with no delivery invented.

| Protocol | Happy | Lost acknowledgment | Disconnect |
| --- | --- | --- | --- |
| LAW | Accept unsolicited LAB-29 result and deliver once | Observe/discard ACK, resend identical result, retain one delivery | Partial MLLP block is retained and parked |
| POCT | Hello, status and split observation, custody before ACK | Observe/discard result ACK; reused incoming ID produces correlated ESC and abnormal END (POCT01-A2 B 5.13, current strict duplicate policy) | Partial observation is retained and closure is parked |
| VIDAS Literal | ENQ, split framed result, packet ACK, ETX/EOT | Observe/discard packet ACK; out-of-phase resend while final controls are expected closes visibly with custody retained | Partial packet is retained and transfer is parked |
| Autolyser | STX/ACK, R/EOT and split report | Omit the machine initialization ACK; require EOF at the configured deadline without sending R or retrying | Partial report is retained and parked |

Autolyser sends no result ACK. Its lost-ACK fixture therefore ends after STX;
the replay test separately waits for host EOF within the test bound, proving
that no command follows the missing initialization acknowledgment. VIDAS also
asserts host EOF after its out-of-phase resend. No recorded gap causes a test
sleep or substitutes for the configured protocol deadline.

Only generated host header fields have named, exact byte-range exceptions:
LAW MSH-7/MSH-10 and POCT HDR.control_id/HDR.creation_dttm. These cannot mask
acknowledgment codes, correlation references, message kinds, payloads or lengths.
The test recomputes the expected exception list and requires equality.

These are protocol fixtures rather than qualified machine definitions, so none
are copied into `machines/`. The existing machines workspace test continues to
check the shipped machine definitions and their own recordings.
