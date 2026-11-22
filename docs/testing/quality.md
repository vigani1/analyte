# Test quality measurements

Coverage measures which lines and smaller code regions ran during tests.
Running a line does not prove that a test checks its answer.

Mutation testing deliberately changes production code in a temporary copy and
runs the tests again. A caught mutation makes a test fail; a missed mutation
needs investigation because it may expose a missing check or may behave exactly
like the original code. A timeout is recorded separately and never counted as
caught.

These measurements improve how well the tests detect broken code. They are not
a clinical safety certification and do not raise any evidence level.

## Results so far

Mutation rounds ran on framing, dialog, mapping, contract, sink and store
between 2026-11-15 and 2026-11-17. Coverage is the baseline before those
rounds added tests: percentages sum covered/count across each crate's `src/`
files, excluding integration test files. Regions are smaller than lines, so a
covered line can still contain an untested decision.

| Crate | Lines % | Regions % | Mutants before (caught / missed / unviable / timeout) | After |
| --- | ---: | ---: | --- | --- |
| analyte-framing | 97.79 | 97.86 | 66 / 5 / 8 / 0 | 70 / 1 / 8 / 0; the survivor is equivalent |
| analyte-dialog | 92.46 | 87.99 | 90 / 34 / 10 / 0 | 124 / 0 / 10 / 0 |
| analyte-mapping | 84.97 | 81.47 | 221 / 185 / 49 / 0 | 399 / 7 / 49 / 0; the 7 survivors are equivalent |
| analyte-contract | 86.92 | 83.28 | 53 / 52 / 71 / 0 | 105 / 0 / 71 / 0 |
| analyte-sink | 92.86 | 83.53 | 32 / 46 / 18 / 16 | 91 / 3 / 18 / 0; the 3 survivors are equivalent |
| analyte-store | 91.64 | 77.57 | Focused baselines per module | Every module measured; two worker mutations left uncaught (see below) |
| analyte | 64.60 | 58.03 | Not run | Not run |
| analyte-astm | 90.73 | 90.20 | Not run | Not run |
| analyte-autolyser | 84.79 | 77.49 | Not run | Not run |
| analyte-config | 85.60 | 88.18 | Not run | Not run |
| analyte-hl7 | 94.79 | 92.24 | Not run | Not run |
| analyte-model | 91.04 | 89.60 | Not run | Not run |
| analyte-poct | 85.87 | 78.23 | Not run | Not run |
| analyte-receiver | 83.58 | 75.71 | Not run | Not run |
| analyte-sim | 0.00 | 0.00 | Not run | Not run |
| analyte-transport | 81.42 | 73.17 | Not run | Not run |
| analyte-vidas | 90.02 | 87.31 | Not run | Not run |

"Equivalent" means the mutated code behaves exactly like the original for
every input that validation lets through; each such survivor was argued case
by case. Every caught row was checked against a failing-test log, not inferred
from coverage. The store rounds found three real restore bugs (a zero page
limit, a missing disk-reserve check and a size cap measured before the schema
upgrade); each was fixed with its regression test kept.

The per-mutant triage tables of these rounds (every missed mutant, the test
that now catches it or the reason it is equivalent) were removed from the tree
on 2026-11-21 because their file and line locations no longer match the code.
They remain in git history: `git show eaadbd0c:docs/testing/mapping-mutations.md`,
`contract-sink-mutations.md` and `store-mutations.md` in the same folder.

## Still open

- **Store worker:** at the end of the store round two mutations in the store's
  owning-thread worker were still not caught: the deadline equality check and
  the startup-cancel guard's drop. The worker was refactored afterwards and has
  not been measured again.
- **The program crate `analyte`** has the lowest coverage (64.6% of lines at
  the baseline) and no mutation run. Its riskiest parts are the admin pages and
  actions, service start and stop, Windows service glue, serial connections and
  HL7 order replies. Running their code alone would not prove them safe; they
  need behaviour tests.
- **Uncovered safety regions** found in the baseline: interrupted and idle
  framing paths, stale dialog callbacks and timeouts that keep malformed
  evidence, revision overflow and empty reprocessing in the store, missing LAW
  order identity in mapping, receipt and signature validation in the contract,
  and park, pause and retry-time overflow in the sink. Review each against the
  ADRs before adding a test; an error that typed inputs make unreachable may
  not warrant building invalid internal state.
- Crates marked "Not run" above have no mutation results. Never count an unrun
  crate as having zero missed mutations.

## How to repeat a run

The tools are local developer executables, not project dependencies, and no
mutation job runs in CI. Measured with cargo-mutants 27.1.0 and cargo-llvm-cov
0.9.1 on Windows with the pinned Rust toolchain. From the repository root, in
PowerShell:

```powershell
cargo install --locked cargo-mutants --version 27.1.0
cargo install --locked cargo-llvm-cov --version 0.9.1
rustup component add llvm-tools-preview
cargo llvm-cov --workspace --all-features
cargo llvm-cov report --json --output-path target/coverage.json
cargo llvm-cov report --text --output-path target/coverage.txt
cargo mutants -p analyte-framing --all-features --timeout 60 --build-timeout 180 -o target/mutation-before/framing
```

Substitute the crate name and output folder for other crates. The mutation
tool works on a scratch copy, never `--in-place`. Each mutant gets a 60-second
test deadline and a 180-second build deadline. After adding tests, run again
into a separate `target/mutation-after/<crate>` folder and keep the before
results; `-F '<regex>'` replays only the named mutations.

Raw logs stay out of the repository; attach them to the pull request when a
reviewer needs them. Each triage row names the test that catches the mutant or
the exact reason it is equivalent.

Tool references: [cargo-mutants](https://mutants.rs/) and
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov).

## Duplication

CI also limits copied code in production and tests. The thresholds, the
exclusions and how to run the check are in
[scripts/duplication.md](../../scripts/duplication.md).
