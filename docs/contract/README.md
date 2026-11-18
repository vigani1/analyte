# App contract

[v1.md](v1.md) is the contract between Analyte and a lab app: result
deliveries, order events and the order feed. [vectors/](vectors/) holds the
published test vectors; `generate.py` writes them.

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
Analyte contract v1 check, 38 cases
RESULT  CASE                                     RULE (docs/contract/v1.md)
PASS    idempotency.first                        Delivery responses and retries
FAIL    idempotency.retry                        Delivery responses and retries
        expected HTTP 200 with contract_version 1 and status duplicate; got HTTP 200 (Analyte: done, accepted)
SKIP    seed.orders-present                      GET /orders
        load docs/contract/vectors/seed-orders.json into your app, then rerun with --seed-file docs/contract/vectors/seed-orders.json
32 passed, 1 failed, 5 skipped
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
