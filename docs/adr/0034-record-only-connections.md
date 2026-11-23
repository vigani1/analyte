# 0034. Record-only connections for the first lab visit

Status: accepted, 2026-11-23 (the owner asked for a "Record only" connection
so the first lab visit produces real recordings of all six analyzers, and
approved the recommended route without waiting). Extends
[ADR 0010](0010-acknowledgment-boundaries.md) with one row, uses the capture
format of [ADR 0009](0009-recording-format.md), and follows
[ADR 0031](0031-lab-chooses-serial-cable-settings.md) and
[ADR 0032](0032-machine-definitions-declare-lab-choices.md) (the lab chooses
in Setup, through one validated write).

## Context

The first lab's six analyzers have blocked definitions: a README and no
`machine.toml`, because the facts a definition needs (identity selectors,
QC rules, deadlines, software versions) come from real conversations, and the
conversations need a connection. Capture already existed (`capture = true`,
ADR 0009) but only on a machine with a full definition, hand-edited into the
program file. A recording needs only the protocol's own link rules: answer
ENQ, acknowledge a frame with a good checksum, send the HL7 acknowledgment,
ask the Autolyser for its next report. None of that needs a test, specimen or
identity mapping, or a lab app.

## Decision

1. **A record-only connection for any model.** The site file's `[recording]`
   table says the machine records only. Setup step 1 offers "Record only" for
   every model, including one not ready yet; `analyte machines set
   --record-only` does the same through the same plan and checks (ADR 0025).
   The program then always captures that machine (capture is its purpose) and
   refuses an overrides file beside it.
2. **Capture profiles ship per protocol, not per lab.** Each protocol a
   record-only connection speaks has one profile file in the config crate
   (`crates/core/config/src/recording/profiles/`), built only from its
   standard or vendor document and Analyte's own bounds, with the source
   section beside each value: ASTM (LIS01-A2 frames and timers), HL7 v2 over
   MLLP, VIDAS Literal and the Autolyser. LAW and POCT have no profile yet and
   are refused by name. A profile is compiled in, so every lab runs the same
   rules and a receipt's configuration snapshot names them exactly.
3. **The model gives only its protocol and link.** A ready model gives them
   from its `machine.toml` (`protocol` and `[transport]`, nothing else). A
   model not ready yet gives them in a small `record.toml` beside its README
   (`recording_schema_version = 1`, `protocol`, `[transport]` with the kind
   and the manual's serial suggestion). The lab's site file gives the
   connection, the cable (ADR 0031) and the answers a profile leaves open:
   `poll_seconds` and `controls` for the Autolyser, `acknowledgment`
   (Literal or Alternate) for VIDAS. Nothing is assumed: a missing answer is
   refused by name, and an answer for another protocol is refused too.
4. **The snapshot is its own stack.** A record-only snapshot is a
   `Stack::Record` with no mappings, no orders and no lab app
   (`site.destination` is absent, and only there). Its canonical TOML holds the
   protocol, the link, the whole profile and the site file, so the digest
   changes when any of them changes.
5. **Store before acknowledging, more strictly.** A record-only host is the
   protocol's own dialog driven by the chunk-journaling runner: every
   transport chunk is committed as a receipt before the dialog sees its first
   byte, so every byte (frames that fail their checksum, blocks that are not
   MLLP, partial transfers) is kept before any reply. ASTM still asks the store
   for room before accepting ENQ and answers NAK when it has none. The new row
   of ADR 0010:

   | Protocol | Durable before | Positive step it guards |
   | --- | --- | --- |
   | Record only (any profile) | Every received chunk, as its own receipt | Every reply: ENQ accepted, frame ACK, HL7 AA, VIDAS ACK, the next Autolyser R |

6. **Nothing is read, delivered or ordered.** Record-only receipts carry the
   driver namespace `analyte-record`. They are never interpreted, parked or
   delivered, startup recovery leaves them out, and reprocessing refuses them
   ("replay its capture instead"). No delivery worker, order poller or order
   path runs for the machine.
7. **A query is never answered "no tests" (safety rule 9).** ASTM: a transfer
   holding a Q record is answered with a header and a terminator with code Q,
   "error in last request for information" (LIS02-A2 12.3), never I. HL7: only
   ORU and OUL get AA ("kept"); every other message, a query above all, gets AE
   (HL7 2.3.1 2.22; 2.5.1 2.9.2), never AA without data, which means "nothing
   found". VIDAS Literal has no host query. The Autolyser is asked only for the
   next report (R) at the lab's interval: never the last report again (L),
   never the archive replay (A), never a work list. Each R is guarded as on a
   normal connection; a response that is incomplete, corrupt or not in the
   expected layout stops polling and leaves the guard uncertain for an
   operator, so a restart sends no R either.
8. **Clear status.** The Overview, the Machines list, the machine's page,
   `machines list` and `doctor` say "Recording only — nothing is sent to a lab
   app", with the messages kept and the failed exchanges since the start. A
   failure is counted, not parked: recording never fills Needs review.
9. **Getting the recordings out.** `analyte captures list` and `analyte
   captures export <machine> <file> --to <folder>` are audited before
   anything is shown or copied; export copies byte for byte and never
   overwrites. Both say the files contain patient data and must be
   de-identified before sharing (docs/develop/adding-a-machine.md). Analyte
   claims no automatic de-identification.
10. **A model's framing mode and its one departure.** A profile follows its
    standard strictly; an ASTM frame holds one record (LIS01-A2 3.3.2,
    6.3.1.1). Some analyzers offer a mode on their own screen that packs
    several records in one frame: the cobas e 411's cobas protocol type does,
    its Elecsys type does not (Host Interface Manual v2.2 B section 2 and D
    section 1; v2.3 pp. 7 and 14). Answering every such frame with NAK would
    keep the analyzer retrying and raising communication alarms, and turn
    the visit into failed sessions. So a `record.toml` may ask that mode
    (`[analyzer_mode]`, its values, no suggestion) and may declare the
    existing ADR 0015 departure `astm_packed_records_in_frame`, with the full
    ADR 0015 scope and evidence, for one of those values. The lab answers
    `recording.analyzer_mode` in the site file (Setup step 2, or `machines set
    --analyzer-mode`); a missing or unoffered answer is refused by name, and
    an answer to a model that asks none too. Only the answered mode's
    departure applies; the snapshot holds it, so the digest names it.
    Loading narrows ADR 0015 for record only: no other variant (every other
    link rule is the shipped profile's), the scope's model is the file's own
    folder key (`vendor/model`, ADR 0033), its mode one the file offers,
    analyzer uploads of ASTM results, documented level only, and the manual
    must state packing (`packed_records`), with explicit coverage when the
    firmware is `unknown`. The frame's checksum, number and length are still
    checked: a bad one is NAKed and kept. Like a normal connection with the
    departure, the dialog applies packing to every inbound frame of that
    connection, a packed query included, and its own replies stay one record
    per frame. The shared ASTM profile, and every model that declares
    nothing, stay strict. A ready `machine.toml` records with its protocol
    and link only, so its departures do not carry into record only.
11. **Leaving record only.** Saving the machine normally (Setup, or `machines
    set --connect-normally --lab-app ...`) needs a ready definition and a lab
    app, and removes `[recording]`. Recorded messages are never delivered
    afterwards; they stay as evidence, and their captures become the
    definition's recordings once de-identified.

## Why

- The first visit is the only way to get the evidence the six definitions
  need; a connection that keeps the analyzer talking and keeps every byte,
  without reading anything, gets it with the least risk.
- The link rules belong to the standard, so they are the same for every lab
  and ship with Analyte; only the protocol, the link and a few answers the
  documents leave open belong to the model and the lab.
- Journaling every chunk before the dialog runs makes "store before
  acknowledging" hold for any byte, including bytes a definition would later
  reject.

## Consequences

- A record-only machine is not a lab connection: no result reaches a lab
  app, so the lab keeps its manual workflow during the visit.
- A record-only receipt is a transport chunk, not a message; the capture
  file is the conversation to replay and to turn into a fixture.
- Each blocked model folder may hold a `record.toml`; the machine tests load
  it through the real loader, and a folder never holds both `record.toml` and
  `machine.toml`.
- A new protocol needs its own capture profile before it can record.
- A model whose analyzer frames differently by mode asks that mode in its
  `record.toml`; the lab's answer, not a shared default, decides whether a
  packed frame is accepted.
