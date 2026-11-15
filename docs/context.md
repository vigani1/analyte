# Context for new sessions

Read this first, then [plan.md](plan.md) and [../AGENTS.md](../AGENTS.md).
Last updated 2026-11-15.

## How this project started

The idea came from one lab that wanted its analyzers connected to a lab app.
Research into the lab's six machines and the existing OpenELIS Analyzer Bridge
(see [research/](research/README.md)) showed the Bridge could not cover them
well: it cannot poll a machine, send orders over a serial cable or answer host
queries with real orders, and it drops the sample position. Other options were
app-specific, closed or too general. The decision was to build **Analyte**: an
open, app-independent bridge in Rust.

**Analyte is its own product.** It is designed for any lab app, from best
practice and the research. No app is its first user, its target or a design
input, including the owner's own lab app (Echo Med, a separate project), which
is just one app that might integrate someday. The research files still say
"our app" for Echo Med because they were written before this decision.

## Working with the owner

- Answer briefly, in plain language; prefer one small table over long reports.
- The owner has no Rust experience. The goal is a clean, high-quality product
  built with current best practices and a codebase that is easy to follow.
  Explain decisions in plain words, and prove behavior with tests and
  recordings.
- Check current versions and practices before choosing a library or approach;
  do not rely on memory. Say when something is unverified.
- Research before deciding. Research subagents (for example Codex) are fine
  when the owner asks for research.
- Go with the recommended option and report what was chosen and why; do not
  ask the owner to pick. Ask only before something public or hard to undo
  outside the repository (force-pushing, spending money).

## Decisions so far

| Decision                                                                   | Date       |
| -------------------------------------------------------------------------- | ---------- |
| Build our own bridge instead of forking the OpenELIS Bridge                | 2026-11-14 |
| Rust; one program for Windows and Linux; library crates usable on their own | 2026-11-15 |
| Name **Analyte**; Apache-2.0; public repository                             | 2026-11-15 |
| Contributions: DCO sign-off on every commit; AI help allowed with human review and disclosure ([CONTRIBUTING.md](../CONTRIBUTING.md)) | 2026-11-15 |
| Layers + config per machine; code only where config cannot express it     | 2026-11-15 |
| Core supports all four standards: ASTM, HL7 v2 (+ MLLP), IHE LAW, POCT1-A  | 2026-11-15 |
| Machine configs can come from the community; each states its evidence level | 2026-11-15 |
| A bridge only: no auto-verification, QC or clinical decisions              | 2026-11-15 |
| Three identities (receipt, delivery, result + revision) instead of byte-hash duplicate detection | 2026-11-15 |
| Dialogs as per-connection state machines instead of a Mirth-style channel engine | 2026-11-15 |
| No plugin system until three formats prove the interface                   | 2026-11-15 |
| Few agent skills; core rules live in AGENTS.md                             | 2026-11-15 |
| Analyte is its own product; the contract is designed for any lab app, not from one app | 2026-11-15 |
| Build and test against a reference receiver inside this repository         | 2026-11-15 |
| The first lab's six machines are the hardware test bed for the drivers     | 2026-11-15 |

## The first machines (hardware test bed)

These six machines from the first lab are the first real machines Analyte
supports and proves.

| Machine                  | Connection                                        | Tube scanner                          | How the sample ID reaches the machine                                 |
| ------------------------ | ------------------------------------------------- | ------------------------------------- | --------------------------------------------------------------------- |
| Roche cobas e 411        | Serial RS-232, ASTM, 9600 8N1                     | Built in                              | Scans the barcode, asks the host for tests (answer within ~15 s)      |
| YHLO iFlash 1200         | Network, HL7 2.3.1 over MLLP; analyzer PC is the client | Built in                        | Scans, asks with QRY^Q02; host answers QCK^Q02 + DSR^Q03              |
| Boule Swelab (likely Lumi) | Network, HL7; analyzer connects to the host (default port 5600) | None; USB handheld possible | Typed or scanned; looks up tests when analysis starts (10 s default) |
| BioCARE Biolyte 2000     | Serial RS-232 (DB25), ASTM, 9600 8N1, send only   | None; external scanner possible       | Typed or scanned (ID up to 12 characters)                             |
| bioMérieux VIDAS (original, five sections) | Serial from the VIDAS PC, vendor "Literal" protocol | None                     | Work list sent ahead; staff assign positions on the VIDAS PC          |
| Dialab Autolyser (REF 20010D, 2020) | Serial from its PC; a TCP option exists but is undocumented | None (this model has no reader) | Work list sent ahead with ID and cup position; staff follow it |

What this means:

- Matching by slot alone is unsafe. Every machine must return the lab's tube
  ID: scanned by the machine, sent ahead in a work list, or typed or scanned
  by staff.
- Recommended tube ID for this lab: at most 12 characters, digits only,
  printed as Code 128.
- Lab PC: one always-on PC near the machines, about four USB-to-serial
  adapters, serial cables under about 15 m (or serial-to-network converters).
- The cobas e 411 keeps its own host trace log, a good source of real
  recordings.

## Still needed from outside

| From                   | What                                                                                     |
| ---------------------- | ---------------------------------------------------------------------------------------- |
| The lab                | Software versions; cobas host test code screen and trace log copy; VIDAS PC version (decides a 12 or 17 character ID); which tests run on which machine; one de-identified recording per machine |
| Dialab service         | Whether a sample barcode reader can be fitted to REF 20010D, its cost, and the TCP/IP settings |

## Reference receiver

Analyte is built and tested against a small reference receiver inside this
repository, not against any real app. It implements the whole contract
(receiving results, serving orders "changes since", checking signatures) and
shows app developers how to implement it. Lessons that carry over to any app:
the lab PC only makes outbound connections, orders are polled, receivers store
before answering and treat a repeated delivery ID as done, Analyte never
answers "no orders" when unsure, and every result keeps the three identities.

## Next steps

1. Settle the open decisions in [plan.md §14](plan.md).
2. Phase 1: workspace, common model, store, outbox, delivery, fake machine,
   first reference receiver, CI.
