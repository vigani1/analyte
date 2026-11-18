# 0008. Contract with lab apps (version 1)

Status: accepted, 2026-11-15; amended before release on 2026-11-15 (order
states and optional order fields) and 2026-11-18 (setup errors pause the
destination)

## Context

Analyte delivers results to lab apps and reads orders from them over outbound
HTTPS ([ADR 0004](0004-outbound-only-polled-orders.md)). Any app, in any
language, must be able to implement the contract from its documents alone. A
lost answer, a retry or a restart must never create a duplicate clinical
effect, and the app must be able to tell that a request came from its Analyte.

## Decision

- **Format:** HTTPS with verified certificates, JSON in UTF-8, and
  `"contract_version": 1` on every request and answer. Version 1 only gains
  fields; receivers ignore fields they do not know.
- **Authentication:** a bearer API key per destination, plus a
  [Standard Webhooks](https://www.standardwebhooks.com/) signature on every
  request with a body, with key rotation.
- **Results and order events** (`POST /results`, `POST /order-events`): each
  delivery has one ID, kept with its exact bytes across retries, resends and
  restarts. The app stores the ID with its effects before answering. Each
  answer is done, retry later, fix the setup (the destination pauses) or
  parked with the app's reason.
- **Orders** (`GET /orders`): a paged full snapshot, then changes since an
  opaque cursor. The cache is trusted only after a complete snapshot and while
  the last poll is fresh; a `410` cursor reloads the snapshot.
- Order data may include a patient identifier when a work list needs it; it is
  the only patient data the contract carries, and nothing is ever defaulted.

Every field, status code and order state is in [docs/contract/v1.md](../contract/v1.md);
the test vectors in `docs/contract/vectors/` cover valid and invalid cases.

## Why

A published signing scheme and idempotent delivery IDs give safe retries in any
language without inventing cryptography.

## Consequences

- Apps need storage keyed by delivery ID, a signature check and a paged
  changes feed; the reference receiver shows all three.
- The host stores and rotates two secrets per destination, never a driver.
- A breaking change needs contract version 2, served beside version 1.

## Frozen wire types

The version 1 types in `crates/core/contract` (`value`, `text`, `identity`,
`list`, `time`, `sample`) keep their own validators instead of sharing the
model's, because each defines what version 1 accepts; a model change must not
silently change that. The property tests in
`crates/core/contract/tests/agreement.rs` compare both sides, and a divergence
needs an explicit contract decision. These paths are excluded from the
duplication gate in `scripts/duplication-exclusions.txt`.
