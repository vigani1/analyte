# 0012. One layout and one naming scheme for every protocol and machine

Status: accepted, 2026-11-15; changed 2026-11-16 (folder front files) and
2026-11-17 to 2026-11-18 (one `encode` place, order code beside its host, one
protocol registry, shared host runner, no file-length cap). Refines
[ADR 0001](0001-rust-and-layered-architecture.md) and
[ADR 0002](0002-config-first.md).

## Context

Protocols were split differently across crates, notes had different shapes,
the same job had four names (`build`, `encode`, `serialize`, `write`), and
there was no place for machine definitions. A reader or contributor should
find any part of any protocol or machine in the same place, under the same
name.

## Decision

1. **Protocol keys:** one lower-case key per protocol, used in files, modules,
   config and docs: `astm`, `hl7`, `law`, `poct`, `vidas`, `autolyser`. A
   profile (LAW on HL7) keeps its key under its base standard's crate.
2. **Code template:** every protocol uses the same places for framing, dialog,
   syntax, encoding, views, versions, mapping, host and orders. The full table
   is in [adding a vendor protocol](../adding-a-vendor-protocol.md#where-each-piece-lives).
3. **Versions:** one implementation per standard; `version.rs` holds the
   `Version` enum and the table of rules that differ, each row with its source
   and a test. No per-version copies; an unsupported version fails config
   validation.
4. **Protocol notes:** `docs/protocols/<key>/` has `README.md`,
   `decisions.md`, `checklist.md` and `versions.md`, in our own words
   ([what each holds](../protocols/README.md)).
5. **Machines:** each machine lives in `machines/<vendor>/<model>/` with
   `machine.toml`, `README.md` and `recordings/`; a workspace test validates
   every definition and replays every recording ([machines/README.md](../../machines/README.md)).
6. **Names:** one verb per meaning: `parse`, `encode`, `map`, `validate`. A
   name says what the item holds: no `utils`, `common`, `helpers` or `misc`. A
   module with children is a folder with `mod.rs`, never `name.rs` beside
   `name/`. File names never repeat their folder.
7. **Size:** Clippy limits function length and complexity; a narrow
   `#[expect(..., reason = "...")]` is the only exception. Files have no length
   limit and split only for separate responsibilities.
8. **File headers:** every Rust file opens with a `//!` sentence saying what it
   holds ([how to write one](../../CONTRIBUTING.md#file-headers)).
9. **Large folders:** a folder of more than about ten files groups them into
   role folders; per-protocol files stay side by side. Exceptions, with
   reasons, are in `scripts/layout-allowlist.txt`.
10. **Crate kinds:** `crates/` holds program, steps, protocols, core and
    testing folders, and every crate of a kind has the same inner layout. A new
    kind needs this ADR.

## Why

The same question always has the same answer: where a piece lives and what it
is called. That makes a new protocol a template to fill, not a design task.

## Consequences

- Adding a machine is one `machines/` folder and no code.
- `crates/core/config/examples/` keeps synthetic teaching configs only.
- `scripts/check-layout.sh` and Clippy enforce the rules in CI.
