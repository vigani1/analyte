# 0032. Deliveries name the machine model; the contract is published as a schema

Status: accepted, 2026-11-22 (the owner asked for a model field on every
delivery, a machine-readable contract and a machine catalog, and approved the
recommended route and the two dependencies without waiting). Changes the
`source` object of [ADR 0008](0008-app-contract.md) and follows
[ADR 0029](0029-lab-app-owns-test-names.md) and
[ADR 0030](0030-lab-app-owns-specimen-codes.md).

## Context

Since ADR 0029 and ADR 0030 a lab app keeps one table per machine model, from
the machine's own test and specimen codes to its own tests. But a delivery's
`source` named only the installed machine (`analyzer`) and the connection, so
the app could not tell which model's table to use without its own list of
installed machines. Developers of lab apps also had only prose (v1.md) and
example vectors: nothing their tools could read to generate types or check a
document, and nothing listing the models Analyte supports.

## Decision

1. **`source.model` is required on result deliveries and order events.** It
   is the machine definition's folder key, `vendor/model`, for example
   `roche/cobas-e411`: two parts of 1 to 64 lower-case letters, digits or
   hyphens. One string, rather than a vendor and model object, because it is
   the single key a lab app files its table under, the same key the setup page
   and the catalog use, and it cannot be half matched.
2. **Taken from where the definition sits, never guessed.** The engine reads
   it from the two folders a definition file is in,
   `<vendor>/<model>/machine.toml`, which is where setup always writes it. A
   definition anywhere else is refused at start with a specific error. Machine
   definition files are unchanged.
3. **Order events need their machine's model.** An interrupted order of a
   machine no longer in the program file keeps its state at restart, and
   `orders reconcile` refuses it with a specific error, until the machine is
   configured again; its event could not name a model.
4. **The contract is published as JSON Schema and OpenAPI, generated from the
   Rust contract types.** `docs/contract/contract-v1.schema.json` (draft
   2020-12) holds every request and answer body; `docs/contract/openapi.json`
   (OpenAPI 3.1) describes the three calls with bearer authentication and the
   Standard Webhooks headers and refers to that schema. Enums are closed and
   spelled as the Rust types write them. Three rules need byte counting or two
   values compared, which JSON Schema cannot express; they stay in the Rust
   validators and v1.md, and the tests name them.
5. **A machine catalog lists the shipped models.**
   `docs/contract/machines.json`, with `machines.schema.json`, has one entry
   per model folder: model id, README title, vendor, protocol, evidence level,
   whether `machine.toml` exists (`ready: false` for a blocked definition) and
   its test and specimen code systems. Facts a blocked definition lacks are
   null, never filled in.
6. **One generator, checked in CI, attached to releases.** The testing crate
   `analyte-publish` writes the four files; its tests and a CI job fail when
   regenerating would change them. The release attaches them, covered by
   SHA256SUMS and build provenance.
7. **Contract v1 changes in place.** Version 1 is unpublished (0.1.0 is a
   draft), so this is not a version bump.
8. **Two dependencies.** `schemars` 1.2 (MIT) derives the schema from the
   types; `jsonschema` 0.60 (MIT, tests only, without network or file
   loading) checks every published vector against it.

## Why

- A lab app can pick the right table from the delivery alone, and a new
  installed machine of a known model needs no change in the app.
- One source of truth: the schema comes from the same types that validate
  documents, so the prose, the schema and the code cannot drift apart
  unnoticed. Apps in any language can generate their types from it.

## Consequences

- Lab apps key their per-model tables on `source.model`; the catalog lists
  the values. A definition folder rename is a model id change for lab apps.
- Tests and fixtures place definitions in `<vendor>/<model>/` folders.
- A contract change regenerates the schema in the same change; CI refuses a
  stale file.
