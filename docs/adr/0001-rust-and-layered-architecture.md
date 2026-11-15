# 0001. Rust and a layered architecture

Status: accepted, 2026-11-15

## Context

Analyte runs unattended on a lab PC, handles bytes from machines that may be
faulty, and moves results that end up in patient reports. It must run on
Windows and Linux. Analyzer protocols differ at every level: the cable or
network link, how messages are framed, who speaks when, the message syntax
and the meaning of each field. The OpenELIS Analyzer Bridge and general
integration engines tie these levels together, so one unusual machine forces
changes across the code.

## Decision

- Write Analyte in stable Rust: one Cargo workspace, one program for Windows
  and Linux, with library crates that can be used on their own.
- Split the work into layers, each replaceable on its own:
  transport → framing → dialog → syntax → mapping → one common model.
- A dialog is a state machine owned by one connection, with its own
  deadlines. It is not a step in a message pipeline.
- Crate dependencies form an acyclic graph of ownership, not the runtime data
  flow: shared model types can be used by protocol and storage crates, and the
  program wires dialogs to host services. A dialog may acknowledge data only
  after the host reports that it is durably stored. A separate crate or public
  API is split out only when the boundary has proven real.

## Consequences

- Memory safety and strong types help keep malformed input from corrupting
  state, and invalid states can be made impossible to write.
- A new protocol usually adds one piece in one layer instead of touching
  everything.
- Config validation must reject combinations of layers that make no sense.
- Contributors need Rust; the owner does not read it yet, so behavior is
  proven with tests and recordings and explained in plain words.
