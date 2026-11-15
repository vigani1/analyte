# Test quality measurements

Coverage measures which lines and smaller code regions ran during tests.
Running a line does not prove that a test checks its answer.

Mutation testing deliberately changes production code in a temporary copy and
runs the tests again. A caught mutation makes a test fail; a missed mutation
needs investigation because it may expose a missing check or may behave exactly
like the original code.

## Scope and reproducibility

Baseline: `ed22d3f695cac78a8ce53b41879514878af92731` on Windows, pinned Rust
1.100.0. Tools are local developer executables, not project dependencies:
cargo-mutants 27.1.0 and cargo-llvm-cov 0.9.1. No mutation job was added to CI.
Synthetic tests retain the documented evidence level; these measurements do
not qualify a machine for patient use.

Commands (PowerShell, from the repository root):

```powershell
$env:Path = "$HOME\.cargo\bin;$env:Path"
cargo install --locked cargo-mutants --version 27.1.0
cargo install --locked cargo-llvm-cov --version 0.9.1
rustup component add llvm-tools-preview
cargo llvm-cov --workspace --all-features
cargo llvm-cov report --json --output-path target/coverage-before.json
cargo llvm-cov report --text --output-path target/coverage-before.txt
New-Item -ItemType Directory -Force target/mutation-before/framing | Out-Null
cargo mutants -p analyte-framing --all-features --timeout 60 --build-timeout 180 -o target/mutation-before/framing
```

The mutation tool uses a scratch copy, not `--in-place`. Each mutant has a
60-second test deadline and a 180-second build deadline; a timeout is recorded
separately, never described as a caught mutation. Results are under
`target/mutation-before/framing/mutants.out/` (ignored build output).

Continue in this order: analyte-dialog, analyte-mapping, analyte-contract,
analyte-sink, analyte-store. For each, repeat the directory creation and mutation
command above, substituting the crate name and output directory. After adding
tests, use a separate `target/mutation-after/<crate>` output directory and retain
the before results. Do not count an unrun crate as having zero missed mutations.

Required handoff checks:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

Tool references: [cargo-mutants](https://mutants.rs/) and
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).
