# Analyzer research: overview

Research dates: 2026-11-14 to 2026-11-15. Research only. Nothing was built, and
no machine was connected or tested. It was done for the Echo Med lab and its
six analyzers, before the decision to build Analyte; "our app" means Echo Med,
and "the Bridge" means the OpenELIS Analyzer Bridge. Analyte's own direction is
in [../plan.md](../plan.md) and [../context.md](../context.md).

Each device file separates **verified** facts (read in a vendor document or in
the Bridge source) from **inferred** and **UNVERIFIED** ones. Read the device
file before relying on any detail below.

| File                               | What it covers                                                                                   |
| ---------------------------------- | ------------------------------------------------------------------------------------------------ |
| [cobas-e411.md](cobas-e411.md)     | Roche cobas e 411: serial ASTM, host query, barcode and rack/disk position workflows, host codes |
| [iflash-1200.md](iflash-1200.md)   | YHLO iFlash 1200: HL7 2.3.1 over MLLP (TCP), query flow, barcode and position workflows          |
| [vidas.md](vidas.md)               | bioMérieux VIDAS (original, five sections) via VIDAS PC: proprietary "Literal" protocol          |
| [autolyser.md](autolyser.md)       | Dialab Autolyser (REF 20010D): proprietary host-polled protocol, from the official manual        |
| [biolyte-2000.md](biolyte-2000.md) | BioCARE Biolyte 2000: serial ASTM, results only                                                  |
| [swelab.md](swelab.md)             | Boule Swelab (likely Lumi, 5-part CBC): HL7 over the network, typed or handheld-scanned IDs      |
| [bridge.md](bridge.md)             | OpenELIS Analyzer Bridge 3.5.0 internals: transports, parsers, output, deployment, gaps          |
| [middleware-practices.md](middleware-practices.md) | Independent research: how integration engines and lab middleware are built, standards notes, failure stories, overlooked topics, Rust ecosystem |
| [idea-check.md](idea-check.md)     | Check of the ideas borrowed from other projects: keep, change or replace                         |
| [rust-practices.md](rust-practices.md) | Rules and tooling from large Rust projects and company guidelines                            |
| [machine-gaps.md](machine-gaps.md) | What each machine still lacks: documents, settings and recordings (kept current)              |

## The machines at a glance

| Machine       | Connection                                                             | Direction                                                 | Tube scanner                                                                               | Bridge today                                                                                             |
| ------------- | ---------------------------------------------------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| cobas e 411   | Cable (RS-232, 9600 8N1)                                               | Results, asks for tests, accepts test lists               | Built in: Code 128, Code 39, Codabar, ITF                                                  | Receives the serial frames; parser needs Roche-specific fixes                                            |
| iFlash 1200   | Network (HL7 over MLLP; the analyzer PC connects to the Bridge)        | Results, asks for tests, accepts test lists               | Built in: Code 128, Code 39, Codabar, ITF                                                  | Receives and acknowledges; parser reads the wrong sample field                                           |
| VIDAS         | Cable from the VIDAS PC (RS-232, 9600 default)                         | Results, accepts test lists (does not ask)                | None for tubes; ID typed or scanned by hand at the VIDAS PC                                | Cannot read its framing or its messages                                                                  |
| Autolyser     | Cable from its PC (RS-232); a TCP/IP option exists but is undocumented | Results only when asked; accepts test lists               | Optional; REF 20010D is catalogued _without_ readers                                       | Cannot poll it or read its messages                                                                      |
| Biolyte 2000  | Cable (RS-232, DB25, 9600 8N1)                                         | Results only                                              | None built in; a scanner can be attached for the ID                                        | Should receive it with a profile; needs a real capture to confirm                                        |
| Swelab (Lumi) | Network (HL7; the analyzer connects to the LIS, default port 5600)     | Results, looks up tests by sample ID when analysis starts | None built in; ID typed (1–25 characters, or auto-numbered) or scanned with a USB handheld | Likely receives with a profile if the sample ID is in OBR-3; needs the Lumi interface spec and a capture |

## What the Bridge needs, per machine

"Profile" means a JSON file we write; "code" means Java changes in our own build
of the Bridge. Every machine also needs its test codes mapped and a bench test
with real (de-identified) messages before go-live.

| Machine       | Results                                                                                                                                                                                               | Orders (later, optional)                                                                                                                   |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| cobas e 411   | Profile **plus code**: Roche test IDs contain slashes (`10//`), quantitative and qualitative results share one field differently, QC is marked in two dialects, and the rack/disk position is dropped | Code: answer host queries and send test lists **over serial** (the Bridge only sends orders over TCP and answers queries with "no orders") |
| iFlash 1200   | Profile **plus code**: take the sample ID from OBR-2, not OBR-3 (OBR-3 is the analyzer's own counter); read qualitative calls from OBX-9; QC arrives in OBR without OBX                               | Code: answer `QRY^Q02` with `QCK^Q02` + `DSR^Q03` (today it gets a plain acknowledgement)                                                  |
| VIDAS         | Code: new framing (checksum sits before ETX, unlike ASTM) and a new parser for the field-code format (`mtrsl\|pi…\|ci…\|rt…`)                                                                         | Code: send test lists (`mtmpr`)                                                                                                            |
| Autolyser     | Code: a new driver that **asks** for results on a timer (STX → ACK → `R` → read → checksum → EOT) and a fixed-width parser                                                                            | Code: send worklists (code + position + 4-character test codes)                                                                            |
| Biolyte 2000  | Profile only, if the real output matches the manual                                                                                                                                                   | Not possible: the machine only sends                                                                                                       |
| Swelab (Lumi) | Profile, possibly no code: depends on where the Lumi puts the sample ID (the Bridge reads OBR-3, then OBR-2) and on histogram/flag handling                                                           | Code: answer its lookup (message type unverified; its relative, the Dymind DF50, uses `ORM^O01` → `ORR^O02`), within 10 s by default       |

Shared work, needed for any machine:

- **Our receiver.** A route in our app that accepts the Bridge's FHIR bundles over
  HTTPS with Basic credentials (no OpenELIS pairing needed), ignores repeats by
  bundle ID, and stores before answering. See [bridge.md §7](bridge.md).
- **Running it on the lab PC.** Run the Bridge as a Java service, not in Docker:
  Docker Desktop on Windows cannot reach COM ports, and four machines use cables.
  See [bridge.md §9](bridge.md).
- **Keeping our build small.** Ship our profiles separately, put each proprietary
  driver in its own package, pin one upstream release, and offer generic fixes
  upstream. MPL-2.0 only obliges us to share changed Bridge files if we give the
  program to another organisation. See [bridge.md §10](bridge.md).

## Barcode or slot: does it still work without barcodes?

Every machine can run without tube barcodes, but **what identifies the sample
in the result differs**, and only some machines report the slot at all.

| Machine       | Without a barcode, the result carries                                                  | Reports the slot?                                              |
| ------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| cobas e 411   | The programmed sample ID (or a generated `@sequence`), plus disk/rack and position     | Yes, in O.4; it can also ask "which tests for rack 7, slot 2?" |
| iFlash 1200   | Sample ID (exact field on the 1200 unverified), plus its own counter and rack^position | Yes, in OBR-10                                                 |
| VIDAS         | The sample ID typed at the VIDAS PC, or the one from a downloaded test list            | No: section/position (A1–E6) never appears                     |
| Autolyser     | The patient code that was programmed for that cup                                      | No: the cup position is only used when programming             |
| Biolyte 2000  | The ID typed on the keypad; its sequence number wraps at 999                           | No                                                             |
| Swelab (Lumi) | The typed, scanned or auto-numbered sample ID (exact field unverified)                 | No: one open tube at a time, no slots                          |

What this means:

1. **Matching by slot alone is not safe and mostly not possible.** Four machines
   never send the slot, and the Bridge currently **drops** the slot even from the
   two that do ([bridge.md §8](bridge.md)). A slot is also reused all day, so it
   cannot identify a patient by itself.
2. **What works everywhere is getting our tube code onto the machine.** There are
   three ways:
   - **Barcode label**, scanned by the machine: works for the cobas e 411 and the
     iFlash.
   - **Test list sent ahead** (orders) with our code and a slot: works for the
     cobas e 411, the iFlash, the VIDAS and the Autolyser.
   - **Staff type our tube code** on the machine (or scan it with a handheld
     scanner at its PC): works on every machine, and is close to what they do now.
3. **Slot-only workflows** (staff load slot 5 and the app knows slot 5 is
   V-00456) would need a Bridge change to keep the position, a loading list in our
   app per run, and only work on the cobas e 411 and the iFlash. Not recommended
   as a first step.

## Tube codes that every machine accepts

The limits found so far: Autolyser patient code 15 characters; VIDAS 12 or 17
(two bioMérieux documents disagree); Biolyte ID 12; cobas e 411 legacy barcode
tables about 12–13; Swelab 1–25; iFlash unverified.

Safe choice until each lab confirms its machines: **at most 12 characters,
digits only**, printed as **Code 128**. Digits-only codes are accepted by every
reader listed above, avoid ASTM/HL7 delimiter characters, and keep Code 128 labels
short. The Autolyser reads Code 128 only if Dialab enables it, but this unit is
probably without a reader anyway.

## Biggest open questions for the lab

1. **cobas e 411:** disk or rack, software version, photo of the host code table,
   and whether Roche must enable the host connection.
2. **iFlash 1200:** software version and the LIS settings screen (server IP and
   port). Ask YHLO to confirm the 2018 protocol applies to the 1200.
3. **VIDAS:** VIDAS PC software version and whether a handheld scanner is attached.
4. **Autolyser:** software version, whether a sample barcode reader was added, and
   the Serial/TCP setting in Setup.
5. **Biolyte 2000:** photo of the back panel and whether an ID scanner is attached.
6. **Swelab:** nameplate photo (confirm Lumi), software version, LIS settings
   screen, and whether a handheld scanner is used. Ask Boule for the Lumi LIS
   interface specification.
7. **All:** which tests run on which machine, how tubes are labelled today, and a
   de-identified captured result message from each machine (needed for bench tests).
