# Contributing to Analyte

Analyte moves clinical lab results. A silent mistake can reach a patient
report, so contributions are held to the rules in [AGENTS.md](AGENTS.md),
which apply to people and coding assistants alike.

## Ways to help

| You want to | What you need | Start here |
| --- | --- | --- |
| Report something that does not work | Nothing but what you saw | The [bug form](https://github.com/vigani1/analyte/issues/new?template=bug.yml) |
| Ask for a machine, or share what you know about one | No coding: the model, its software version, photos of its host settings | The [new analyzer form](https://github.com/vigani1/analyte/issues/new?template=new-analyzer.yml) |
| Record a real machine's conversation (the most useful help) | Access to the machine; the recording is de-identified before it is shared | [Capture, de-identify and replay](docs/adding-a-machine.md#capture-de-identify-and-replay) |
| Add a machine that speaks a protocol Analyte has (ASTM, HL7, LAW, POCT1-A, VIDAS, Autolyser) | No Rust: a `machine.toml` settings file, a README and recordings in `machines/<vendor>/<model>/` | [Adding a machine](docs/adding-a-machine.md) |
| Add a machine with its own language | Rust; open an issue first, because a new protocol is a large change | [Adding a vendor protocol](docs/adding-a-vendor-protocol.md) |
| Connect a lab app | Any language: three HTTPS calls | [Contract v1](docs/contract/v1.md) and its [checker](docs/contract/README.md#check-your-app) |
| Ask how a protocol rule works | A question | The [protocol question form](https://github.com/vigani1/analyte/issues/new?template=protocol-question.yml) |
| Fix a bug or a document | A fork and a pull request | [Forks and pull requests](#forks-and-pull-requests) |
| Report a security problem | Report privately, never in an issue | [SECURITY.md](SECURITY.md) |

Analyte stays a bridge, so changes that judge results are not accepted:
auto-verification, reflex tests, QC decisions or clinical interpretation belong
in the lab app ([ADR 0003](docs/adr/0003-clinical-safety-rules.md)).

## Before you start

Save text as UTF-8, including on Windows. Legacy code pages can silently replace
section marks and range dashes with question marks; the Layout check rejects
these suspicious patterns.

- Read [docs/plan.md](docs/plan.md) and the ADRs in [docs/adr/](docs/adr/).
- For anything larger than a small fix, open an issue first and describe the
  change, so the design can be agreed before the code is written.
- Supporting a new machine starts with its documents and, ideally, a
  de-identified recording of a real conversation.
- Writing tests that wait on timers or the store: read
  [test timing](docs/testing/timing.md) first.

## Forks and pull requests

1. Fork the repository and create a branch from `main`.
2. Make the change with its tests, sign off every commit (`git commit -s`, see
   [Sign-off](#sign-off-dco)) and run the checks below.
3. Open a pull request to `main` and fill in its template.

The first time you contribute, the maintainer approves your pull request's
workflow runs before CI starts. CI then runs the checks below: layout,
format, Clippy, tests on Windows and Linux, minimum Rust, duplication,
licences and advisories, and short fuzz runs. A pull request merges only when
they all pass.

The maintainer reviews every pull request (see `.github/CODEOWNERS`). Expect
questions about sources and evidence; answer them in the pull request and push
new commits rather than rewriting history. Report security problems privately
([SECURITY.md](SECURITY.md)), and follow the
[code of conduct](CODE_OF_CONDUCT.md).

A new machine definition needs its documents and evidence, as described in
[adding a machine](docs/adding-a-machine.md); its evidence level is only as
high as that evidence.

## What every change needs

- Tests that fail without the change and pass with it. Protocol rule tests have
  a short behaviour name and a comment above them naming the source, for
  example `// HL7 2.5.1 §2.7`.
- A clean run of `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo test --workspace --all-features` and
  `cargo deny check`. Run the production feature pass too:
  `cargo clippy -p analyte -p analyte-sink --all-targets --no-default-features -- -D warnings`
  and `cargo test -p analyte -p analyte-sink --no-default-features`.
  See [AGENTS.md Verification](AGENTS.md#verification) for the remaining handoff checks.
- A clean `bash scripts/check-layout.sh`. It runs only what the standard tools
  cannot check: file headers, file and folder names, crate folders, text
  damaged by a Windows code page, test names in protocol checklists and the
  CI fuzz matrix. CI also runs `python3 scripts/check-duplication.py` (jscpd,
  with production and test code measured separately).
- Only synthetic or de-identified data. Never commit patient data, vendor
  manuals, standards documents or text copied from them; write notes in your
  own words.
- Each machine config or driver states its evidence level (proven, simulated
  or documented) and never raises it without the evidence.

## File headers

Every Rust file opens with one `//!` sentence that a newcomer can read alone
([ADR 0012](docs/adr/0012-uniform-layout-and-naming.md), item 8). Crate READMEs
use the same plain descriptions, stating the crate's kind first.

- Start with a verb and the concrete things handled: `//! Validates LAW MSH,
  MSA and ERR segments.`, not `//! Manager control profiles.`
- Put limits after the job: `//! Maps ASTM result records to model results
  without I/O.`, not `//! No I/O or clinical decisions.`
- Say what the file holds, not where it belongs, and avoid abstract words
  (semantics, profiles, assembly, boundary).
- Avoid words whose lab meaning differs: "collection" means collecting a
  specimen, so write `//! Holds evidence lists capped at 256 entries.`

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
