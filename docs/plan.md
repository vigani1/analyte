# Analyte plan

Status: planning, 2026-11-15. Nothing is built.

## 1. Goal

One open program that connects any clinical lab analyzer to any lab app:

- receives results and stores them safely;
- answers analyzer questions ("which tests for this tube?") and sends work
  lists ahead;
- delivers everything to the lab app over HTTPS in one common format.

Analyte is its own product, built for any lab and any lab app. Apps connect
through a published contract; no app is a design input. One lab's six
analyzers are the first real machines to support and prove: they are the
hardware test bed for the drivers.

## 2. Why build it

Existing options were researched before this plan:

- The OpenELIS Analyzer Bridge (Java) only reads ASTM, HL7 and files. It
  cannot poll a machine, send orders over a serial cable or answer host
  queries with real orders, and it drops the sample position.
- General integration engines (Mirth and its forks, Iguana, Rhapsody, IRIS)
  route messages well, but leave every analyzer conversation to custom
  scripts.
- Lab middleware (Data Innovations and vendor products) is closed and costly.
- Open projects (BLIS interface client, SENAITE ASTM, several small ASTM and
  HL7 libraries) each cover one app, one protocol or one machine. No Rust
  implementation of clinical ASTM exists.

## 3. Scope

**In:** serial, TCP and file transports; ASTM, HL7 v2 (2.3.1 and 2.5.1), IHE
LAW and POCT1-A protocols; vendor formats as small pieces; durable storage;
delivery to apps; order lookup and work lists; a local admin page; Windows and
Linux service installs.

**Out:** anything a lab information system decides: auto-verification, reflex
testing, QC rules, critical-value alerts, patient matching. Analyte supplies
evidence; the app decides.

## 4. Principles

1. Works with any app from day one. The contract follows best practice and
   the research, not one app's routes, hosting or test codes.
2. A small core that knows no machine. Machines are configs; unusual formats
   are pieces in the right layer.
3. Store first, then acknowledge. Nothing is ever silently lost.
4. Every driver and config ships with recorded conversations and states its
   evidence level (proven, simulated, documented).
5. Outbound connections only: Analyte calls the app; the lab network opens no
   inbound ports to the internet.
6. One common model for results, orders and sample identity, including the
   position.
7. Standards first, machines after. Each standard is implemented completely
   from its official documents and tested against official examples or
   simulators, independent of any machine. Machines are then added as configs
   on top, by us or the community.

## 5. Architecture

```text
Lab app  ⇄  HTTPS: results out, orders in (polled)
              ↑
Core: durable store · delivery outbox · order cache · audit · admin page
              ↑
Mapping:  machine codes → common model (config)
Syntax:   ASTM records · HL7 segments · tags · fixed width · XML · CSV
Dialog:   machine pushes · machine asks · host polls · host sends ahead
Framing:  ASTM (E1381) · MLLP · start/end + checksum · lines · none
Transport: serial · TCP client/server · watched folder
```

- Dialogs are state machines owned by one connection, with their own
  deadlines. They may call the syntax layer to decide a reply.
- Not every combination is valid; config validation rejects impossible ones.
- Planned workspace crates (one reason to change each):

| Crate               | Owns                                                         |
| ------------------- | ------------------------------------------------------------ |
| `analyte-model`     | Common model: results, orders, identities, revisions         |
| `analyte-store`     | Local storage (SQLite), outbox, audit                        |
| `analyte-transport` | Serial, TCP, folder                                          |
| `analyte-framing`   | ASTM frames, MLLP, start/end + checksum, lines               |
| `analyte-dialog`    | Conversation state machines                                  |
| `analyte-astm`      | ASTM record syntax                                           |
| `analyte-hl7`       | HL7 v2 syntax, acknowledgments, LAW transactions             |
| `analyte-config`    | Machine config files (TOML), validation, versions            |
| `analyte-sink`      | Delivery to apps and order polling                           |
| `analyte-sim`       | Fake machine and replay of recordings                        |
| `analyte-receiver`  | Reference receiver: a small app that implements the contract |
| `analyte`           | The program: service, CLI, admin page                        |

Vendor formats (VIDAS, Autolyser) and POCT1-A get their own crates when they
are built.

## 6. Common model

- **Three identities:**
  1. the receipt: what arrived from the machine, with its raw bytes;
  2. the delivery: one ID per delivery to the app, unchanged across retries;
  3. the result: a stable observation identity plus a revision (final,
     corrected, cancelled), so corrections and reruns are never confused with
     duplicates.
- **Sample identity:** ID as a string with its source, plus position (rack or
  disk, slot) when the machine reports one.
- **Values:** original text, parsed number with comparator and precision when
  it parses, original unit, mapped UCUM unit when mapped, dilution and whether
  it was applied.
- **Flags, ranges, comments, QC class (patient, control, calibration),
  machine times and receive time.**
- **Order lifecycle:** requested → sent → accepted by machine → done, or
  cancelled / rejected, each with its time and source.

## 7. Contract with lab apps

- **Results:** Analyte sends JSON over HTTPS, signed, with a delivery ID. The
  app stores before answering 2xx and treats a repeated delivery ID as already
  done. Retries back off; parked deliveries stay visible.
- **Orders:** Analyte polls the app for "changes since" every few seconds and
  keeps a local order cache, so machine queries are answered within their
  deadline (cobas e 411: about 15 seconds). If the cache cannot be trusted, it
  never answers "no orders".
- **Versioned and documented** so any app can implement it.
- **Reference receiver:** a small server in this repository implements the
  whole contract. Tests run against it, and it is the worked example for app
  developers. Packages for other languages come later.
- FHIR output is an option for later, not the base format.

## 8. Protocol support

| Protocol          | Source documents                                        | How it is tested before real machines |
| ----------------- | ------------------------------------------------------- | ------------------------------------- |
| ASTM              | CLSI LIS01-A2 + LIS02-A2, vendor manuals                | Examples in the standards, then vendor traces |
| HL7 v2 + MLLP     | HL7 v2.5.1 and v2.3.1, MLLP Release 1                    | Examples in the standard, HL7 tools, then vendor traces |
| IHE LAW           | IHE PaLM Technical Framework Vol. 1 and 2b, Rev 11.0     | IHE Gazelle simulator                 |
| POCT1-A           | CLSI POCT1-A2                                           | Documents only at first               |
| Vendor formats    | Each vendor's interface manual                          | Vendor examples, recordings           |

Reference terminologies: LOINC (test codes) and UCUM (units).

Machine documents used for the first machines:

| Machine              | Documents                                                                           |
| -------------------- | ----------------------------------------------------------------------------------- |
| Roche cobas e 411    | Host Interface Manual v2.3 (v2.2 also read); Operator's Manual v3.3.1               |
| YHLO iFlash 1200     | YHLO LIS Protocol Manual V1.1 (iFlash family); iFlash 1200 Operation Manual V3.0    |
| Boule Swelab Lumi    | Lumi User Manual 1504529; Boule's LIS protocol description for the Lumi             |
| BioCARE Biolyte 2000 | Service Manual 4.2A, Appendix VI                                                    |
| bioMérieux VIDAS     | BCI LINK LIS Interface Driver Development Guide 161150-486 C; VIDAS PC User Manual F |
| Dialab Autolyser     | Autolyser / Autolyser ISE Operator's Manual Rev18; Service Manual Rev01             |

## 9. First machines

These six machines from one lab are the hardware test bed: the first real
machines to reach the proven level. They are added as configs (or vendor
format pieces) after the standards they use are complete.

| Machine                | Protocol                          | Direction                         | Evidence today | Missing                                          |
| ---------------------- | --------------------------------- | --------------------------------- | -------------- | ------------------------------------------------ |
| Roche cobas e 411      | ASTM over serial                  | Results, host query, orders       | documented     | Lab's host code table, trace log copy            |
| YHLO iFlash 1200       | HL7 2.3.1 over MLLP (TCP)         | Results, query (QRY/QCK/DSR)      | documented     | Protocol confirmed for the 1200, port            |
| Boule Swelab Lumi      | HL7 over TCP                      | Results, sample lookup            | documented     | Sample ID field, lookup message type             |
| BioCARE Biolyte 2000   | ASTM over serial                  | Results only                      | documented     | One real recording                               |
| bioMérieux VIDAS (PC)  | Vendor "Literal" over serial      | Results, work list download       | documented     | VIDAS PC version (sample ID length 12 or 17)     |
| Dialab Autolyser       | Vendor format, host polls         | Results when asked, work list     | documented     | TCP settings; barcode reader retrofit            |

## 10. Testing

- **Recordings:** exact bytes, direction and timing of real conversations,
  de-identified, replayed by the fake machine. Serial captures come from the
  machine's own trace log, a sniffer on the lab PC or a hardware tap.
- **Failure paths:** broken checksums, lost acknowledgments, split and merged
  reads, timeouts, disconnects, restart between storing and acknowledging,
  disk full, corrected and repeated results, late query replies.
- **Property tests** for framers and parsers; **fuzzing** on Linux CI.
- **Validation pack for labs:** the cases a lab must check before go-live and
  after changes, all the way to the screen the doctor sees (CAP and ISO 15189
  require this).

## 11. Operations

- One program, installed as a Windows service or Linux systemd unit, starting
  without a login.
- Local admin page (localhost, password): machines, last messages, parked
  items, resend and reprocess as separate actions, all audited.
- Alerts on oldest undelivered item, stalled machines and disk space.
- The PC must not sleep; updates are scheduled; USB serial reconnects and COM
  port changes are handled.
- Backups of the store; a tested restore.

## 12. Security, privacy and regulation

- Analyzer ports listen only on the lab network; outbound HTTPS with verified
  certificates and rotated credentials.
- Health data stays in the protected store; logs carry IDs, not payloads.
- GDPR roles and retention are decided per deployment.
- EU: software that only moves data is often not a medical device, but sending
  work lists to analyzers needs an intended-purpose statement and an
  assessment under MDCG 2019-11 before any claim is made.

## 13. Phases

| #   | Work                                                                                | Done when                                  |
| --- | ----------------------------------------------------------------------------------- | ------------------------------------------ |
| 0   | Name, license, repo, this plan, first ADRs                                          | Plan approved                              |
| 1   | Core: workspace, common model, store, outbox, delivery, fake machine, first reference receiver, CI | A fake result reaches the reference receiver, failure paths included |
| 2   | ASTM complete: serial and TCP transport, LIS01 framing, LIS02 records, all dialogs (machine pushes, machine asks, host sends ahead) | Conforms to LIS01-A2 and LIS02-A2, tested with their examples (documented) |
| 3   | HL7 v2 complete: 2.3.1 and 2.5.1 syntax, acknowledgments, MLLP              | Conforms to HL7 v2 and MLLP Release 1, tested with official examples and HL7 tools (documented) |
| 4   | IHE LAW: LAB-27 query, LAB-28 work order, LAB-29 results                     | Passes the IHE Gazelle simulator (simulated) |
| 5   | POCT1-A                                                                      | Conforms to POCT1-A2, tested with its examples (documented) |
| 6   | Orders and query answers, generic: order cache, "changes since" polling, deadlines; full reference receiver and published contract with conformance tests | Contract conformance tests pass end to end; query answers meet configured deadlines |
| 7   | Machine configs: cobas e 411, Biolyte 2000, iFlash 1200, Swelab Lumi        | Each config passes its vendor traces (simulated) |
| 8   | Vendor formats: VIDAS Literal, Autolyser (start/end framing, tags, fixed width, polling dialog) | Each piece passes its vendor examples (documented or simulated) |
| 9   | Real-lab pilot with the six machines, admin page, installers, release process, docs for adding a machine | The six machines reach proven at the lab; another lab can install it |
| 10  | Plugin interface once three formats prove it                                 | Contributors add machines                  |

A standard is done when it conforms to its official documents, at the
documented or simulated level, never when one machine happens to pass. The
proven level comes only from real machines in phase 9.

## 14. Open decisions

1. Exact JSON contract and signing scheme for any lab app (phase 1 ADR).
2. Plugin mechanism (WASM components or out-of-process), decided only after
   the vendor formats (phase 8).

## 15. Research behind this plan

The detailed research (devices, OpenELIS Bridge internals, middleware
practices, idea check, Rust practices) lives in
[research/](research/README.md). Background and decisions are in
[context.md](context.md). Protocol notes for implementation go into
`docs/protocols/` in our own words as each piece is built.
