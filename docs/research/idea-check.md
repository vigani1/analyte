| Idea                               | Verdict                                                 | One-line reason                                                                                                      |
| ---------------------------------- | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| 1. Durable receipt before ACK      | Keep but change                                         | Define durability separately for ASTM frames, HL7 acceptance and application acceptance.                             |
| 2. Transactional outbox            | Keep but change                                         | A local durable queue plus an idempotent receiver is sufficient; retries alone cannot guarantee one clinical effect. |
| 3. Duplicate detection             | Replace with layered identities                         | Wire retries, HTTPS retries and repeat/corrected observations have different identities.                             |
| 4. Versioned machine profiles      | Keep                                                    | Pin the effective configuration and parser revision to each receipt; rollback must preserve queued work.             |
| 5. Mirth channels                  | Replace with a small pipeline and dialog state machines | Transformation pipelines fit results, but do not describe timed, bidirectional analyzer conversations.               |
| 6. Host queries                    | Keep but change                                         | Implement the vendor conversation, not a universal query/reply inferred from one project.                            |
| 7. DI configurable driver          | Keep as a limited reference                             | Public evidence supports configurable file exchange, not arbitrary serial drivers or a public plugin API.            |
| 8. Message browser and resend      | Keep but change                                         | Distinguish retrying an unchanged delivery from reinterpreting a clinical message.                                   |
| 9. Declarative mapping             | Keep but change                                         | Use bounded, typed mappings; move stateful or irregular behavior into small code modules.                            |
| 10. Simulator and recording/replay | Keep                                                    | Test entire conversations, failures and timing; a message sender alone is insufficient.                              |
| 11. WASM plugins                   | Keep but change                                         | Prefer Wasmtime components for distributable untrusted parsers, but defer a plugin subsystem until needed.           |

# Analyzer bridge: independent idea check

Research date: 2026-11-15. Research only: no software installation, implementation, device tests, commits or pushes. Recommendations concern the proposed independent Rust bridge, not approval to change EchoMed's product scope.

**Verified** means stated in a linked source read during this research, not tested on hardware. **Inferred** means engineering judgment or a proposed policy. **Unverified** identifies a material evidence gap. Vendor marketing establishes advertised features, not implementation quality. Search timestamps are not document revision dates.

The layers are useful, but not a strictly one-way stack: dialog sends framing commands and uses parsed query/order identities; transport reports disconnects and deadlines. Preserve raw bytes and provenance alongside the common model. One model must retain corrections, reruns, QC, specimen/container identity and unknown vendor flags rather than flatten everything into sample/test/value. These are inferred design requirements.

## 1. Durable receipt before acknowledging

**Verdict: keep but change.** This is established reliable-messaging practice, not an invention of OpenELIS. However, neither an ASTM ACK nor an HL7 ACK universally means that a clinical result was accepted by the lab app.

**Verified — ASTM implementation evidence:** Roche's e 411 manual, sections D and F, describes per-frame ACK/NAK and a 15-second sender timeout. Cepheid's E1381-02 state diagram also separates frame reception, repeats, retries and timeouts. These are vendor implementations of the standard, not proof that every LIS1 revision or device has identical timers. Roche e 411 Host Interface Manual v2.2, sections D and F; [Cepheid specification, section 3.2](https://infomine.cepheid.com/sites/default/files/2025-02/302-2261%2C%20Rev.%20F%20LIS%20Protocol%20Specification.pdf).

**Unverified:** the complete licensed E1381/LIS1 normative text was not available for clause-by-clause review. Do not describe “15 seconds” as a universal durability allowance. Qualify each manufacturer's revision and each timer's start event; sender response timeout and receiver inter-frame timeout are different timers.

**Verified — HL7:** MLLP supplies message boundaries, not the meaning of clinical acceptance. HL7 original mode uses application acknowledgment; enhanced mode separates accept acknowledgment (`CA/CE/CR`) from application acknowledgment (`AA/AE/AR`). MSH-15/16 negotiate the conditions; both absent/null select original mode. Enhanced acceptance considers safe storage. [HL7 v2.8.2 chapter 2, sections 2.9 and MSH/MSA definitions](https://hl7.eu/HL7v2x/v282/std282/ch02.html).

**Inferred implementation policy:**

- For ASTM, validate and durably journal each accepted frame before its ACK if the promise is to survive a crash after any ACK. Persist frame sequence/session state sufficiently to recognize a repeated preceding frame. Keep incomplete transfers separate from complete messages. Waiting for EOT before ACKing earlier frames deadlocks the sender.
- ENQ acceptance means ready to receive, not “message saved.” Reserve capacity before accepting a transfer. A full/unwritable disk cannot produce a positive data ACK.
- Commit a complete receipt and its processing obligation locally. Do not wait for cloud HTTPS, terminology lookup or clinical posting on the frame ACK path. Disk commit latency must fit comfortably inside the actual device timeout; measure on the target PC under load.
- Enhanced HL7 can send CA after durable acceptance, then the agreed application response. Do not silently replace an original-mode application ACK with CA. Original-mode AA is defensible only after the receiving application has completed its agreed responsibility; document whether the bridge is that application or merely an intermediary.
- If the device requires immediate original-mode AA, qualify that contract explicitly. Durable raw receipt alone does not prove semantic validity or remote clinical acceptance. Report subsequent processing failures visibly.

**Verified:** durable local storage depends on flush behavior and storage assumptions, not merely a successful buffered write. [SQLite atomic commit documentation](https://www.sqlite.org/atomiccommit.html). **Inferred:** use a local transactional store with appropriate durability settings rather than inventing a bespoke journal; the choice still needs failure testing.

## 2. Outbox, retries, dead letters and ordering

**Verdict: keep but change.** The relevant origin is reliable messaging's transactional outbox/idempotent-consumer patterns. A single-PC bridge needs neither a broker cluster nor distributed transactions.

**Verified:** the transactional outbox atomically records outgoing intent with local state changes. Its relay can publish twice after a crash; the consumer must tolerate duplicates. Ordering is a separate requirement. [Pattern author's outbox description](https://microservices.io/patterns/data/transactional-outbox.html), [idempotent consumer](https://microservices.io/patterns/communication-style/idempotent-consumer.html).

**Inferred recommendation:**

- Persist raw receipt plus pending interpretation in one transaction. Later persist the immutable normalized payload plus delivery obligation atomically. A failed parser must leave a visible retained receipt, not a silently empty outbox.
- A durable inbox/queue is enough if no second local state update is involved; “transactional outbox” should not justify extra tables or a framework. Direct HTTPS with no recoverable local intent is worse when offline operation is required. CDC adds little on this PC.
- Give each delivery a stable ID, payload digest and destination. The receiving lab app must atomically store that ID with its clinical effects, and reject reuse with different content. A lost HTTP response then causes a safe retry. “Any lab app” requires a documented receiver/adapter contract; the bridge alone cannot guarantee idempotent clinical posting.
- Retry connection failures, uncertain outcomes, rate limiting and transient server errors with capped exponential backoff and jitter; honor Retry-After. Retain the same ID and bytes. Pause authentication/configuration failures for repair. Park permanent validation errors with their specific response.
- Do not discard clinical data after an arbitrary attempt count. Retry transient outages while retained, alert on age/backlog/disk pressure, and stop accepting new analyzer data before storage exhaustion. Retention must cover the declared outage/reconciliation window.
- A dead letter is an operator-visible parked item with reason and history, not deletion. Permit controlled retry after repair. Set retention and escalation explicitly; no standards source found mandates a particular retry count.
- Preserve causal order for an observation's revisions. Begin with one delivery in flight per machine if that is sufficient; avoid one global queue that blocks unrelated machines. A parked final/correction blocks or explicitly quarantines dependent revisions. Never release an old final after a correction without receiver revision checks.

These policies provide at-least-once transport with idempotent effects, not universal exactly-once delivery.

## 3. Duplicate detection and clinical identity

**Verdict: replace byte-hash-only suppression with layered identities.** Exact bytes remain useful evidence; they are not a universal result identifier.

**Verified:** HL7 MSH-10 identifies a message and MSA-2 correlates its acknowledgment. It is not an observation identifier. [HL7 chapter 2](https://hl7.eu/HL7v2x/v282/std282/ch02.html). In v2.8.2, OBX-21 is persistent between messages when supported; the chapter cautions about use outside its established context. It is not available in Analyte's target editions: v2.3.1 has no OBX-21 and v2.5.1 reserves it (v2.5.1 ch.7 §7.4.2.21). OBX-3/4 identify observation/sub-ID context; OBX-11 distinguishes final, correction and deletion, among other states. [HL7 chapter 7, OBX definitions](https://hl7.eu/HL7v2x/v282/std282/ch07.html).

**Verified counterexample:** Iguana's published duplicate filter compares message hashes in a bounded lookback and permits a transform to remove trivial variation such as an MSH timestamp. Its documentation explicitly limits identical-message matching. This is a practical heuristic, not clinical reconciliation. [Iguana duplicate filter](https://help.interfaceware.com/v6/duplicate-filter).

**Inferred recommendation — three identities:**

1. **Wire receipt:** stable configured instrument identity plus sender namespace/control ID where reliable; exact payload hash detects unchanged retries and control-ID collisions. ASTM frame numbers only deduplicate frames within a conversation. Reopening a connection must not reset the instrument's durable identity.
2. **Delivery event:** a bridge-generated durable event ID, unchanged across automatic retries and operator resend. This solves bridge-to-app redelivery even if the app never sees the original protocol.
3. **Clinical observation and revision:** prefer a vendor-qualified persistent observation/run ID, with its namespace. Otherwise negotiate a profile key from specimen/accession with assigning authority, order identity, assay plus subtest/channel, instrument and measurement run/repeat identity. Include dilution or replicate identity where the vendor distinguishes them. Keep result status and revision history separately from the observation identity.

Instrument + sample + test + run time is only a fallback: samples are reused, time can be absent/coarse/wrong, and a correction can retain the original measurement time. Value equality alone cannot distinguish an identical rerun from retransmission. Do not globally strip timestamps/control IDs before hashing and assume safety.

An F-to-C transition must become a correction of the identified observation, retaining the old value. A new F from a distinct run is a rerun, even with the same value. Same observation/revision identity with conflicting content is a reconciliation error, not a silent drop or last-write-wins update. If the source lacks enough identity, retain and flag ambiguity; no algorithm can recover information the instrument never sends.

Preserve all receipts while linking confirmed duplicates to the existing event. Define dedupe retention at least as long as the supported retry/replay window, and prevent operator replay from bypassing expired receiver keys. Keep clinical reconciliation in the lab app; the bridge supplies source identities, status and evidence.

## 4. Versioned machine configurations and rollback

**Verdict: keep.** This comes from configuration management and reproducible deployment, with useful examples from interface engines. It does not require copying their deployment systems.

| Product                     | Verified public evidence                                                                                                                                                  | Limit of the evidence                                                                                                                                                                       |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Mirth Connect 4.5           | Channels have saved/deployed revisions and XML export; editing and deployment are separate.                                                                               | A channel revision is not proof of a complete dependency snapshot or reversible database migration.                                                                                         |
| Rhapsody Integration Engine | Vendor white paper advertises configuration version control.                                                                                                              | Detailed documentation redirected to authentication; migration format, rollback behavior and dependency atomicity were not verified. Do not substitute IBM Rational Rhapsody documentation. |
| Iguana 6 / IguanaX          | V6 Translator runs a selected commit and supports restoring commits. IguanaX manages components/libraries with Git, as repositories or folders in an instance repository. | Script/component history is not proof that every secret, queued message and runtime setting rolls back with it.                                                                             |
| Data Innovations            | Public driver library lists driver name, mode and version, with download/detail access.                                                                                   | Public material reviewed does not establish customer configuration Git, atomic migration or rollback guarantees.                                                                            |

Sources: [Mirth 4.5 vendor guide, channel summary/export sections](https://s3.amazonaws.com/downloads.mirthcorp.com/connect-user-guide/latest/mirth-connect-user-guide.pdf), [Rhapsody white paper](https://rhapsody.health/wp-content/uploads/2022/01/Integration-Strategies-for-Health-IT-Vendors-Whitepaper-032020-Screen.pdf), [Iguana 6 version control](https://help.interfaceware.com/v6/version-control), [IguanaX components](https://docs.interfaceware.com/docs/IguanaX_Documentation_Home/Application/Components_2C_Libraries_and_Collections/Components/Components_2725642309/), [DI community guide, driver library](https://support.datainnovations.com/wp-content/uploads/2023/10/My%20DI%20Community%20Quick%20Start%20Guide.pdf). Mirth PDF passages were available through indexed vendor text; full browser fetch exceeded its size limit.

**Inferred policy:** distinguish schema version, immutable profile revision and site machine settings. Keep secrets outside distributable profiles. Validate a candidate, show its effective changes, run fixtures, then activate between conversations. Persist the effective profile content/digest and parser/build identity with each receipt, not only a mutable filename.

Migrate into a new candidate without overwriting the original; reject unsupported versions with a useful error. Roll back by activating the previous compatible revision. Unsent rendered deliveries keep their original payloads; already posted results are not undone by rolling back config. Reinterpreting old raw data is a separate audited operation. Store/schema upgrades need an explicit compatibility/backup check before downgrade; a Git revert alone is insufficient.

## 5. Mirth's channel model

**Verdict: replace the full channel system with a small fixed pipeline plus explicit dialog state machines.** Keep separation of input, validation, transformation and delivery.

**Verified:** Mirth is a general integration engine with source/destination connectors, filters and transformers; its public source is available for inspection. [Vendor repository](https://github.com/nextgenhealthcare/connect). Iguana documents single-stage versus queued two-stage processing, illustrating that a general channel graph is not necessary for every interface. [Iguana interface structure](https://help.interfaceware.com/v6/design-your-interface-structure).

**Inferred:** a source-to-destination pipeline works well for complete pushed results. It is a poor sole abstraction for ENQ contention, frame retransmission, queries awaiting worklists, polling and cancellation. Those need connection-owned state, deadlines and correlated replies. Do not represent each ACK as a new general-purpose channel message.

Use transport + reusable framing codecs + a dialog strategy + syntax parser + profile mapping + durable delivery. Dialog strategies may call parsing before selecting a reply. Define capabilities so invalid combinations fail configuration validation; not every transport/framer/dialog/syntax combination is meaningful. Keep driver code as the escape hatch instead of building a visual workflow engine, arbitrary routing graph or expression language. General engines remain alternatives when a deployment actually needs many unrelated destinations and hospital workflows.

## 6. Host-query conversations

**Verdict: keep but change.** Universalis is one implementation example, not the authority for ASTM or every Siemens instrument.

**Verified — LIS2 scope:** the public CLSI preview identifies section 11's Request Information Record, including range IDs, test ID, time limits and request status; section 12 defines termination. The complete normative field rules were not publicly verified. [CLSI LIS2-A2 preview](https://webstore.ansi.org/preview-pages/clsi/preview_lis2-a2.pdf). A manufacturer example shows that Q fields may be unused or restricted rather than implemented generically. [ORGENTEC TB2010](https://www.orgentec.com/images/files/tb%202010.pdf). QIAGEN specifies a query response containing header, patient and order records in a single message. [QIAGEN data interface guide, section 3.6](https://www.qiagen.com/pr/resources/download.aspx?id=47567666-86eb-4931-81de-cffb8ea14556&lang=en).

**Verified — e 411:** Roche v2.2 section F requires returning sequence, carrier and position unchanged. Disk query cancellation occurs after 15 seconds; rack communication has an approximately 40-second limit. Q request status `O` requests orders and `A` cancels. Elecsys and cobas dialects use different no-order O-record examples: do not substitute a generic empty response. Barcode-error/generated identifiers can prevent host identification. Roche e 411 Host Interface Manual v2.2, F-2–F-5. The manual is dated 2013; it is not confirmation for the installed firmware.

**Verified — LAW 1.5:** pull supports specimen and query-all; push is also supported. LAB-27 QBP/RSP acknowledges the query; LAB-28 OML/ORL transfers work separately. QAK OK is query acceptance, not proof that orders exist. No work uses a negative LAB-28 response with ORC-1 DC. RCP requests immediate/real-time handling, but the work-response deadline is analyzer-specific. Late negative replies can be rejected. Cancellation can succeed or return unable-to-cancel, and duplicate AWOS requests are rejected individually. [IHE LAW final text, X.2, W.1.3.2, 3.Q and 3.R](https://www.ihe.net/uploadedFiles/Documents/PaLM/IHE_PaLM_Suppl_LAW_Rev1.5_FT_2016-02-24.pdf).

**Inferred policy:** maintain an outstanding-query context with deadline, exact source identifiers and cancellation state. Distinguish known specimen/no eligible orders, unknown specimen, invalid query and lookup unavailable internally, even if a vendor maps two outcomes to the same wire reply. Never turn cloud failure into a fabricated clinical “no orders.” A late HTTPS lookup cannot revive a cancelled conversation or accidentally answer the next sample.

Outbound HTTPS can service lookups, but WAN latency must fit the device's deadline including serial turnaround/transmission. Where that cannot be assured, prefetch a narrowly defined worklist with freshness/revocation rules or explicitly declare host-query mode unavailable offline. A stale cache cannot authorize an already cancelled test. These are actual order-boundary concerns, not reasons to build a general scheduling subsystem.

Qualify duplicate queries, simultaneous result traffic, link contention, missing barcode, no orders, disconnect, timeout, cancellation before reply and cancellation after work starts. Define whether a repeat query returns the same assigned work; the bridge must not independently invent order allocation policy.

## 7. Data Innovations configurable delimited driver

**Verdict: keep as a reference for file mapping; drop the assumption that it is a universal device-driver architecture.**

**Verified:** DI says most drivers are model/vendor-specific and based on vendor specifications. Its configurable driver supports File I/O and FTP, configurable extensions and field/component/repeat delimiters, bidirectional order/result file structures, timed/per-specimen order files and worklist-driver plate maps. Incoming results can start after a line/text marker, add sequence numbers for repeated tests and group lines by specimen. [DI's own feature description](https://support.datainnovations.com/resource/tips-advanced-configurable-delimited-instrument-driver/). The library identifies `diicfgdi` as a versioned bidirectional driver. [DI library guide](https://support.datainnovations.com/wp-content/uploads/2023/10/My%20DI%20Community%20Quick%20Start%20Guide.pdf).

**Unverified:** driver source/ABI, arbitrary checksum or dialog programmability, serial support for this particular configurable driver, config schema, duplicate clinical handling, migration and rollback internals. Do not infer these from the word “advanced” or the size of the driver library.

**Inferred:** adopt bounded selectors, grouping and explicit replicate identity for file adapters. Safe file acquisition also needs completed-file detection and recoverable receipt before removal. Keep proprietary live dialogs in code. There is no public evidence here for copying DI's internal plugin mechanism.

## 8. Message browser with resend

**Verdict: keep but change.** Browsability is an operational recovery tool; a resend button is not an idempotency mechanism.

**Verified:** Mirth reprocessing can overwrite its existing stored message or create a new one, select destinations, and rerun the source connector. These are engine-history controls, not a documented guarantee about downstream clinical uniqueness. [Mirth guide, Reprocess Message](https://s3.amazonaws.com/downloads.mirthcorp.com/connect-user-guide/latest/mirth-connect-user-guide.pdf). Iguana exposes single/multiple log resubmission and queue repositioning. [IguanaX log resubmission](https://docs.interfaceware.com/docs/IguanaX_Documentation_Home/Application/Logs/Log_Resubmission/Log_Resubmission_2715091111/).

**Unverified:** no public primary documentation reviewed establishes that Rhapsody resubmission automatically prevents duplicate clinical results; its detailed docs require authentication. Nor do the Mirth/Iguana sources establish such a universal guarantee. Do not equate “overwrite engine message” with “replace lab result.”

**Inferred design:**

- **Retry delivery:** resend the saved payload to the same destination with the original event ID. Show prior attempts and whether the last outcome is uncertain. Receiver idempotency is mandatory for safe repeat effects.
- **Reinterpret receipt:** run a selected new mapping/parser against retained raw data, show the normalized difference, and record lineage. A changed payload gets a new revision/event identity and explicit clinical correction/reconciliation semantics; never reuse an idempotency key for different bytes.
- **Replay to analyzer:** keep out of the ordinary result-resend action. Replaying an order can cause physical testing; the outbox's HTTPS guarantees do not make it safe.

Show received, parsed, parked, delivered and remotely accepted/rejected states separately. Keep operator, reason, destination and profile revision in the history. Restrict raw-data access and sanitize exported fixtures; clinical message storage is not an ordinary debug log.

## 9. Declarative field mapping

**Verdict: keep but change.** Borrow the bounded mapping concept, not a universal configurable language.

**Verified:** hl7-converter offers JSON separators, field/component references, positional or input-driven generation, aliases and validation. It expressly excludes orchestration and deep workflows; its JavaScript hooks are trusted code without a built-in sandbox/timeout. [Project README](https://github.com/singl3focus/hl7-converter). Iguana's Translator is Lua code with versioned projects, not merely a declarative field mapper. [Translator versioning](https://help.interfaceware.com/v6/version-control). Mirth combines mapping with a more general transformer model. [Mirth source](https://github.com/nextgenhealthcare/connect).

**Inferred:** config should select fields within explicit record/group context, declare cardinality, map assay/unit/status codes, parse documented decimals/timestamps, and set defaults only where justified. Validate references when loading the profile and validate required normalized values at the boundary. Retain original values and vendor codes.

“Delimited” can share low-level utilities, but HL7 and ASTM need distinct syntax rules: MSH separator handling, escapes, repetitions, record hierarchy and patient/order/result grouping cannot be implemented safely by generic string splitting. CSV quoting, XML namespaces and fixed-width byte-versus-character offsets likewise belong to their parsers.

Mappings break down at multi-specimen messages, context-sensitive repeated records, reflex/rerun relationships, vendor binary payloads, conditional checksums and timed query state. A small pure code module is clearer than accumulating expressions and hidden state in config. Keep code deterministic where possible, with explicit inputs/outputs and fixture examples. Field selection never substitutes for clinical identity or correction policy.

## 10. Simulator and conversation recording/replay

**Verdict: keep.** Protocol simulation and capture/replay are established testing techniques; none of the tools below proves device conformance by itself.

**Verified tool scope:**

- [HL7 Soup tutorials](https://www.integrationsoup.com/HL7Tutorials.html) document sending/receiving messages. [7Edit](https://7edit.com/features/) advertises TCP/IP, serial and MLLP exchange. Useful for HL7 payload/ACK tests; this does not establish ASTM handshake or proprietary analyzer emulation.
- [SENAITE ASTM middleware](https://github.com/senaite/senaite.astm) includes a frame-by-frame simulator with configurable delay. [LACS](https://github.com/libinfaby/LACS) advertises analyzer configuration, generated results and ASTM/HL7 templates. LACS is a candidate, not a verified conformance suite; no runtime test or exhaustive protocol audit was performed.
- [IHE Gazelle Order Manager](https://connectathon.ihe-catalyst.net/gazelle-documentation/Order-Manager/user.html) documents LAW analyzer/manager scenarios. This is closer to work-order conversation testing than an HL7 message editor.

**Verified Windows capture options:** [AGG's serial sniffer](https://www.aggsoft.com/serial-port-monitor/serial-port-sniffer.htm) documents driver-based monitoring and attachment before application open; [Eltima's UART sniffer](https://www.eltima.com/uart-sniffer.html) documents software monitoring. Driver/OS compatibility and timing impact must be qualified locally. [com0com](https://com0com.sourceforge.net/) creates virtual null-modem pairs; hub4com/com2tcp are related tools, not passive taps of an existing physical port. [IO Ninja hardware manual](https://ioninja.com/doc/hw/ioninja-hw-man.pdf) documents an RS-232/RS-485/TTL serial tap.

**Inferred capture plan:**

- Prefer recording inside our own transport once it owns the connection. For an existing vendor application use a qualified OS sniffer or receive-only hardware tap; a second terminal normally cannot share its exclusive COM handle.
- A hardware capture needs both directions, correct RS-232 voltage conversion, correct grounding/isolation and any relevant control-line changes. Do not connect a TTL logic input directly to RS-232 or join two transmitters with a simple Y cable.
- Record direction, exact bytes, monotonic timing, port settings, session boundaries, disconnects and relevant control signals. A printable transcript loses control bytes and checksum evidence. Deidentify before sharing while preserving lengths/relationships and recalculating checksums.
- Replay against an isolated bridge/test receiver. Script the peer's responses and timing rather than blindly sending a captured byte stream. Use real captured sequences to qualify the simulator; otherwise both can reproduce the same mistaken assumptions.
- Include fragmented/coalesced TCP reads, byte-at-a-time serial input, repeated frames, bad checksum, lost ACK, contention, partial disconnect, disk-write failure, lost HTTPS response, rerun/correction and late query reply. Restart after durable receipt but before ACK/delivery marking.

No capture tools were installed and no live session was disturbed. Virtual ports do not test physical wiring, flow control or USB adapter behavior; finish qualification with the real model/firmware and cable.

## 11. Third-party plugin mechanism for Rust in late 2026

**Verdict: keep WASM as the preferred future distribution boundary, but change its scope and defer implementation.** Begin with reviewed built-in Rust drivers and a small typed seam. If independently shipped third-party parsers are actually required, prefer Wasmtime components/WIT. There is no evidence that every driver needs runtime loading now.

| Option                   | Verified mechanism                                                                                                   | Inferred fit/trade-off                                                                                                                                                                    |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Wasmtime component model | Typed WIT interfaces and generated host/guest bindings; sandboxed execution with configurable resource controls.     | Best long-term fit for a Rust-owned typed parser API; contributor toolchains and component packaging add work.                                                                            |
| Extism                   | Wasm modules, PDKs, byte-oriented calls and explicit host functions; manifest includes resource/capability settings. | Quicker multilingual plugin onboarding for simple complete-message conversion; application schema/versioning still belongs to us. It is not the same ABI as WIT components.               |
| Native dynamic libraries | Rust FFI crosses an unsafe boundary; Rust ABI has no stability guarantee.                                            | Avoid for arbitrary contributors: in-process crash/corruption risk, OS/architecture builds and ABI ownership. A C ABI does not add sandboxing.                                            |
| Rhai                     | Rust-embedded scripting with operation/progress controls.                                                            | Good for trusted small transforms; less build friction, but shared process/host functions require careful limits. Not a blanket isolation guarantee.                                      |
| Lua                      | Embeddable runtime, selectable libraries and hooks.                                                                  | Accessible to interface developers; host must restrict filesystem/OS/native-module access and enforce execution/memory policy. Familiarity can outweigh Wasm tooling for trusted scripts. |
| Out-of-process driver    | OS process boundary and IPC; no particular framework is prescribed here.                                             | Best exception for vendor SDKs/native dependencies or full transport ownership. Separate crash recovery and packaging costs; process separation alone is not a permissions sandbox.       |

Sources: [Wasmtime plugin example](https://docs.wasmtime.dev/wasip2-plugins.html), [Wasmtime security model](https://docs.wasmtime.dev/security.html), [Wasmtime Config API](https://docs.wasmtime.dev/api/wasmtime/struct.Config.html), [Extism plugins](https://extism.org/docs/concepts/plug-in/), [Extism manifest](https://extism.org/docs/concepts/manifest/), [Rust FFI/ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html), [Rhai execution limits](https://rhai.rs/book/safety/progress.html), [Lua manual, libraries and hooks](https://www.lua.org/manual/5.4/manual.html).

**Inferred boundary:** keep port ownership, buffering, protocol deadlines, durable writes, credentials and HTTPS in the host. Invoke plugins on bounded complete messages or chunks, not once per byte. A proprietary framer may need incremental calls; expose bounded state and byte batches while the host owns scheduling. Full third-party dialog code needs a separate capability review, not accidental access through a field mapper.

At 9600 baud with 8N1, the theoretical ceiling is 960 bytes/second; at 115200 it is 11,520. This arithmetic suggests byte-processing speed rarely decides the mechanism. It is not a benchmark: JIT startup, GC, blocking host functions and scheduling can still miss a deadline. Preload outside conversations and measure worst-case latency on Windows.

Use explicit host capabilities, no inherited filesystem/network access, bounded input/output/memory and execution budgets. Wasmtime fuel/epoch controls bound guest execution only when configured; blocking host calls need their own deadlines. Runtime security updates remain necessary. Do not promise that “WASM is safe” without specifying the host API.

Current Wasmtime API pages identify a development build; this research does not choose that build or promise every contributor language has equal component support. Choose a supported release and verify its target/toolchain matrix at implementation time. Extism is a reasonable replacement if byte-in/byte-out PDK ease proves more valuable than typed WIT; avoid supporting both initially.

## What this changes in the proposed design

**Inferred conclusion:** retain the layered bridge, durable receipt/delivery, immutable profiles, bounded mappings and conversation fixtures. Replace one-way-pipeline assumptions with connection-owned dialog state, and replace “hash equals duplicate result” with separate receipt, delivery and observation identities. Make receiver idempotency and correction semantics part of the public app contract. Keep plugin distribution and a general integration-engine UI out of the first implementation unless a concrete driver requires them.

The unresolved qualification inputs are the licensed standards text, exact installed vendor manuals/firmware, receiver acceptance contract and real conversations. DI/Rhapsody internals remain evidence gaps rather than invented best practices. This note recommends design choices; it does not establish clinical readiness.
