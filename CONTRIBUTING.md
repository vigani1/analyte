# Contributing to Analyte

Analyte moves clinical lab results. A silent mistake can reach a patient
report, so contributions are held to the rules in [AGENTS.md](AGENTS.md),
which apply to people and coding assistants alike.

## Before you start

Save text as UTF-8, including on Windows. Legacy code pages can silently replace
section marks and range dashes with question marks; the Layout check rejects
these suspicious patterns.

- Read [docs/plan.md](docs/plan.md) and the ADRs in [docs/adr/](docs/adr/).
- For anything larger than a small fix, open an issue first and describe the
  change, so the design can be agreed before the code is written.
- Supporting a new machine starts with its documents and, ideally, a
  de-identified recording of a real conversation.

## What every change needs

- Tests that fail without the change and pass with it. Protocol rules name
  their source in the test, for example `HL7 2.5.1 §2.7 escape sequences`.
- A clean run of `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo test --workspace --all-features` and
  `cargo deny check`. Run the production feature pass too:
  `cargo clippy -p analyte -p analyte-sink --all-targets --no-default-features -- -D warnings`
  and `cargo test -p analyte -p analyte-sink --no-default-features`.
  See [AGENTS.md Verification](AGENTS.md#verification) for the remaining handoff checks.
- Only synthetic or de-identified data. Never commit patient data, vendor
  manuals, standards documents or text copied from them; write notes in your
  own words.
- Each machine config or driver states its evidence level (proven, simulated
  or documented) and never raises it without the evidence.

## Test wall-clock budgets

`analyte_fixtures` defines harness and store-request budgets with a fixed CI
scale factor of three: a 180-second hang guard and a 30-second store wait.
These allow scheduler delays and durable writes on loaded Windows runners;
expiry still fails the test. Synthetic loopback service fixtures explicitly
apply the store allowance to delivery workers and order pollers. Production
entry points reject these overrides, and production deadlines stay unchanged.
Do not use these allowances for a protocol timer whose expiry is under test.
Keep those configured deadlines and quiet-window assertions exact. Name waits
with the integration `step` helper so an expiry identifies its operation; print
only synthetic case labels when looping over scenarios.

## Service test concurrency and startup benchmark

To repeat the manual capacity measurement (creates and
removes a synthetic store of at least 2 GiB in a temporary directory), run:

```text
cargo test -p analyte-store --release startup_timing_two_gib_synthetic_store -- --ignored --nocapture
```

Windows measurement, 2026-11-15: a 2,150,469,632-byte synthetic store took
1.713..1.775 seconds for integrity checking and 1.604..1.630 seconds for
restarts. These are local disk/cache measurements.

The global test-thread override was removed after the startup fix. Three concurrent
full analyte integration binaries with default threads passed in 35.19, 35.64 and
35.49 seconds (127 passed, one external-network test ignored in each). Concurrent
comparison runs with explicit four and two threads took 66.85 and 126.31 seconds.
These runs also overlapped compilation; the numbers describe that load rather than
an isolated performance benchmark. No fixture semaphore was needed.

## AI-assisted contributions

AI tools are allowed, with these conditions:

- A person who understands the change has reviewed and tested it before it is
  submitted, and answers review questions personally.
- Say in the pull request that AI assistance was used, and for which parts.
- Technical claims come with evidence: the source section, a test or a
  recording. Never claim a check ran without its output.

## Sign-off (DCO)

Every commit must be signed off under the
[Developer Certificate of Origin 1.1](https://developercertificate.org/):
you certify that you wrote the change or otherwise have the right to submit
it under the project's license. Add the sign-off with `git commit -s`, which
appends a line like:

```text
Signed-off-by: Your Name <you@example.com>
```

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE).
