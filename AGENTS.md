# Project Instructions

Analyte connects clinical lab analyzers to lab apps. Results it moves end up in
patient reports, so a silent mistake can harm a patient. When in doubt, keep
the data, flag it and stop; never guess.

## Source of truth

- Start each session with [docs/context.md](docs/context.md): background,
  decisions so far and how to work with the owner.
- The user's request, [docs/plan.md](docs/plan.md), the ADRs in `docs/adr/`
  that are not superseded, and the current code. Background research lives in
  [docs/research/](docs/research/README.md).
- Protocol rules come from the standards and vendor manuals listed in the plan.
  Our own notes on them live in `docs/protocols/`, written in our own words.
- Update the plan only when the work changes what it claims.

## What Analyte is and is not

- It is a **bridge**: it receives, stores and delivers faithfully, and answers
  machine queries from orders the lab app gave it.
- It is **not** a lab information system. No auto-verification, no reflex
  rules, no QC decisions, no clinical interpretation. Those belong in the lab
  app.
- It never invents data: no default sample IDs, no guessed units, no "no
  orders" reply when the real answer is unknown.

## Design rules

- **Layers:** transport → framing → dialog → syntax → mapping → one common
  model. Each layer is replaceable on its own. Dialogs own connection state
  and deadlines; they are state machines, not pipelines.
- **Config first:** a machine that speaks a supported protocol is a config
  file. Write code only when a config cannot express the behavior, and then as
  a small piece in the right layer, never as a special case inside a shared
  one.
- **Standards first:** implement a standard from its official documents and
  test it against official examples or simulators, independent of any
  machine. Never shape a standard's code around one machine's behavior; that
  belongs in the machine's config.
- **The host owns the risky parts:** port access, buffering, deadlines,
  durable storage, credentials and HTTPS stay in core code, never in a driver.
- **Two-question test** before building anything "for the future": would it be
  much harder to add later, and is it really needed within about a year? Two
  "no" answers mean a clean seam, not the feature. No plugin system until a
  real outside driver needs one.
- **One way per thing:** one place per rule, one stored format, one validation
  of each input at the boundary that owns it.
- **Illegal states can't be written:** use enums and types so invalid
  combinations do not compile or fail config validation.
- **Keep errors specific.** Never swallow an error or blur it into a generic
  failure.
- **A wrapper earns its place:** it hides a detail, adds a rule or is a
  deliberate seam. A wrapper that only forwards or renames goes.
- **One change, one place:** if supporting a new machine or rule touches many
  unrelated files, move the boundary instead.
- **Folders:** group files by a clear owner (layer, then role). Split a module
  when it has a separate responsibility, not to meet a file count. File names
  never repeat their folder (`astm/frame.rs`, not `astm/astm_frame.rs`).
- **Stop rule:** if work seems to need a new subsystem, format, protocol
  family, dependency or ADR that the plan does not contain, stop and ask.

## Clinical safety rules

These are not optional and need an ADR to change.

1. **Store before acknowledging.** Never tell a machine "got it" for data that
   is not durably stored. If storage fails or the disk is nearly full, refuse
   visibly.
2. **Never drop data.** Failed parses, unknown tests and undeliverable results
   are parked with a reason and shown to an operator, never deleted.
3. **Keep the original.** Store raw bytes with the driver and config version
   that read them. Reprocessing creates a new version; it never overwrites.
4. **Three identities:** the message from the machine, the delivery to the app
   (same ID on every retry), and the result itself with its revision. A
   corrected result is a new revision, not a duplicate; a rerun is a new
   result.
5. **Values stay as written.** Keep `<0.1`, `1,234`, "positive" and error
   tokens exactly as sent next to any parsed number. Sample IDs are strings:
   keep leading zeros and case.
6. **Units and dilution:** keep the original unit; convert only through an
   explicit mapping. Record whether a value is already multiplied by its
   dilution.
7. **Bound everything.** Every buffer, frame, message and wait has a size and
   time limit.
8. **Order lifecycle:** sent, accepted by the machine, done and cancelled are
   separate states. Never resend an order to a machine as a side effect of a
   retry or a replay.
9. **Lookup failure is not "no orders".** If the lab app cannot be reached,
   say so in the machine's protocol or not at all; never answer "no tests".
10. **No patient data in ordinary logs.** Raw messages live only in the
    protected store.

## Standards and sources

- Every protocol rule in code is traceable: its test names the source, for
  example `HL7 2.5.1 §2.7 escape sequences` or `cobas e 411 HIM v2.3 §D`.
- Write notes and docs in our own words. Never paste text, tables or examples
  from paid or copyrighted documents into the repository.
- Never commit standards PDFs or vendor manuals. Keep them in `references/`,
  which git ignores.
- LOINC and UCUM content keeps their copyright notices and is never modified.

## Evidence levels

Every protocol driver and machine config declares one level, and docs show it:

- **proven:** tested against the real machine with recorded conversations.
- **simulated:** tested against an official simulator (for example IHE
  Gazelle) or vendor sample traces.
- **documented:** built from documents only.

Never raise a level without the evidence that justifies it.

## Rust conventions

### Toolchain and workspace

- Stable Rust, edition 2024, pinned in `rust-toolchain.toml`. State the
  minimum supported version (`rust-version`) in the workspace and test it.
- One Cargo workspace. Shared versions live in `[workspace.dependencies]`,
  shared lints in `[workspace.lints]`. Commit `Cargo.lock`.
- Crates follow the layers; a crate depends only on layers below it. No
  dependency cycles, no "utils" crate.

### Types

- Make wrong states unrepresentable: enums for states and variants, newtypes
  for IDs and units (`SampleId`, `DeliveryId`, never a bare `String`), and
  constructors that validate once.
- Parse, don't validate: turn raw input into typed values at the boundary and
  pass the types inward.
- Keep fields private when callers could break an invariant; constructors and
  methods preserve the rule, including after deserialization.
- Match every variant of our protocol and lifecycle enums explicitly; use a
  wildcard only for an external non-exhaustive type, and explain its behavior.
- Use `#[non_exhaustive]` only for types meant to be extended by users outside
  the workspace; keep internal state enums exhaustive.
- Prefer plain functions and concrete types. Add generics or traits only for
  a real second implementation or a test seam.

### Errors and panics

- Use typed errors (`thiserror`) where callers must tell recovery actions
  apart; group them by a meaningful API boundary and keep the cause. Only the
  program's top-level orchestration may use one catch-all error type.
- External input and routine I/O failures return errors and never panic. A
  proven internal invariant may use `expect` with its reason. Never catch a
  panic and carry on a protocol session as if it succeeded.
- No integer overflow surprises: use checked or explicit arithmetic for
  lengths, offsets and checksums. Never index slices with untrusted values;
  use `get`.

### Safety

- `#![forbid(unsafe_code)]` in every crate. An exception needs an ADR, stays
  behind a small safe API, explains why safe Rust cannot do the job, documents
  every safety precondition and why it holds, and is tested with Miri where
  supported.
- Treat every byte from a machine, file or network as hostile: bounded
  reads, no recursion on input, no allocation sized by input without a cap.

### Concurrency and cancellation

- One async runtime (Tokio) for network, timers and coordination. Blocking
  serial I/O and storage run on dedicated threads and talk through bounded
  channels.
- Every external operation has a deadline. Long-lived loops and idle waits
  are cancellable, and shutdown finishes within a bounded time. Every task has
  an owner; no detached tasks, no global mutable state.
- For each stateful async operation, document what cancellation can leave
  behind. A timeout never proves that nothing happened, and retries keep the
  same identity.
- Never hold a blocking mutex guard across `.await`; any async lock must name
  the invariant it protects.

### Dependencies and supply chain

- **Ask before adding a dependency.** Check maintenance, license, last release
  and how much it pulls in. Prefer the standard library and a few
  well-maintained crates. A dependency already approved in the plan needs no
  second approval.
- Update only the dependencies a task needs; review their lockfile changes,
  enabled features, build scripts and sources, and keep unrelated upgrades
  out.
- `cargo deny` checks licenses, sources and security advisories in CI; record
  every exception with its reason. Allowed licenses must be compatible with
  Apache-2.0.
- Features are additive; default features are kept minimal.

### Style and docs

- `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings`
  must be clean. Enable selected `clippy::pedantic` lints in the workspace.
  Prefer fixing a lint; when an exception is justified, use the narrowest
  `#[expect(..., reason = "...")]`, never a blanket suppression.
- Every public item has a doc comment saying what it guarantees; examples in
  docs compile as tests.
- Logging uses `tracing` with structured fields (connection, session,
  delivery IDs), never payloads.
- Comments explain why, not what.

## Tests

- Test behavior from the plan and the standards, not from the code. Assert
  outcomes: bytes sent, records stored, deliveries made.
- Recorded conversations (bytes, direction, timing) are the main fixtures.
  Only synthetic or de-identified data enters the repository.
- Cover the failure paths: broken checksum, lost acknowledgment, split and
  merged reads, timeouts, disconnects, restart after store and before
  acknowledgment, disk full, corrected and repeated results.
- Framers and parsers get property tests; byte parsers get fuzz targets (run
  on Linux in CI).
- Automated tests use temporary stores, local test receivers and controlled
  time. They never contact real analyzers or lab apps, and never use live
  credentials or patient records.
- Turn every parser or dialog bug into a small regression case: keep the
  failing bytes and the source reference, and check changed expectations
  against the protocol before accepting them.
- Regenerate generated files with their owning tool and inspect the diff.
  Malformed fixtures are deliberate test input. Never accept changed
  snapshots just because the tests then pass.
- Tests run on both Windows and Linux in CI, plus the minimum supported Rust
  version.
- A test must fail when the behavior it guards breaks. Never loosen, skip or
  delete a test just to get a pass.

## Verification

Run focused checks while iterating. Before handoff run `cargo fmt --check`,
`cargo clippy --all-targets --all-features -- -D warnings`, `cargo test
--workspace` and `cargo deny check`. Report every check that was not run and
why; never claim a check passed without its output. Back technical claims with
evidence: the source section, the test or the recording.

## Git discipline

- Commit as the repository owner; do not add co-author trailers. Sign off
  every commit (`git commit -s`, see [CONTRIBUTING.md](CONTRIBUTING.md)).
- Imperative Conventional Commit messages (`feat:`, `fix:`, `docs:`,
  `test:`, `refactor:`, `ci:`, `chore:`).
- Coherent, buildable commits. Never amend, squash, rebase or force-push
  unless the user asks. Never skip hooks.
- Never commit secrets, local data, captures with patient data or reference
  documents.
- Push and merge only when the user has authorized it for the current task.

## Agent skills

- This file holds the core rules. Add a skill only for a repeated, exact
  procedure (adding a machine, turning a manual into recordings, releasing),
  and remove it if it is not used or does not help.
- Before choosing a library or approach, check current documentation and
  release status rather than relying on memory.
