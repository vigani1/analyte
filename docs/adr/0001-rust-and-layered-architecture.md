# 0001. Rust and a layered architecture

Status: accepted, 2026-11-15

## Context

Analyte runs unattended on a Windows or Linux lab PC, reads bytes from
machines that may be faulty, and moves results that end up in patient reports.
Analyzer protocols differ at every level, from the cable to the meaning of
each field. Tools that tie these levels together must change everywhere for
one unusual machine.

## Decision

- Stable Rust, one Cargo workspace, one program for Windows and Linux, and
  library crates usable on their own.
- Layers, each replaceable on its own: transport → framing → dialog → syntax →
  mapping → one common model.
- A dialog is a state machine owned by one connection, with its own deadlines,
  not a step in a message pipeline.
- Crate dependencies follow ownership, not data flow, and have no cycles. The
  program wires dialogs to host services, and a dialog acknowledges data only
  after the host reports it durably stored. A crate or public API is split out
  only when the boundary has proven real.

## Why

Memory safety and strong types keep malformed input from corrupting state and
make invalid states impossible to write. Separate layers mean a new protocol
usually adds one piece in one layer instead of touching everything.

## Consequences

- Config validation must reject layer combinations that make no sense.
- Contributors need Rust. The owner does not read it, so behavior is proven
  with tests and recordings and explained in plain words.
