# 0008. Contract with lab apps (version 1)

Status: accepted, 2026-11-15; amended 2026-11-15 before release (order
states and optional order fields, matching contract v1); amended 2026-11-18
before release (setup errors pause the destination)

## Context

Analyte delivers results to lab apps and reads orders from them, always over
outbound HTTPS from the lab PC (ADR 0004). Any app must be able to implement
the contract from its documentation alone, in any language. A lost answer,
a retry or a restart must never create a duplicate clinical effect, and an
app must be able to tell that a request really came from its Analyte.

## Decision

**Transport and format.** HTTPS with verified certificates. JSON in UTF-8.
Every request and response carries `"contract_version": 1`. Within version 1,
fields are only added, never changed or removed; receivers ignore fields they
do not know. Each destination is configured with a base URL; the paths below
are relative to it.

**Authentication.**

- Every request carries `Authorization: Bearer <api key>`, one key per
  destination.
- Every request with a body is also signed with the
  [Standard Webhooks](https://www.standardwebhooks.com/) scheme:
  `webhook-id` is the delivery ID, `webhook-timestamp` the send time, and
  `webhook-signature` an HMAC-SHA256 over `id.timestamp.body` with the
  destination's signing secret. Several signatures may be sent during secret
  rotation. Receivers reject timestamps more than five minutes away from
  their clock.

**Results and events: `POST /results` and `POST /order-events`.**

- The body is a delivery document with a `delivery_id` (a UUIDv7 created
  once), the receipt it came from (ID, receive time, SHA-256 of the raw
  bytes), the sample identity and every result with its identity, revision,
  source status, value as written, parsed value, units and flags, as defined
  in the contract document.
- Retries and operator resends reuse the same ID and the exact same bytes;
  only the signature headers are recomputed.
- An attempt interrupted by shutdown or a crash is retried automatically after
  restart with the same ID and bytes, with an audit entry; this is safe because
  receivers answer a repeat as `duplicate`. Parked, conflicting and paused
  deliveries still need an operator. (Orders sent to machines are different:
  they are never resent automatically, AGENTS rule 8.)
- The app stores the delivery ID together with its effects before it
  answers. Answers:
  - `200` with `{"status": "accepted"}` or `{"status": "duplicate"}`: done;
  - `409`: the same ID arrived with different content; Analyte parks it;
  - `400` or `422` with `{"error": ..., "detail": ...}`: parked with the
    app's reason;
  - `401`, `403`, a redirect (`3xx`), `404` or `413`: delivery to that
    destination pauses until the setup is fixed;
  - `408`, `429`, `5xx` or no answer: retried with capped exponential backoff
    and jitter, honouring `Retry-After`;
  - any other status: parked.
- Order status changes (requested, sent, accepted, rejected, done, cancel
  requested, cancel confirmed, uncertain) travel to `POST /order-events`
  through the same outbox. Analyte's internal "sending" state is never sent.

**Orders: `GET /orders?cursor=<cursor>&limit=<n>`.**

- Without a cursor the app returns a full snapshot, paged; with one, the
  changes since it. Each page returns `orders`, `next_cursor` and
  `complete` (true on the last page). The cursor is opaque to Analyte.
- Each order carries its ID, version, state, sample ID, requested tests
  (each with its code system), priority, optional target analyzers and expiry.
  It may also carry a sample position, a specimen type and a patient
  identifier when a machine needs them for a work list; none is ever
  defaulted, and the patient identifier is the only patient data the contract
  carries (no names or demographics). A deleted or withdrawn order arrives as
  a tombstone.
- `410 Gone` means the cursor is no longer valid: Analyte marks its cache
  untrusted and reloads the snapshot.
- Analyte commits each page together with its cursor. The cache is trusted
  only after a complete snapshot and while the last successful poll is
  younger than the configured freshness limit.

**Documentation and test vectors.** The full field list lives in
`docs/contract/v1.md`. `docs/contract/vectors/` holds request and response
examples, valid and invalid, with their expected outcomes and signatures, so
any app can test itself without our reference receiver.

## Consequences

- Apps need idempotent storage keyed by delivery ID, a signature check and a
  paged changes feed; the reference receiver (`analyte-receiver`) shows all
  three.
- Signing uses a published scheme with existing libraries in most languages;
  Analyte invents no cryptography.
- Two secrets per destination (API key and signing secret) must be stored
  and rotated by the host, never by a driver.
- A breaking change needs contract version 2, served alongside version 1
  during migration.

### Frozen wire types

Version 1 document types deliberately own separate copies of the model's text,
value, result and identity validators. Model changes must not silently change
accepted version 1 documents. The property tests in
`crates/core/contract/tests/agreement.rs` compare acceptance and rejection for bounded
text, decimal notation, UUID identities, UTC offsets, bounded and nonempty
evidence lists, unit mapping sources and decoded text interpretation. A divergence
requires an explicit contract decision rather than sharing these implementations.


The independence includes acceptance rules, not only JSON shapes. No validator
in these six modules is a shared policy: each defines what version 1 accepts.
Sharing a model constructor would change that policy when the model evolves.

| Contract module | Reason to retain its own implementation |
| --- | --- |
| `value.rs` | Decimal grammar and length, decoded-text compatibility and exact unit-source matching are version-1 acceptance rules. Value variants and their field shapes are frozen. |
| `text.rs` | Requiredness, UTF-8 byte bounds and control-character rules define accepted version-1 identifiers and evidence. |
| `identity.rs` | RFC UUIDv7 acceptance, observation keys and revisions define version-1 identity, independently of future internal identity changes. |
| `list.rs` | The 256-entry bound, bounded deserialization and nonempty replacement evidence define version-1 input limits. |
| `time.rs` | Precision variants, optional offset evidence and accepted civil offset range belong to version 1; Jiff already owns the underlying offset validation. |
| `sample.rs` | Provenance and carrier variants and optional position fields are wire shapes; their leaf validation is owned by version-1 text types. |

These exact contract paths are excluded from the production duplication gate;
see `scripts/duplication-exclusions.txt`. Model paths remain measured. The
agreement tests remain required even though these intentional copies are excluded.
