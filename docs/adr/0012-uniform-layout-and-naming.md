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
| Stored order outcome codes and their exact spellings | `crates/steps/dialog/src/<key>/outcome.rs`; codes the host writes for any protocol, and the test pinning every spelling, in `crates/steps/dialog/src/outcome/` |
| Grammar: bytes to structure | `crates/protocols/<key>/src/syntax.rs` (a profile: `crates/protocols/hl7/src/<key>/`) |
| Writing: typed values to outgoing bytes or messages | `crates/protocols/<key>/src/encode.rs`, or `encode/` when it has parts (a profile: `crates/protocols/hl7/src/<key>/encode/`) |
| Typed named-field access | `crates/protocols/<key>/src/views.rs` |
| Version rules | `crates/protocols/<key>/src/version.rs` |
| Machine terms to the common model and back | `crates/steps/mapping/src/<key>.rs` |
| Connection driving (I/O, store calls, timers) | `crates/analyte/src/machine/<key>.rs` |
| Order exchange: queries from the machine and work sent ahead | `crates/analyte/src/machine/<key>/orders.rs`, or `orders/` when it has parts; its failures in `machine/<key>/error.rs` |
| Parking reasons the host stores for uninterpreted bytes | Constants or text functions in `crates/analyte/src/machine/<key>/mod.rs`; every spelling pinned in `crates/analyte/src/machine/receive/bytes.rs` |
| Registration: stream session, link, chunk-journal rule for startup recovery, host run, worker error, stored envelope and receipt reader | One arm per match in `crates/analyte/src/machine/protocols.rs` |

(Changed on 2026-11-18 to slim the program crate: stored outcome codes are pure
protocol text, so they moved from `crates/analyte/src/orders/outcome/` to the
dialog that produces them; the program stores the text it is given. Changed
again the same day: each protocol's order exchange moved from
`crates/analyte/src/orders/<key>` to its host, and its failures joined the
host's `error.rs`, wrapping the shared `orders::OrderError`. HL7 and LAW order
code raises shared causes directly, so it keeps a separate `OrderError` there;
otherwise the host error's custody conversions would turn those causes into
custody refusals. `crates/analyte/src/orders/` keeps only the lookup,
admission, reservation, sending and operator commands every protocol shares.)

(Changed again on 2026-11-18: a new protocol edited about ten shared program
files, each with its own match over protocols. `machine/protocols.rs` now holds
every such match: the stream session enum and its link, the host each session
runs, the worker error, the stored envelope and receipt reader dispatch, and
whether startup recovery leaves a stack's receipts alone because the host
journals every chunk without interpreting it. Watched-folder stacks have no
stream session and select `Link::Folder` there. Parking reasons moved from one
shared enum to the host that stores them, with unchanged spellings, so
`machine/receive/bytes.rs` keeps only the interrupted-input reasons and the
tests pinning every spelling.)

A file grows into a folder of the same name (`law/mod.rs` plus its pieces)
when it has separate responsibilities. A concern that a protocol lacks has no file.

Every builder of an outgoing message lives in `encode`: orders, work lists,
envelopes, acknowledgments and query replies. Reading an incoming message,
including a query from the machine, stays with `syntax` and `views`; ASTM
`query.rs` reads incoming queries, so it is not encoding. HL7 `encode/wire.rs`
writes size-capped segments without escaping for every HL7 and LAW writer.
(Changed on 2026-11-17 after the eighth review, finding Y3: writers had four
shapes across five crates, including ASTM `encode.rs` and `order.rs`, HL7
`order/encode/`, `wire.rs` and LAW `work/encode.rs`, and POCT `messages.rs`.)

ASTM also has `codes.rs` for standard code tables and `query.rs` for incoming
query records. HL7 also has `datatypes.rs` for shared field types, `order/` for
order grammar and `query/` for query grammar.

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
`#[expect(..., reason = "...")]`, for a flat table that reads best whole.
Several related yes/no flags become one enum of named states.

Files have no length limit. A file is split when it holds separate
responsibilities, and a piece too small to stand alone stays in its owner's
file. (Changed on 2026-11-18 at the owner's request: an 800-line file cap with
28 `Kept whole:` exemptions split modules for the count's sake and left many
files under 40 lines; the cap, its exemptions and those tiny files were
removed or merged back.)

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

The Layout CI job checks that every Rust file opens with a `//!` line.

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
| Program | `crates/analyte` | analyte | Role folders (`machine`, `orders`, `runtime`, `operator`, `installation`, `admin`, `intake`) and `reading.rs`; per-protocol code sits in `machine` and in one reader function per protocol in `reading.rs`, shared order steps in `orders` |
| Steps | `crates/steps/` | transport, framing, dialog, mapping, sink | One file or folder per protocol key, plus shared pieces named for what they hold |
| Protocols | `crates/protocols/` | astm, hl7, poct, vidas, autolyser | One file or folder per job: `syntax` (reading), `encode` (writing), `views`, `version`, plus the extras named in section 2; a profile (LAW) is a folder with the same jobs |
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

Every protocol uses a `Host` in `machine/<key>/mod.rs`. It owns its
conversation state and borrows the connection's immutable evidence, concrete
protocol settings and store handle. The caller owns those resources through
shutdown; no host copies settings or opens its own transport. Protocol-specific
borrows, such as a pending ASTM lookup or a LAW order exchange, stay with that
host and cannot outlive the session.

VIDAS, Autolyser and POCT implement the driver's `Protocol` trait in the same
front module, and the driver's `serve` runs them. Their `Session` holds the
borrowed evidence, settings, store, connection and stop signal, the connection
clock, the trigger of the latest input and the current step's action queue.
`serve` owns the receive loop and its priority (stop, then deadline, then
input), journals each chunk before the dialog sees it, feeds it one byte at a
time with each byte's actions run before the next byte, turns a stop or close
into one `Disconnected` input, and always attempts `close` and the final
flush. A host only queues actions from `idle` and `input` and runs one per
`effect` call.

ASTM, HL7 and LAW still use the older `Effects` and `Drain` traits until they
move to `Protocol`: their driver supplies transport and cancellation through
`Context`, and each host runs its actions through the driver's one `drain`
loop, which calls `Drain::effect` once per action. In both shapes, actions
produced by storage completion join the same ordered queue before the next
input byte is fed, and no host owns a separate receive loop. Protocol-specific
deadlines and refusal rules remain explicit; LAW keeps its own loop because
each of its effects has its own deadline.

Every `drain` and `effect` returns `Result<Flow, Error>` with the shared
driver's `Flow`. `Stop` from an effect ends the queue at once. An effect that
ends the session only after the queued actions have run calls
`Session::stop_when_drained`. A failure that must wait until the dialog's own
queued replies have run (a store failure before its NAK, a failed reply write)
is kept in the queue's one `Failures` value and reported when the queue ends;
the driver documents the rule beside `Flow`.
(Added on 2026-11-17 after the eighth review, finding Y5: the hosts returned
`bool`, `()` or a state field and kept failures in fields or locals. Changed on
2026-11-18 after the ninth review, finding Z10: every host copied the queue
loop, so `drain` now owns it. Changed again on 2026-11-18: the three
chunk-journaled hosts each repeated the receive, byte and store skeleton, so
`serve` now owns it.)
