# App contract

This folder is for **connecting a lab app to Analyte**: the calls the app
answers and how to check them. Not here: running Analyte (see
[operations](../operations/README.md)).

[v1.md](v1.md) is the contract between Analyte and a lab app: result
deliveries, order events and the order feed. [vectors/](vectors/) holds the
published test vectors; `generate.py` writes them.

| File | What it is |
| --- | --- |
| [contract-v1.schema.json](contract-v1.schema.json) | JSON Schema (draft 2020-12) of every request and answer body |
| [openapi.json](openapi.json) | OpenAPI 3.1 document of `POST /results`, `POST /order-events` and `GET /orders`, with the key and signature headers |
| [machines.json](machines.json) | The machine catalog: each shipped model's id (`source.model`), name, vendor, protocol, evidence level, ready or blocked, and code systems |
| [machines.schema.json](machines.schema.json) | JSON Schema of the catalog |

`cargo run -p analyte-publish` generates these four files from the Rust
contract types and the `machines/` folder; do not edit them by hand. Each
release attaches them too ([ADR 0032](../adr/0032-machine-model-and-published-schema.md)).

## Building a lab app

Use the schema to generate types in your language: point a JSON Schema or
OpenAPI code generator at `openapi.json` (it refers to
`contract-v1.schema.json` beside it), or validate incoming bodies against the
schema's `Delivery` and `OrderEvent` definitions. Like the contract, the
schema allows and ignores unknown fields. It cannot count bytes or compare two
values, so three rules in [v1.md](v1.md) still need your own check: byte
limits, each result's receipt equal to the envelope's, and a unit mapping's
source equal to the original unit.

Key your per-model test and specimen tables on `source.model`; the catalog
lists the values and each model's code systems. A model with `"ready": false`
is a blocked definition: Analyte cannot run it yet.

## Check your app

`analyte-contract-check` runs the published vectors against your app's real
endpoints and prints PASS, FAIL or SKIP for each case. Your app can be written
in any language; the checker only talks HTTP to it.

### What it needs

The same three things Analyte needs for your app, as environment variables or
in a file of `NAME=value` lines passed with `--env-file`. They are never
command-line arguments and never printed.

| Name | Value |
| --- | --- |
| `ANALYTE_APP_URL` | Base URL; the checker calls `results`, `order-events` and `orders` under it. HTTPS with a certificate the system trusts, or `http://` to a literal loopback address (`127.0.0.1`, `[::1]`) for local development. |
| `ANALYTE_API_KEY` | The bearer key your app expects. |
| `ANALYTE_SIGNING_SECRET` | The `whsec_…` Standard Webhooks secret your app trusts. |
| `ANALYTE_REQUEST_TIMEOUT_SECONDS` | Optional, 1 to 120, default 10: the deadline for each whole request, as in Analyte. |

The checker behaves like Analyte: it never follows redirects, never skips
certificate checks and reads at most 1,048,576 bytes of any answer. Every run
uses new synthetic delivery IDs, so you can run it again against the same app.
It writes only synthetic data from the vectors; use a test instance, not a
production app.

### How to run it

From a checkout of this repository (Rust toolchain installed):

```sh
export ANALYTE_APP_URL=http://127.0.0.1:8080
export ANALYTE_API_KEY=...            # your test key
export ANALYTE_SIGNING_SECRET=whsec_...
cargo run -p analyte-contract-check
```

On any system, the same three lines can go in a file instead (keep it out of
version control):

```sh
cargo run -p analyte-contract-check -- --env-file app.env
```

The order-feed cases that need known orders are skipped until you load
[vectors/seed-orders.json](vectors/seed-orders.json) into your app (four
orders: two requested, one cancellation, one tombstone). Then tell the checker
what you loaded:

```sh
cargo run -p analyte-contract-check -- --seed-file docs/contract/vectors/seed-orders.json
```

The reference receiver loads the same file with
`analyte-receiver --seed docs/contract/vectors/seed-orders.json`.

### How to read the output

```text
Analyte contract v1 check, 40 cases
RESULT  CASE                                     RULE (docs/contract/v1.md)
PASS    idempotency.first                        Delivery responses and retries
FAIL    idempotency.retry                        Delivery responses and retries
        expected HTTP 200 with contract_version 1 and status duplicate; got HTTP 200 (Analyte: done, accepted)
SKIP    seed.orders-present                      GET /orders
        load docs/contract/vectors/seed-orders.json into your app, then rerun with --seed-file docs/contract/vectors/seed-orders.json
34 passed, 1 failed, 5 skipped
```

- **CASE** is the vector file and the vector's name, so `idempotency.retry` is
  the `retry` step in [vectors/idempotency.json](vectors/idempotency.json).
- **RULE** is the section of [v1.md](v1.md) the case checks.
- A **FAIL** line is followed by one reason: what the contract expects, what
  your app answered, and what Analyte would do with that answer (done, parked,
  destination paused or retried later).
- A **SKIP** says what was not checked and what to do. Skips do not fail a run.
- Exit code 0 means no case failed, 1 means at least one failed, 2 means the
  run could not start (for example a missing setting).

### What it cannot check

Some rules need time or a state the checker cannot create:

- Cursor expiry after one hour; the checker sends an unknown cursor instead,
  which needs the same 410 answer.
- Changes made while a snapshot is being paged, and tombstones that appear in
  "changes since"; the checker sees only what the app holds during the run.
- Durable storage before the 200 answer; the checker sees a retry answered as
  a duplicate, not your disk.
- Retry-After on 429 and 5xx answers is checked only if your app sends one
  during the run.

The sender-side vectors (`responses.json`, `retry-after.json`,
`order-versions.json`, `freshness.json`) describe what Analyte does with your
answers; Analyte's own tests run them.
