# Analyte plan

Status, 2026-11-15: phases 1 to 4, 6 and 8 are built at the documented level,
while phase 5 has a POCT Basic Profile implementation with an open conformance
evidence gate (see the audited [POCT checklist](protocols/poct1-a.md)),
including IHE LAW as Analyzer Manager in the program (persistent or independent
directional links; successful recorded simulator qualification remains). The program receives and parks
results from every stack, answers machine queries and sends work lists for
ASTM, HL7, LAW, VIDAS and Autolyser through one durable order path that never
resends. Recordings replay through the program. From phase 9: OS services,
safe restore, the release process, the read-only admin page with audited
reprocess and resend, alerts with named responders, and the guide to adding a
machine exist. Phase 7 machine configs and the pilot wait on the lab. Nothing
has met a real machine yet. Recorded Gazelle message validation passes all eight
Analyzer Manager messages after fixing a SAC-3/SAC-10 defect; this does not
replace simulator qualification. The first live LAB-28 [run](conformance/gazelle/law-run/README.md)
retained an invalid ORL as Uncertain and stopped before cancellation. See [reports](conformance/gazelle/README.md).

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

- The OpenELIS Analyzer Bridge (Java) receives ASTM, HL7 and files and can
  send orders over TCP, but it cannot poll a machine, send orders over a
  serial cable or answer host queries with real orders, and it drops the
  sample position.
- General integration engines (Mirth and its forks, Iguana, Rhapsody, IRIS)
  route messages well, but leave every analyzer conversation to custom
  scripts.
- Lab middleware (Data Innovations and vendor products) is closed and costly.
- Open projects (BLIS interface client, SENAITE ASTM, several small ASTM and
  HL7 libraries) each cover one app, one protocol or one machine. No
  maintained Rust implementation of clinical ASTM was found (November 2026).

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
   [evidence level](adr/0005-evidence-levels.md).
5. Outbound connections only: Analyte calls the app; the lab network opens no
   inbound ports to the internet.
6. One common model for results, orders and sample identity, including the
   position.
7. Standards first, machines after. Each standard is implemented completely
   from its official documents and tested against official examples or
   simulators, independent of any machine. Machines are then added as configs
   on top, by us or the community. "Complete" means a written scope: the
   versions, actor roles, message types, options, acknowledgment modes and
   limits supported, with explicit exclusions.

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
- The diagram shows how data flows, not how crates depend on each other.
  Crate dependencies form an acyclic graph of ownership: shared model types
  are used by protocol and storage crates, and the program wires dialogs to
  host services. A dialog may acknowledge data only after the host reports
  that it is durably stored.
- Planned workspace crates (one reason to change each):

| Crate               | Owns                                                         |
| ------------------- | ------------------------------------------------------------ |
| `analyte-model`     | Common model: results, orders, identities, revisions         |
| `analyte-store`     | Local storage (SQLite), outbox, audit                        |
| `analyte-transport` | Serial, TCP, folder                                          |
| `analyte-framing`   | ASTM, MLLP, VIDAS and Autolyser envelopes, lines               |
| `analyte-dialog`    | ASTM, HL7, LAW, POCT, VIDAS and Autolyser conversations                                  |
| `analyte-astm`      | ASTM record syntax                                           |
| `analyte-hl7`       | HL7 v2 syntax, acknowledgments, LAW transactions             |
| `analyte-config`    | Machine config files (TOML), validation, versions            |
| `analyte-mapping`   | Parsed messages → common model, driven by machine config     |
| `analyte-contract`  | The app contract: documents, signing, validation (ADR 0008)   |
| `analyte-sink`      | Delivery to apps and order polling                           |
| `analyte-sim`       | Fake machine and replay of recordings                        |
| `analyte-poct`      | POCT01-A2 DML Observation Reviewer (XML)                     |
| `analyte-vidas`     | VIDAS tagged message syntax                    |
| `analyte-autolyser` | Autolyser fixed-width record syntax        |
| `analyte-receiver`  | Reference receiver: a small app that implements the contract |
| `analyte`           | The program: service, CLI, admin page                        |

Vendor syntax and POCT1-A have their own crates, so a new vendor format is a
new crate rather than a change to a shared one.

## 6. Common model

- **Three identities:**
  1. the receipt: what arrived from the machine, with its raw bytes;
  2. the delivery: one ID per delivery to the app, unchanged across retries;
  3. the result: a stable observation identity plus a revision number that
     Analyte assigns, so corrections and reruns are never confused with
     duplicates. The source's own status or action (for example final,
     corrected, deleted) is kept as sent next to it and is never used as the
     revision itself: a correction can arrive without the original, and one
     result can span several source segments.
- **Source structure kept:** the protocol's own hierarchy (for ASTM: message,
  patient, order, result, comment), sequence numbers and every source status
  code are kept as sent, next to Analyte's own summary; they are never
  flattened into final/corrected/cancelled. Training and debug messages
  never reach production delivery.
- **Identity evidence:** IDs keep their namespace (which system assigned
  them). Each machine config declares how a retransmission, a correction and
  a new run are told apart. When the machine does not send enough to tell,
  the receipt is kept with its identity marked unresolved and shown to an
  operator; Analyte never invents a result key from sample, test, value or
  time. Different content under the same trusted identity is an error, never
  a silent replacement.
- **Revisions** are assigned by the store from the identity and the retained
  history, in the same transaction as the interpretation (rules in ADR 0003).
- **Sample identity:** ID as a string with its source, plus position (rack or
  disk, slot) when the machine reports one.
- **Values:** original text, optional fully decoded protocol-neutral text (absent
  for unsupported escapes or formatting, which require parking), parsed number with comparator and precision when
  it parses, original unit, mapped UCUM unit when mapped, dilution and whether
  it was applied.
- **Flags, ranges, comments, QC class (patient, control, calibration or
  unknown), machine times and receive time.** Times keep their original text,
  precision and offset; protocol deadlines use a monotonic clock.
- **Order lifecycle:** requested → sent → accepted by machine → done, or
  cancelled / rejected, each with its time and source. "Cancel requested"
  and "cancel confirmed" are separate states. A timeout after sending an
  order is an uncertain outcome to reconcile, never a reason to resend.

## 7. Contract with lab apps

- **Results:** before sending, Analyte stores the exact payload bytes, their
  digest, the contract version, the destination and the delivery ID. Retries
  and resends reuse the same bytes and ID. The app stores the ID together
  with its effects before answering; a repeated identical request gets the
  original answer, and the same ID with different content is a conflict.
  Each answer is classed as done, retry later (backoff, honouring
  Retry-After), fix the setup (for example a rejected credential) or parked
  with the app's reason. A correction is never delivered before the result it
  corrects.
- **Authentication:** Standard Webhooks signing with key IDs and rotation, selected
  by [ADR 0008](adr/0008-app-contract.md).
- **Orders:** poll the app and answer only from trusted cached work, following
  [ADR 0004](adr/0004-outbound-only-polled-orders.md).
- **Versioned and documented** so any app can implement it.
- **Reference receiver:** a small server in this repository implements the
  whole contract. Tests run against it, and it is the worked example for app
  developers. Because it is ours, it cannot prove interoperability on its
  own: the contract also publishes test vectors, including invalid cases,
  that any app can run. Packages for other languages come later.
- FHIR output is an option for later, not the base format.

## 8. Protocol support

| Protocol          | Source documents                                        | How it is tested before real machines |
| ----------------- | ------------------------------------------------------- | ------------------------------------- |
| ASTM              | CLSI LIS01-A2 + LIS02-A2, vendor manuals                | Examples in the standards, then vendor traces |
| HL7 v2 + MLLP     | HL7 v2.5.1 and v2.3.1, MLLP Release 1                    | Examples in the standard, HL7 tools, then vendor traces |
| IHE LAW           | IHE PaLM Technical Framework Vol. 1, 2b and 2x (§2.2.4), Rev 11.0; IHE General Introduction Appendix E; HL7 v2.5.1 | IHE Gazelle simulator |
| POCT1-A           | CLSI POCT01-A2 (Device Messaging Layer)                  | Documents only at first               |
| Vendor formats    | Each vendor's interface manual                          | Vendor examples, recordings           |

Reference terminologies: LOINC (test codes) and UCUM (units).

The scope Analyte supports in each standard, and its conformance checklist,
are in [protocols/](protocols/README.md). Analyte's role: in IHE LAW it is the
Analyzer Manager; in POCT01-A2 it is the DML Observation Reviewer. The other
side of each standard exists only in the test peer.

Machine documents used for the first machines:

| Machine              | Documents                                                                           |
| -------------------- | ----------------------------------------------------------------------------------- |
| Roche cobas e 411    | Host Interface Manual v2.3 (v2.2 also read); Operator's Manual v3.3.1               |
| YHLO iFlash 1200     | YHLO LIS Protocol Manual V1.1 (iFlash family); iFlash 1200 Operation Manual V3.0    |
| Boule Swelab Lumi    | Lumi User Manual 1504529; Boule's LIS protocol description for the Lumi (still needed) |
| BioCARE Biolyte 2000 | Service Manual 4.2A, Appendix VI                                                    |
| bioMérieux VIDAS     | Research: BCI Link guides 161150-486 A/B and VIDAS PC User Manual F; implementation from research notes |
| Dialab Autolyser     | Autolyser / Autolyser ISE Operator's Manual Rev18; Service Manual Rev01             |

## 9. First machines

These six machines from one lab are the hardware test bed: the first real
machines to reach the proven level. They are added as configs (or vendor
format pieces) after the standards they use are complete. Recordings from
them can be collected at any time and kept as test data.

| Machine                | Protocol                          | Direction                         | Evidence today | Missing                                          |
| ---------------------- | --------------------------------- | --------------------------------- | -------------- | ------------------------------------------------ |
| Roche cobas e 411      | ASTM over serial                  | Results, host query, orders       | documented     | Lab's host code table, trace log copy            |
| YHLO iFlash 1200       | HL7 2.3.1 over MLLP (TCP), from the iFlash family manual | Results, query (QRY/QCK/DSR)      | documented     | Protocol confirmed for the 1200, port            |
| Boule Swelab Lumi      | HL7 over TCP                      | Results, sample lookup            | documented     | Sample ID field, lookup message type             |
| BioCARE Biolyte 2000   | ASTM over serial                  | Results only                      | documented     | One real recording                               |
| bioMérieux VIDAS (PC)  | Vendor "Literal" over serial      | Results, work list download       | documented     | VIDAS PC version (sample ID length 12 or 17)     |
| Dialab Autolyser       | Vendor format, host polls         | Results when asked, work list     | documented     | TCP settings; barcode reader retrofit            |

## 10. Testing

- **Recordings:** retain and replay synthetic or de-identified conversations in
  the format owned by [ADR 0009](adr/0009-recording-format.md). The program can
  opt each machine into protected live capture (`capture = true`); capture failure
  is separate from clinical custody and remains visible in status.
- **Failure paths:** broken checksums, lost acknowledgments, split and merged
  reads, timeouts, disconnects, restart between storing and acknowledging,
  disk full, corrected and repeated results, late query replies; a crash
  just before and just after every positive acknowledgment; the app's answer
  lost after it stored a delivery; same-ID conflicts; an incomplete order
  feed; cancellation races; restore from backup; clock jumps; character sets
  and escape rules; file handoff.
- **Property tests** for framers and parsers; **fuzzing** on Linux CI.
- **Validation pack for labs:** the cases a lab must check before go-live and
  after changes, all the way to the screen the doctor sees (CAP and ISO 15189
  require this).

## 11. Operations

- One program, installed as a Windows service or Linux systemd unit, starting
  without a login.
- Local admin page (localhost, bearer token): machines, last messages, parked
  items, resend and reprocess as separate actions, all audited.
- Alerts on oldest undelivered item, stalled machines and disk space.
- The PC must not sleep; updates are scheduled; USB serial reconnects and COM
  port changes are handled.
- Backups of the store; a tested restore. Restoring an older store
  reconciles deliveries and orders before any machine gets a command.
- **Machine configs:** the shipped definition, the site settings and local
  overrides are kept apart. Every interpretation stores the exact effective
  config and its digest. A config goes draft → validated → active and
  switches only between conversations; activation never changes queued
  deliveries or resends an order. Store and config formats are versioned,
  with rehearsed migrations.
- **Monitoring:** pending bytes and oldest item age, storage commit time,
  order cache freshness and uncertain orders, each alert with a named
  responder.
- **Before a pilot:** the supported outage length, disk capacity and reserve,
  retention, and backup and restore targets are written down.

## 12. Security, privacy and regulation

- Analyzer ports listen only on the lab network; outbound HTTPS with verified
  certificates and rotated credentials.
- Health data stays in the protected store; logs carry IDs, not payloads.
- GDPR roles and retention are decided per deployment. Nothing is deleted
  silently; if a deployment needs data to expire, that is a separate audited
  policy.
- EU: software that only moves data is often not a medical device, but sending
  work lists to analyzers needs an intended-purpose statement and an
  assessment under MDCG 2019-11 before any claim is made.

## 13. Phases

| #   | Work                                                                                | Done when                                  |
| --- | ----------------------------------------------------------------------------------- | ------------------------------------------ |
| 0   | Name, license, repo, this plan, first ADRs                                          | Plan approved                              |
| 1   | Core: workspace, common model, store, outbox, delivery, order contract and order cache ("changes since" polling, freshness, deadlines), fake machine, first reference receiver, protected command-line view of parked items, CI | A fake result reaches the reference receiver and a fake query is answered from the cache, failure paths included |
| 2   | ASTM complete within [its scope](protocols/astm.md): serial and TCP transport, LIS01 framing, LIS02 records, all dialogs (machine pushes, machine asks, host sends ahead) | Every checklist row has passing tests (documented) |
| 3   | HL7 v2 complete within [its scope](protocols/hl7.md): 2.3.1 and 2.5.1 syntax, acknowledgments, MLLP | Every checklist row has passing tests, plus official examples and HL7 tools (documented) |
| 4   | IHE LAW as Analyzer Manager within [its scope](protocols/ihe-law.md): LAB-27 query, LAB-28 work order, LAB-29 results, selected options | Every checklist row has passing tests and a recorded Gazelle run (simulated) |
| 5   | POCT01-A2 as DML Observation Reviewer within [its scope](protocols/poct1-a.md) | Every checklist row has passing tests (documented) |
| 6   | Published contract: full reference receiver, contract docs and test vectors any app can run | The test vectors pass against the reference receiver end to end |
| 7   | Machine configs: cobas e 411, Biolyte 2000, iFlash 1200, Swelab Lumi        | Each config passes its vendor traces (simulated) |
| 8   | Vendor formats: VIDAS Literal, Autolyser (start/end framing, tags, fixed width, polling dialog) | Each piece passes its vendor examples (documented or simulated) |
| 9   | Real-lab pilot with the six machines, admin page, installers, restore rehearsal, alerts with named responders, release process, docs for adding a machine | The six machines reach proven at the lab; another lab can install it |
| 10  | Plugin interface, only if a real outside driver needs one                    | Contributors add machines                  |

Each standard's phase starts by writing its scope (see principle 7). A
standard is done when it conforms to its official documents within that
scope, at the documented or simulated level, never when one machine happens
to pass. The proven level comes only from real machines in phase 9.

## 14. Open decisions

1. Plugin mechanism (WASM components or out-of-process), decided only when a
   real outside driver needs one.

## 15. Research behind this plan

The detailed research (devices, OpenELIS Bridge internals, middleware
practices, idea check, Rust practices) lives in
[research/](research/README.md). Background and decisions are in
[context.md](context.md). Protocol notes for implementation go into
`docs/protocols/` in our own words as each piece is built.
