# 0012. One layout and one naming scheme for every protocol and machine

Status: accepted, 2026-11-15. Refines [ADR 0001](0001-rust-and-layered-architecture.md)
and [ADR 0002](0002-config-first.md).

## Context

ASTM and HL7 split their code across the layer crates, while POCT, VIDAS and
Autolyser keep framing, dialog or mapping inside their own crate. Protocol
notes have different shapes, vendor protocols have none, and the repository
has no place for machine definitions. Output functions are called `build`,
`encode`, `serialize` or `write` for the same meaning. A measurement on
2026-11-15 found 57 production functions over 100 lines and six over the
cognitive complexity limit, because no size lint was enabled.

A reader, a reviewer or a community contributor should find any part of any
protocol or machine in the same place, under the same name, explained the same
way. HL7 alone now has five editions (2.3.1, 2.5.1, 2.6, 2.8.2, 2.9.1), so
versions need one pattern before they multiply.

## Decision

### 1. Protocol keys

Every protocol has one lower-case key, used unchanged in file names, module
names, config values and docs: `astm`, `hl7`, `law`, `poct`, `vidas`,
`autolyser`. A profile of another standard (LAW on HL7) keeps its own key and
lives under its base standard's syntax crate.

### 2. Code template

Every protocol, standard or vendor, uses the same places:

| Concern | Place |
| --- | --- |
| Message boundaries and checksums | `crates/steps/framing/src/<key>.rs`, when the protocol has its own framing |
| Conversation state, deadlines, acknowledgment choice | `crates/steps/dialog/src/<key>.rs` |
| Grammar: bytes to structure and back | `crates/protocols/<key>/src/syntax.rs` (a profile: `crates/protocols/hl7/src/<key>/`) |
| Typed named-field access | `crates/protocols/<key>/src/views.rs` |
| Version rules | `crates/protocols/<key>/src/version.rs` |
| Machine terms to the common model and back | `crates/steps/mapping/src/<key>.rs` |
| Connection driving (I/O, store calls, timers) | `crates/analyte/src/machine/<key>.rs` |
| Order sending | `crates/analyte/src/orders/<key>.rs` |

A file grows into a folder of the same name (`law/mod.rs` plus its pieces)
when it has separate responsibilities. A concern that a protocol lacks has no file.

HL7 also has `ack.rs` for acknowledgment grammar, `datatypes.rs` for shared
field types, `order/` for order grammar and `query/` for query grammar.

HL7 `wire.rs` writes size-capped segments without escaping; POCT `messages.rs`
builds reviewer messages and escapes XML. These distinct jobs keep distinct names.

### 3. Versions

A standard with several versions has one implementation. `version.rs` holds
the `Version` enum and the table of rules that differ by version. There are no
per-version copies of parsers, dialogs or mappings. The machine definition
names its version, and an unsupported version fails config validation. Every
row in the table names its source section and has a test.

### 4. Protocol notes

Every protocol, vendor formats included, has `docs/protocols/<key>/` with the
same four files:

| File | Holds |
| --- | --- |
| `README.md` | What the protocol is, Analyte's role in it, source document names and editions, scope, evidence level |
| `decisions.md` | Each choice the standard leaves open or Analyte adds on top of it (role, strictness, supported options, readings of ambiguous text, defaults, safety-driven departures): what Analyte does, why, the source section or ADR behind it, and the test that guards it. Plain requirements live only in `checklist.md` |
| `checklist.md` | The conformance checklist, one or more tests per row |
| `versions.md` | Supported versions, the difference table with sources, and the compatibility rule for unlisted versions |

Notes are written in our own words; nothing is copied from the documents.

### 5. Machines

Each machine definition lives in `machines/<vendor>/<model>/`, lower-case and
hyphenated (`machines/roche/cobas-e411/`):

| Entry | Holds |
| --- | --- |
| `machine.toml` | The shipped definition: protocol key, version, transport kind, mappings, named departures, evidence level |
| `README.md` | Fixed sections: Machine, Connection, Settings on the machine, Supported workflows, Departures, Evidence, Open questions, Sources |
| `recordings/` | ADR 0009 recordings, synthetic or de-identified only |

Site settings and credentials never enter the repository. A workspace test
loads every `machines/**/machine.toml`, validates it, replays every recording
and checks that the README states the same evidence level as the definition.
The protocol and the transport kind are independent settings, so any protocol
can run over any transport that its framing supports.

### 6. Names

- One verb per meaning: `parse` (bytes or text to typed values), `encode`
  (typed values to bytes), `map` (machine terms to model terms, or back),
  `validate` (a rule check that returns a specific error). A term defined by a
  standard keeps the standard's name (for example HL7 escape decoding).
- A name says what the item holds. No `utils`, `common`, `helpers`, `misc`, or
  names that point to a neighbour instead of the content.
- A module without child modules is one file, `name.rs`. A module with child
  modules is one folder, `name/`, whose front file is `name/mod.rs`; there is
  never a `name.rs` beside a `name/` folder, so each topic has one place.
  (Changed on 2026-11-16 at the owner's request: one name per topic reads
  more clearly when browsing the repository.) Unit tests that leave their
  module go to `<module>/tests.rs`.
- File names never repeat their folder, and item names never repeat their
  module.

### 7. Size and complexity

The workspace enables `too_many_lines` (100), `cognitive_complexity` (25),
`fn_params_excessive_bools`, `struct_excessive_bools`, `items_after_statements`,
`module_name_repetitions` and `similar_names`. An exception is a narrow
`#[expect(..., reason = "...")]`, for a flat table that reads best whole. A
tracked Rust file under `crates/` or `fuzz/` is limited to 800 lines; files of 601 to 800 lines require a `//!` header line containing `Kept whole:` and its reason, enforced by the Layout CI job.
Several related yes/no flags become one enum of named states.

### 8. Every crate and every file says what it holds

Every Rust file starts with a `//!` line that explains its contents to a
newcomer. Crate READMEs use the same concrete descriptions, stating the kind
first. Apply these rules:

- Write one complete sentence that can be read alone. Good: `//! Detects repeated POCT messages within a bounded window.` Bad: `//! Bounded duplicate detection. The host persists identities in the`.
- Start with a verb and the main things the file handles, naming concrete segments, types or records. Good: `//! Validates LAW MSH, MSA and ERR segments.` Bad: `//! Manager control profiles.`
- Put safety and I/O limits after the job. Good: `//! Maps ASTM result records to model results without I/O.` Bad: `//! No I/O or clinical decisions.`
- Describe contents rather than where they belong. Good: `//! Reads ASTM header record fields.` Bad: `//! Header views belong beside the grammar.`
- Prefer concrete terms to abstractions such as semantics, profiles, assembly or boundary. Good: `//! Writes HL7 segments within a byte limit.` Bad: `//! Private bounded wire assembly.`
- Avoid words whose lab or standards meaning differs from the code. Good: `//! Holds evidence lists capped at 256 entries.` Bad: `//! Holds evidence collections.` In a lab, collection means collecting a specimen.
- Keep any `//! Kept whole:` reason on its own line, as the second header line. Good: `//! Runs service startup and shutdown.` followed by `//! Kept whole: Preserves worker ownership order.` Bad: `//! Service ownership. Kept whole: Preserves worker ownership order.`

The Layout CI job checks the opening header and finds the separate
`Kept whole:` line in the opening documentation block.

### 9. Large folders group files by role

A source folder holding more than about ten files groups them into role
folders named for what they hold (for example `intake/`, `delivery/`,
`upkeep/` in the store), so a reader can tell from the folder why a file is
there. Per-protocol files (`astm.rs`, `hl7.rs`, ...) stay side by side, since
the protocol key already explains them. Small folders stay flat. The Layout check caps direct Rust files at ten and
rejects file stems equal to, or prefixed by, their containing folder name.
Justified flat folders are listed with reasons in `scripts/layout-allowlist.txt`;
the check rejects stale exceptions. Domain terms (receipt, delivery, revision, custody, parked item and so
on) are defined once in `docs/glossary.md`.

### 10. Crates are grouped by kind, and each kind has one inner layout

`crates/` holds one folder per kind; every crate of a kind lays out its `src/`
the same way, and its `README.md` states the kind first.

| Kind | Folder | Crates | Inside `src/` |
| --- | --- | --- | --- |
| Program | `crates/analyte` | analyte | Role folders (`machine`, `orders`, `runtime`, `operator`, `installation`, `admin`, `intake`, `reading`), one file per protocol inside |
| Steps | `crates/steps/` | transport, framing, dialog, mapping, sink | One file or folder per protocol key, plus shared pieces named for what they hold |
| Protocols | `crates/protocols/` | astm, hl7, poct, vidas, autolyser | One file per job: `syntax`, `views`, `version`, `wire` (HL7 segment writer), `messages` (POCT message builders); a profile (LAW) is a folder |
| Core | `crates/core/` | model, store, config, contract | Role folders named for what they hold |
| Testing | `crates/testing/` | receiver, sim, fixtures | Stand-ins and shared synthetic fixtures used only by tests |

Transport is arranged by connection kind because it has no per-protocol variants.
Sink is arranged by role because it has no per-protocol variants.

A new crate joins one kind, or the plan gets a new kind through this ADR.

## Consequences

- Adding a protocol means filling the same template; adding a machine means
  one `machines/` folder and no code.
- POCT, VIDAS and Autolyser move their framing, dialog and mapping into the
  layer crates. The plan's crate table changes with them.
- `crates/core/config/examples/` keeps synthetic teaching configs. Real machines
  live only in `machines/`.
- Existing protocol notes move into the four-file shape without losing
  content.
- New code that breaks the size limits fails CI, so the cleanup holds.

### Shared layer contracts

All six stacks decode one byte at a time and consume events immediately, so
one call never accumulates an unbounded list. Concrete decoders expose one byte
entry point, `push(byte)`, and one end-of-stream operation, `finish()`.
ASTM/MLLP return `Option<Event>`; fallible decoders return
`Result<Option<Event>, Error>`. Autolyser `finish()` always returns the partial
frame so its bytes can be stored.
Rejected evidence can be an event or a refusal; hosts journal bytes before
feeding either. LAW uses HL7 MLLP directly. A forwarding trait adds no generic
production boundary, so these concrete operations are the integration contract.
Do not mix POCT's host-buffer scanning and stream-owned buffering on one decoder
instance. Timers stay in the dialog.

Every dialog has one event entry: `step(now, event) -> Result<Vec<Action>, Error>`.
Invalid monotonic time is rejected before mutation; protocol refusals and host
effects remain ordered actions. VIDAS's event transition method is private.

### Program connection hosts

Every protocol uses `Host<'a, 'm>` in `machine/<key>/mod.rs`. It owns its
conversation state and borrows the connection's immutable evidence, concrete
protocol settings and store handle. The caller owns those resources through
shutdown; no host copies settings or opens its own transport. Protocol-specific
borrows, such as a pending ASTM lookup or a LAW order exchange, stay with that
host and cannot outlive the session.

The `Effects` implementation stays in the same front module. The shared driver
owns receive priority, input journaling and the final cleanup attempt; its
`Context` supplies transport and cancellation. Each host feeds dialog events,
then drains the resulting actions in one `apply` method. Actions produced by
storage completion join that same ordered queue before the next input byte is
fed. A single-action method may handle a large match, but never owns a separate
receive loop. Protocol-specific deadlines and refusal rules remain explicit.
