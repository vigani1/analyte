# 0006. Apache License 2.0

Status: accepted, 2026-11-15

## Context

Analyte should be usable by any lab and any lab app, including commercial
ones, and should attract contributions. The license also limits which
dependencies can be used.

## Decision

- Analyte is licensed under the Apache License 2.0 and developed in a public
  repository.
- Contributions are accepted under the same license, with a Developer
  Certificate of Origin sign-off on every commit (see
  [CONTRIBUTING.md](../../CONTRIBUTING.md)).
- Dependencies must have licenses compatible with Apache-2.0; `cargo deny`
  checks this in CI.

## Consequences

- Anyone can use, change and ship Analyte, including in closed products, and
  receives an explicit patent grant.
- Copyleft dependencies (for example GPL) cannot be used.
- Standards and vendor manuals are never committed; notes on them are written
  in our own words.
