# Adding a vendor protocol

Use this guide only when the confirmed vendor interface cannot be expressed by
an existing protocol and machine configuration. Read the plan and
[ADR 0012](../adr/0012-uniform-layout-and-naming.md) first. A new protocol family,
dependency or subsystem outside the plan requires the owner's decision before
implementation. Obtain the official manual; keep it in ignored `references/`.
Write notes in our own words and cite sections in tests.

## Size and ownership

The eighth review used VIDAS as its yardstick: about 13 new implementation
entries and about 25 existing files to edit. These are planning estimates,
not literal file totals: entries such as a crate or `syntax/` expand into
several files, and four protocol documents and a fuzz target are additional.
A protocol without orders or unique framing needs fewer entries. Exhaustive
matches deliberately expose integration work; there is no plugin subsystem.
Since 2026-11-18 the program crate's share is four existing files: the
protocol registry `machine/protocols.rs`, two module lists and the parking
reason pins.

## Where each piece lives

This is the code template of [ADR 0012](../adr/0012-uniform-layout-and-naming.md).
A concern the protocol lacks has no file; a file grows into a folder of the same
name only for separate responsibilities.

| Concern | Place |
| --- | --- |
| Message boundaries and checksums | `crates/steps/framing/src/<key>.rs` |
| Conversation state, deadlines, acknowledgment choice | `crates/steps/dialog/src/<key>.rs` |
| Stored order outcome codes and their spellings | `crates/steps/dialog/src/<key>/outcome.rs`; codes for every protocol and the spelling test in `crates/steps/dialog/src/outcome/` |
| Reading bytes into structure | `crates/protocols/<key>/src/syntax.rs` (a profile: `crates/protocols/hl7/src/<key>/`) |
| Writing every outgoing message (orders, work lists, envelopes, acknowledgments, query replies) | `crates/protocols/<key>/src/encode.rs` or `encode/` |
| Typed named-field access; version rules | `views.rs`; `version.rs` |
| Machine terms to the common model and back | `crates/steps/mapping/src/<key>.rs` |
| Connection driving (I/O, store calls, timers) and parking reasons | `crates/analyte/src/machine/<key>/mod.rs`; spellings pinned in `machine/receive/bytes.rs` |
| Order queries and work sent ahead; their failures | `machine/<key>/orders.rs`; `machine/<key>/error.rs` |
| Registration (session, link, startup recovery rule, host run, worker error, stored envelope, receipt reader) | One arm per match in `crates/analyte/src/machine/protocols.rs` |

Reading an incoming message, a machine query included, stays in `syntax` and
`views`. ASTM adds `codes.rs` and `query.rs`; HL7 adds `datatypes.rs`,
`order/` and `query/`, and its `encode/wire.rs` writes size-capped segments for
every HL7 and LAW writer. `crates/analyte/src/orders/` keeps only the steps
every protocol shares; HL7 and LAW keep a separate `OrderError` there because
their order code raises shared causes directly.

**Layer contracts.** Decoders take one byte at a time with `push(byte)` and end
with `finish()`; Autolyser's `finish()` returns the partial frame so its bytes
can be stored. Hosts journal bytes before feeding them. Every dialog has one
entry, `step(now, event) -> Result<Vec<Action>, Error>`, which rejects invalid
monotonic time before changing state. Timers stay in the dialog.

**Hosts.** Each protocol has a `Host` that borrows the connection's evidence,
settings and store; the caller owns them through shutdown. VIDAS, Autolyser and
POCT implement the driver's `Protocol` trait, and its `serve` owns the receive
loop (stop, then deadline, then input), journals each chunk, feeds it one byte
at a time and always attempts `close` and the final flush. ASTM, HL7 and LAW
still use `Effects` and `Drain`, whose one `drain` loop runs each action; LAW
keeps its own loop because each effect has its own deadline. Every `drain` and
`effect` returns `Result<Flow, Error>`. An effect that ends the session after
the queued actions calls `Session::stop_when_drained`; a failure that must wait
for queued replies is reported when the queue ends. Watched-folder stacks have
no stream session and use `Link::Folder`. Never mix POCT's host-buffer scanning
and stream-owned buffering on one decoder.

## New files

- Create `crates/protocols/<key>/Cargo.toml`, `README.md` and `src/lib.rs`;
  implement bounded grammar in `src/syntax.rs` or `src/syntax/mod.rs`, and
  outgoing messages in `src/encode.rs` or `src/encode/mod.rs`.
  Follow ADR 0012 for views and versions when needed.
  Keep I/O, storage, credentials and deadlines outside the protocol crate.
- Add `crates/steps/framing/src/<key>.rs` for distinct framing,
  `crates/steps/dialog/src/<key>.rs` for the conversation state machine and
  `crates/steps/mapping/src/<key>.rs` for common-model translation.
  Grow each into a folder only for separate responsibilities.
- Add `crates/core/config/src/schema/<key>.rs` for raw configuration and
  `crates/core/config/src/stack/<key>.rs` for validated settings.
- Add `crates/analyte/src/machine/<key>/{mod,error,settings}.rs` for host
  driving, specific failures and dialog construction. The host owns bounded
  I/O, durable storage and effect deadlines. Its `mod.rs` also holds the
  parking reasons it stores, as constants or small text functions.
- Add a `<key>` function to `crates/analyte/src/reading.rs` for reading stored
  receipts.
- If orders are supported, add `crates/analyte/src/machine/<key>/orders.rs`
  with its failures in the host's `error.rs`, and
  `crates/steps/dialog/src/<key>/outcome.rs`. The outcome file declares one
  `Code` enum of fixed stored codes with the `codes!` macro and, when
  codes carry text, one `Detail` enum; pin every new spelling in
  `crates/steps/dialog/src/outcome/tests.rs`.
  When the host stores an order outcome as its session ends, add
  `machine/<key>/outcome.rs` that works the code out from the session result.
  Preserve delivery and result identities; retries and replay must never cause
  an order resend.
- Add `docs/protocols/<key>/{README,decisions,checklist,versions}.md` with
  source editions, scope, evidence level and named conformance tests.
- Add a bounded byte-parser fuzz target in `fuzz/fuzz_targets/` and synthetic
  or de-identified tests and recordings in the owning test folders.

## Existing edits

Check these dispatch and registration points; omit an edit only when its
concern is absent from the protocol:

| Owner | Files and work |
| --- | --- |
| Workspace | `Cargo.toml`: crate membership and shared dependency registration; `Cargo.lock`: regenerate through Cargo |
| Config | `src/lib.rs`, `orders/mod.rs`, `orders/validation`, `schema/mod.rs`, `selection`, `stack/mod.rs`, `stack/capabilities.rs`, `validation/stack`: protocol selection, validation, order support and capability row |
| Steps | `framing/src/lib.rs`, `dialog/src/lib.rs`, `mapping/src/lib.rs`, `mapping/src/inbound/evidence.rs`, `mapping/src/orders/mod.rs`: register owned layers and evidence |
| Program registry | `machine/protocols.rs`: one arm in each match (stream session, link, chunk-journal rule for startup recovery, host run, worker error, stored envelope, receipt reader) |
| Program modules | `machine/mod.rs`: declare the new host module; `reading.rs`: name the reader's syntax and framing failures |
| Program tests | `machine/receive/bytes.rs`: pin every new parking reason spelling |
| Fuzzing and CI | `fuzz/Cargo.toml` and the CI fuzz matrix: register the target and regenerate its lockfile if needed |

Paths in the config row are relative to `crates/core/config/`; steps and
program paths are relative to `crates/steps/` and `crates/analyte/src/`.
Some named modules are folders; follow their current front file rather than
creating a same-named sibling. Review capability facts explicitly: do not
infer support from a transport kind or another vendor's settings.

## Evidence before handoff

Keep raw bytes and the config and driver versions. Test storage failure before
ACK, malformed input, bounded waits, split and merged reads, disconnect and
restart, unknown tests, corrected results and repeat results. Orders also need
lookup-outage and retry tests that prove no false no-orders reply or resend.
Every protocol requirement needs a cited source and a named test; raise the
evidence level only with the required recordings or simulator evidence.

Run the complete AGENTS.md verification set, the protocol-checklist check and
the CI Layout and Duplication checks. Inspect generated
file diffs. A machine definition is a separate step in
[adding a machine](adding-a-machine.md); protocol support alone does not qualify
a machine for clinical use.
