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
