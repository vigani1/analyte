# 0009. Recording format

Status: accepted, 2026-11-15

## Context

Recorded conversations are the main test fixtures (plan §10) and the evidence
for the simulated and proven levels ([ADR 0005](0005-evidence-levels.md)).
Captures come from trace logs, a sniffer or a hardware tap, and the fake
machine replays them, so every tool must read one stored format.

## Decision

- A recording is one UTF-8 `.rec.jsonl` file, one JSON object per line, LF
  endings.
- The first line is a header: format version `1`, the protocol stack, the
  machine and software version when known, the evidence source (`machine`,
  `vendor_trace`, `simulator` or `synthetic`), a de-identification statement,
  and the source or capture method named in words, never copied text.
- Every other line is one event: direction (`machine_to_host` or
  `host_to_machine`), milliseconds since the first event and the exact bytes in
  base64, or a link event (`connect`, `disconnect`, `break`).
- Bytes are exactly as on the wire, including split and merged reads; nothing
  is normalized.
- Readers check file size, line length, event count and bytes per event before
  allocating. An invalid line is an error at that line, never skipped.
- Replay plays the machine side. Host bytes must match exactly, and timing must
  fit the protocol and config deadlines, not the recorded gaps. Bytes that
  differ per run (message IDs, timestamps) are declared per recording as named,
  located exceptions, never matched by pattern.

## Why

Text lines are easy to review and diff, and base64 keeps control characters
exact.

## Consequences

- Only synthetic or de-identified recordings enter the repository; real
  captures stay in the protected store.
- A new format version needs a new ADR, and old recordings stay readable.

## Amendment: model replay settings (2026-11-23)

A repository model can place an optional `replay.toml` beside its definition,
keyed by recording filename. Entries supply the site's `choices` (ADR 0032)
or `recording` answers (ADR 0034), and can declare a known missing final
reply by event index, failure and reason. The harness still replays the
conversation and fails when a marked reply starts arriving. See the
[machine folder guide](../../machines/README.md). The sidecar belongs to
repository tests. The version 1 capture header and events are unchanged.

## Amendment: de-identified copies (2026-11-26)

`analyte captures de-identify` writes a de-identified copy of a stored
recording ([ADR 0037](0037-de-identified-recording-copies.md)). The copy keeps
format version 1, its evidence and every event length; its
`de_identification` statement starts with "De-identified by Analyte captures
de-identify".
