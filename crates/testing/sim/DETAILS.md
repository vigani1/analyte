# analyte-sim

A test peer that plays the machine side of recorded conversations and compares
host bytes exactly. It is a library, not a command-line simulator. The recording
format, types and bounded codec belong to `analyte-transport::recording`; this
crate reuses them. See [ADR 0009](../../../docs/adr/0009-recording-format.md).

## Recording layout

Use a UTF-8 `.rec.jsonl` file with one JSON object per line, LF endings and a
final LF. The first line is a header:

| Field | Meaning |
| --- | --- |
| `version` | Format version, currently `1`. |
| `stack` | Four config names in order: transport, framing, dialog, syntax. |
| `machine` | Machine name when known; otherwise `null`. |
| `software` | Machine software version when known; otherwise `null`. |
| `evidence` | Origin: `machine`, `vendor_trace`, `simulator` or `synthetic`. |
| `de_identification` | Explicit statement of synthetic origin or what was de-identified. |
| `source` | Source document/sections or capture method, in your own words. |
| `exceptions` | Optional list of named, fixed host byte locations allowed to vary. |

Each later line is an event with `direction` (`machine_to_host` or
`host_to_machine`), `time_ms` (milliseconds since the first event), and `bytes`
(exact wire bytes encoded as base64). Optional `link` is `connect`, `disconnect`
or `break`. Preserve control characters, sample ID case, leading zeros and
chunk boundaries. Never trim, normalize or re-encode the decoded bytes.

Each exception has a nonempty `name`, a zero-based `event` index (the header is
not an event), and `bytes: {"start": 27, "end": 46}`. Offsets are zero-based
within that event's decoded bytes; the end is exclusive. Declare only the
specific generated control ID or timestamp that may differ. Exceptions cannot
change lengths and must locate host bytes; no patterns or whole-message masks.
See `recordings/hl7.rec.jsonl` for the synthetic example.

The default reader limits are 16 MiB per file, 16,384 bytes per JSON line,
100,000 events and 4,096 decoded bytes per event. Callers can supply explicit
`recording::Limits`. Invalid lines fail with a one-based line number; they are
never skipped. Use `recording::write_header` and `write_event` to author the
shared format, then read the complete file back with bounded `recording::read`.
The low-level writers bound individual lines/events; callers own total file and
event limits across writes.

## Adding a recording

1. Start from a synthetic conversation or make a reviewed de-identified copy
   of a protected capture. Review decoded bytes as well as metadata; base64
   does not remove patient data. Recalculate framing checksums when synthetic
   replacement bytes change them. Keep the original capture protected.
2. Add a descriptive `.rec.jsonl` file under `recordings/`. Fill every header
   field, name the protocol sections/capture method, and describe any deliberate
   fault. Write source notes in your own words; do not copy paid manual examples.
3. Preserve the observed direction and chunk boundaries. Add only justified,
   named exceptions at exact host byte locations. A recording of a fault should
   retain the faulty bytes, not silently repair them.
4. Add a test with a source comment in `crates/analyte/tests/delivery.rs`, following its
   `recording` helper and existing cases. Use a temporary store, loopback peers
   and the reference receiver. Load the recording through transport's bounded
   reader, then call `analyte_sim::replay` with its events and header exceptions.
   Derive the deadline from the effective protocol/config, never recorded gaps.
5. Assert exact stored bytes, acknowledgments sent or withheld, and deliveries
   made, including duplicates/revisions where applicable. For partial-frame
   cases, assert that no ACK arrives before completion. TCP may combine or split
   writes independently of recorded chunks; never assume socket packet boundaries.
   Keep mismatch hex diagnostics in protected test output, never ordinary logs.
6. Run `cargo test -p analyte --all-features --test delivery recording` and the
   workspace checks required by `AGENTS.md`. The feature-specific restart test
   exercises the durable-frame/pre-ACK pause; `test-hooks` must never be enabled
   for release builds.

Replay operates on an already connected `AsyncRead + AsyncWrite` peer: loopback
TCP or a bounded serial test seam. A `connect` event is descriptive and does not
open a socket; `disconnect` shuts down writes. Serial `break` is represented by
the format but rejected by this replay implementation. Times are retained as
capture evidence; replay does not sleep for recorded gaps. Discard a connection
when replay is cancelled or fails: a write may already have sent a prefix.

The ASTM failure fixtures cover checksum rejection, split/merged input,
retransmission after a discarded ACK, disconnect mid-frame and restart before
a frame ACK. The restart fixture stops at the commit boundary; its test owns
the stop/restart and replays the normal fixture afterward. These are synthetic
faults, not real-machine qualification.

## Privacy and evidence

Real captures stay in the protected store. Live taps declare `machine` origin
and explicitly state that the capture is NOT de-identified. Only synthetic or
reviewed de-identified recordings may enter this repository. Never place raw
messages or hex payloads in ordinary logs, issue descriptions or PR bodies.

A recording never raises an evidence level by itself. Under
[ADR 0005](../../../docs/adr/0005-evidence-levels.md), a level applies to the named
machine, firmware, driver/config, dialogs and failure cases actually exercised.
Synthetic replay does not establish `simulated` or `proven`: official simulator
or vendor evidence is needed for `simulated`, and real-machine conversations for
`proven`, with the corresponding capability review. These fixtures leave the
existing documented evidence levels unchanged.
