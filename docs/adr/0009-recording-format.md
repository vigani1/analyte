# 0009. Recording format

Status: accepted, 2026-11-15

## Context

Recorded conversations are the main test fixtures (plan §10) and the evidence
for the simulated and proven levels (ADR 0005). Captures come from machine
trace logs, a sniffer on the lab PC or a hardware tap, and the fake machine
replays them. One stored format is needed so every capture tool, fixture and
replay reads the same thing.

## Decision

A recording is one UTF-8 text file with the extension `.rec.jsonl`: one JSON
object per line, LF line endings.

- **First line, header:** format version (`1`), protocol stack (transport,
  framing, dialog, syntax as named in config), the machine and its software
  version when known, the evidence source (`machine`, `vendor_trace`,
  `simulator` or `synthetic`), the de-identification statement, and the
  source document or capture method named in words, never copied text.
- **Every other line, one event:** direction (`machine_to_host` or
  `host_to_machine`), time in milliseconds since the first event, and the
  exact bytes as base64. A line may also mark a link event: `connect`,
  `disconnect` or `break` (serial line break).
- Bytes are exactly as seen on the wire, including split and merged reads as
  they happened. Nothing is normalized, trimmed or re-encoded.
- Readers are bounded: maximum file size, line length, event count and bytes
  per event, each checked before allocation. An invalid line is an error at
  that line, never skipped.

Replay plays the machine side and compares what the host sends with the
recorded host side: bytes must match exactly, and timing must fall inside the
deadlines of the protocol and config, not the recorded gaps. Host bytes that
legitimately differ per run (message control IDs, timestamps) are declared per
recording as named, located exceptions, never matched by pattern.

## Consequences

- Fixtures stay readable in review and diff line by line; base64 keeps binary
  control characters exact.
- Only synthetic or de-identified recordings enter the repository; real
  captures with patient data stay in the protected store.
- A new format version needs a new ADR; old recordings stay readable.
