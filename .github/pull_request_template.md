## What and why

<!-- What this changes and why, in a few plain sentences. -->

Closes #

## Evidence level

<!-- Does this change what any protocol or machine claims (documented, simulated, proven)?
     A level is only raised with the evidence that justifies it: name the recording or simulator run. -->

- [ ] No evidence level changes
- [ ] Evidence level changes (which, and the evidence):

## Clinical safety rules

<!-- The rules are in AGENTS.md ("Clinical safety rules") and ADR 0003. -->

- [ ] This does not touch a clinical safety rule
- [ ] This touches a safety rule (which numbers, and how the tests cover it):

## Checks

See [AGENTS.md Verification](../AGENTS.md#verification). Tick what you ran; say why for any you did not.

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo deny check`
- [ ] `cargo check --manifest-path fuzz/Cargo.toml --bins`
- [ ] `cargo clippy -p analyte -p analyte-sink --all-targets --no-default-features -- -D warnings`
- [ ] `cargo test -p analyte -p analyte-sink --no-default-features`
- [ ] `bash scripts/check-layout.sh`

## Before you submit

- [ ] Every commit is signed off (`git commit -s`, see [CONTRIBUTING.md](../CONTRIBUTING.md#sign-off-dco)).
- [ ] No patient data, real sample IDs, real captures or credentials.
- [ ] No text, tables or examples copied from standards or vendor manuals; notes are in our own words.
- [ ] If AI tools helped, I say which parts, and I have reviewed and tested the change myself.
