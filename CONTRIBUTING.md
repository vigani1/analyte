# Contributing to Analyte

Analyte moves clinical lab results. A silent mistake can reach a patient
report, so contributions are held to the rules in [AGENTS.md](AGENTS.md),
which apply to people and coding assistants alike.

## Before you start

- Read [docs/plan.md](docs/plan.md) and the ADRs in [docs/adr/](docs/adr/).
- For anything larger than a small fix, open an issue first and describe the
  change, so the design can be agreed before the code is written.
- Supporting a new machine starts with its documents and, ideally, a
  de-identified recording of a real conversation.

## What every change needs

- Tests that fail without the change and pass with it. Protocol rules name
  their source in the test, for example `HL7 2.5.1 §2.7 escape sequences`.
- A clean run of `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo test --workspace` and
  `cargo deny check`.
- Only synthetic or de-identified data. Never commit patient data, vendor
  manuals, standards documents or text copied from them; write notes in your
  own words.
- Each machine config or driver states its evidence level (proven, simulated
  or documented) and never raises it without the evidence.

## Store startup and service tests

`[store].startup_timeout_seconds` bounds opening, full integrity verification
and migration together. It defaults to 600 seconds; validation accepts 30 through
3600 seconds. The program carries this as a `Duration`. Ordinary store requests
retain their separate short deadlines. Startup logs contain the active step,
migration versions and elapsed milliseconds, never stored payloads. A timeout
names its step. SQLite cancellation rolls back the active migration transaction;
earlier committed versions remain valid and the next startup resumes from them.
A commit already completed when cancellation arrives remains fully committed.

The shared integration-test `Running` helper waits for service readiness and
returns the original startup error before any peer/cache wait. Its 60-second
hang guard is unchanged. To repeat the manual capacity measurement (creates and
removes a synthetic store of at least 2 GiB in a temporary directory), run:

```text
cargo test -p analyte-store --release startup_timing_two_gib_synthetic_store -- --ignored --nocapture
```

On Windows on 2026-11-15, a 2,150,469,632-byte store took 1.713..1.775 seconds
for standalone integrity checks. Migration 7 to 8 took 21.042 seconds and 8 to 9
another 1.483 seconds; complete upgrade startup took 24.014 seconds. Current-schema
restarts took 1.604..1.630 seconds. These are local disk/cache measurements, not a
capacity guarantee. The ten-minute default allows substantial headroom for slower
disks, contention and larger stores; measure the deployment and configure its
budget within the supported range.

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
