# The parts of Analyte

A **crate** is one self-contained part of the Rust project. Its `Cargo.toml`
is its ID card, `src/` holds its code, and `tests/`, when present, holds tests
that exercise the part from outside. Some tests also live beside the code.

[analyte](analyte/) is the program you run on the lab PC: it connects these
building blocks, owns connections and storage calls, and runs operator tools.
The other crates provide building blocks; `receiver` and `contract-check` also
have separate test programs. They are not separate services needed to run the bridge.

Crates are grouped by kind:

```text
crates/
  analyte/
  steps/       transport, framing, dialog, mapping, sink
  protocols/   astm, hl7, poct, vidas, autolyser
  core/        model, store, config, contract
  testing/     receiver, contract-check, sim, fixtures
```

Crates of the same kind share a layout:

| Kind | How its parts are arranged |
| --- | --- |
| **step** | One file per protocol, such as `astm.rs` or `hl7.rs`; transport uses connection kinds instead. |
| **protocol** | One file per job, such as `syntax.rs`, `views.rs` or `version.rs`, where needed. |
| **testing** | Test stand-ins keep their library entry in `lib.rs` and an optional executable in `main.rs`. |
| **core** | Folders group parts by role; small roles can stay in a single file. |
| **program** | Role folders, with per-protocol files inside where needed. |

A module is a named part of the code. A module with children has a folder and
a `mod.rs` front file; the guides below name modules.

The table follows an incoming message: transport -> framing -> dialog -> protocol
grammar -> mapping -> model, then the supporting parts. Grammar means the rules
for reading and writing message fields. The five grammar crates are alternatives
chosen for a machine. Dialog tracks a conversation throughout, rather than
simply passing each message along once. Outgoing orders use these parts too.

| Part | Job |
| --- | --- |
| [transport](steps/transport/) | Moves bytes through network connections, serial cables or files. |
| [framing](steps/framing/) | Finds message or packet boundaries and checks checksums where required. |
| [dialog](steps/dialog/) | Tracks each conversation: who speaks next, which reply is allowed and when time runs out. |
| [astm](protocols/astm/) | Reads and writes structured ASTM records within size limits. |
| [hl7](protocols/hl7/) | Reads and writes HL7 messages, including the IHE LAW laboratory profile. |
| [poct](protocols/poct/) | Reads and builds POCT Basic Profile messages while retaining the source values. |
| [vidas](protocols/vidas/) | Reads and writes tagged messages for the VIDAS PC Literal format. |
| [autolyser](protocols/autolyser/) | Reads and writes Autolyser fixed-width message records. |
| [mapping](steps/mapping/) | Translates machine fields and codes into the common model using explicit settings, and maps outgoing work. |
| [model](core/model/) | Defines shared results, orders, values and identities across protocols. |
| [store](core/store/) | Keeps original evidence, result history, pending deliveries and order state durably. |
| [contract](core/contract/) | Defines the versioned documents and signatures exchanged with lab apps. |
| [sink](steps/sink/) | Delivers results to lab apps over HTTP and polls apps for order changes. |
| [config](core/config/) | Loads size-limited configuration and checks settings before use. |
| [analyte](analyte/) | Runs the bridge and coordinates all these parts. |
| [receiver](testing/receiver/) | Provides a reference lab-app endpoint for integration tests and examples. |
| [contract-check](testing/contract-check/) | Runs the published contract vectors against a lab app's real endpoints and reports each case. |
| [fixtures](testing/fixtures/) | Shares synthetic messages, storage probes and bounded waits across tests. |
| [sim](testing/sim/) | Plays the machine side of synthetic recordings and compares the bridge replies byte for byte. |

Protocol keys stay the same across layers: per-protocol files are `astm.rs`,
`hl7.rs`, `poct.rs`, `vidas.rs` and `autolyser.rs` wherever that concern exists.
`law.rs` holds the LAW profile; its grammar lives inside `hl7`. HL7 byte
boundaries use the `hl7` module in framing, which handles the MLLP envelope. Inside each
grammar crate, the `syntax` module holds the grammar. A module with children
uses a folder with a `mod.rs` front file. See [the layout decision](../docs/adr/0012-uniform-layout-and-naming.md).
