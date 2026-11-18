# 0006. Apache License 2.0

Status: accepted, 2026-11-15

## Context

Analyte should be usable by any lab and any lab app, commercial ones included,
and should attract contributions. The license also limits which dependencies
can be used.

## Decision

- Apache License 2.0, in a public repository.
- Contributions under the same license, with a Developer Certificate of Origin
  sign-off on every commit ([CONTRIBUTING.md](../../CONTRIBUTING.md)).
- Dependency licenses are reviewed one by one in the `cargo deny` config, which
  CI checks. Allowed: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, and MPL-2.0 for
  unmodified dependencies. Not allowed: GPL, AGPL and LGPL, because Rust links
  statically.

## Why

A permissive license with an explicit patent grant lets anyone use, change and
ship Analyte, including in closed products.

## Consequences

- No strong copyleft dependencies; third-party notices are kept, and code from
  incompatible projects is never copied.
- Standards and vendor manuals are never committed; notes on them are in our
  own words.
