# Adding a vendor protocol

Use this guide only when the confirmed vendor interface cannot be expressed by
an existing protocol and machine configuration. Read the plan and
[ADR 0012](adr/0012-uniform-layout-and-naming.md) first. A new protocol family,
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
  I/O, durable storage and effect deadlines.
- Add `crates/analyte/src/reading/<key>.rs` for reading stored receipts.
- If orders are supported, add `crates/analyte/src/orders/<key>.rs` and
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
| Config | `src/lib.rs`, `migration`, `orders/mod.rs`, `orders/validation`, `schema/mod.rs`, `selection`, `stack/mod.rs`, `stack/capabilities.rs`, `validation/stack`: protocol selection, validation, order support and capability row |
| Steps | `framing/src/lib.rs`, `dialog/src/lib.rs`, `mapping/src/lib.rs`, `mapping/src/inbound/evidence.rs`, `mapping/src/orders/mod.rs`: register owned layers and evidence |
| Program machine | `machine/mod.rs`, `machine/settings.rs`, `machine/error.rs`, `machine/receive/bytes.rs`, `machine/receive/connection_input.rs`: dispatch, errors and input custody |
| Program reading | `reading/mod.rs`, `reading/envelope.rs`: receipt dispatch and specific errors |
| Program orders | `orders/mod.rs`, `orders/error.rs`: admission and sending |
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
the CI layout, hook, line-length and duplication checks. Inspect generated
file diffs. A machine definition is a separate step in
[adding a machine](adding-a-machine.md); protocol support alone does not qualify
a machine for clinical use.
