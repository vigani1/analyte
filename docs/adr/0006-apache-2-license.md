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
- Dependency licenses are reviewed one by one and recorded in the
  `cargo deny` configuration, which CI checks. Allowed: permissive licenses
  (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode) and MPL-2.0 for unmodified
  dependencies, whose obligations stay file-level. Not allowed: GPL, AGPL and
  LGPL, because Rust links statically.

## Consequences

- Anyone can use, change and ship Analyte, including in closed products, and
  receives an explicit patent grant.
- Strong copyleft dependencies (GPL, AGPL, LGPL) cannot be used; third-party
  notices are kept, and code from incompatible projects is never copied.
- Standards and vendor manuals are never committed; notes on them are written
  in our own words.
