# The parts of Analyte

A **crate** is one self-contained part of the Rust project. Its `Cargo.toml`
is its ID card, `src/` holds its code, and `tests/`, when present, holds tests
that exercise the part from outside. Some tests also live beside the code.

[analyte](analyte/) is the program you run on the lab PC: it connects these
building blocks, owns connections and storage calls, and runs operator tools.
The other crates provide building blocks; `receiver` also has a separate test
program. They are not separate services needed to run the bridge.

The table follows an incoming message: transport -> framing -> dialog -> protocol
grammar -> mapping -> model, then the supporting parts. Grammar means the rules
for reading and writing message fields. The five grammar crates are alternatives
chosen for a machine. Dialog tracks a conversation throughout, rather than
simply passing each message along once. Outgoing orders use these parts too.

| Part | Job |
| --- | --- |
| [transport](transport/) | Moves bytes through network connections, serial cables or files. |
| [framing](framing/) | Finds message or packet boundaries and checks checksums where required. |
| [dialog](dialog/) | Tracks each conversation: who speaks next, which reply is allowed and when time runs out. |
| [astm](astm/) | Reads and writes structured ASTM records within size limits. |
| [hl7](hl7/) | Reads and writes HL7 messages, including the IHE LAW laboratory profile. |
| [poct](poct/) | Reads and builds POCT Basic Profile messages while retaining the source values. |
| [vidas](vidas/) | Reads and writes tagged messages for the VIDAS PC Literal format. |
| [autolyser](autolyser/) | Reads and writes Autolyser fixed-width message records. |
| [mapping](mapping/) | Translates machine fields and codes into the common model using explicit settings, and maps outgoing work. |
| [model](model/) | Defines shared results, orders, values and identities across protocols. |
| [store](store/) | Keeps original evidence, result history, pending deliveries and order state durably. |
| [contract](contract/) | Defines the versioned documents and signatures exchanged with lab apps. |
| [sink](sink/) | Delivers results to lab apps over HTTP and polls apps for order changes. |
| [config](config/) | Loads size-limited configuration and checks settings before use. |
| [analyte](analyte/) | Runs the bridge and coordinates all these parts. |
| [receiver](receiver/) | Provides a reference lab-app endpoint for integration tests and examples. |
| [sim](sim/) | Plays the machine side of synthetic recordings and compares the bridge replies byte for byte. |

Protocol keys stay the same across layers: per-protocol files are `astm.rs`,
`hl7.rs`, `poct.rs`, `vidas.rs` and `autolyser.rs` wherever that concern exists.
`law.rs` holds the LAW profile; its grammar lives inside `hl7`. HL7 byte
boundaries use `framing/src/mllp.rs`, named for that envelope. Inside each
grammar crate, `syntax.rs` holds the grammar. Larger parts can have a matching
folder. See [the layout decision](../docs/adr/0012-uniform-layout-and-naming.md).
