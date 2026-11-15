# Test quality measurements

Coverage measures which lines and smaller code regions ran during tests.
Running a line does not prove that a test checks its answer.

Mutation testing deliberately changes production code in a temporary copy and
runs the tests again. A caught mutation makes a test fail; a missed mutation
needs investigation because it may expose a missing check or may behave exactly
like the original code.

## First round results

This round stops after framing, as allowed for long runs. Mutations in the other
five requested crates remain unmeasured; their coverage is available below.
Percentages sum covered/count across each crate's `src/` files, excluding
integration test files. Regions are smaller than lines, so a covered line can
still contain an untested decision. These are baseline measurements, before the
new tests; they are not a prediction of the final coverage.

| Crate | Lines % | Regions % | Before caught / missed / unviable / timeout | After |
| --- | ---: | ---: | --- | --- |
| analyte-framing | 97.79 | 97.86 | 66 / 5 / 8 / 0 | 70 / 1 / 8 / 0 |
| analyte-dialog | 92.46 | 87.99 | Not run | Not run |
| analyte-mapping | 84.97 | 81.47 | Not run | Not run |
| analyte-contract | 86.92 | 83.28 | Not run | Not run |
| analyte-sink | 92.86 | 83.53 | Not run | Not run |
| analyte-store | 91.64 | 77.57 | Not run | Not run |
| analyte | 64.60 | 58.03 | Outside requested mutation scope | Not run |
| analyte-astm | 90.73 | 90.20 | Outside requested mutation scope | Not run |
| analyte-autolyser | 84.79 | 77.49 | Outside requested mutation scope | Not run |
| analyte-config | 85.60 | 88.18 | Outside requested mutation scope | Not run |
| analyte-hl7 | 94.79 | 92.24 | Outside requested mutation scope | Not run |
| analyte-model | 91.04 | 89.60 | Outside requested mutation scope | Not run |
| analyte-poct | 85.87 | 78.23 | Outside requested mutation scope | Not run |
| analyte-receiver | 83.58 | 75.71 | Outside requested mutation scope | Not run |
| analyte-sim | 0.00 | 0.00 | Outside requested mutation scope | Not run |
| analyte-transport | 81.42 | 73.17 | Outside requested mutation scope | Not run |
| analyte-vidas | 90.02 | 87.31 | Outside requested mutation scope | Not run |

### Every missed framing mutation

Locations refer to the baseline production files, which this round did not edit.

| Mutation | Decision and evidence |
| --- | --- |
| `astm.rs:315`, checksum OR changed to XOR | Equivalent: hex parsing returns 0–15; multiplying the high nibble by 16 leaves the low four bits clear. OR and XOR therefore give the same byte for every reachable input. Keep production code and tests unchanged. |
| `mllp.rs:113`, `finish` returns no event | Real gap: partial evidence was not checked on disconnect. `adr0003_mllp_r1_1_2_disconnect_retains_partial_block_and_resets` requires the exact incomplete bytes and verifies reset/recovery. |
| `mllp.rs:167`, `> 31` changed to `>= 31` | Real gap: byte 0x1F must be rejected. `mllp_r1_1_2_payload_repertoire_boundary_excludes_unit_separator` checks both encoding and decoding, plus permitted neighboring/high-bit/CR bytes. |
| `mllp.rs:174`, payload length `>` changed to `==` | Real gap: over-limit encoding could escape the check. `rule7_mllp_r1_1_2_encoder_payload_limit_is_inclusive` requires rejection above the limit. |
| `mllp.rs:174`, payload length `>` changed to `>=` | Real gap: a payload exactly at the configured limit could be rejected. The same boundary test checks below, at and above the limit. |

Three tests were added in `crates/framing/tests/custody.rs`. No production bug
was exposed and no production code, dependency, existing test or machine evidence
level changed. This is improved detection of broken code, not a clinical safety
certification.

The complete 79-mutant rerun caught all four behavior-changing misses. The
only survivor is the equivalent checksum operation above. The before run took
about seven minutes alongside cold workspace builds; the after run took about
two minutes. Eight mutants could not compile in each run and none timed out.
Before/after counts use the same production source, mutation tool, package,
features and deadlines. An initial after-run scratch-directory build failed
with Windows access denied; retry succeeded. An intermediate after run was
cancelled when the final boundary tests were added; its partial counts are not
used in this table.

### Uncovered safety regions to follow up

These are zero-count region starts in the baseline LLVM export, rather than
claims that entire functions or lines never ran. Review them against the ADRs
before deciding which deserve tests; errors made unreachable by typed inputs
may not warrant manufacturing invalid internal state.

- Framing: `astm.rs:266–267` (interrupted-frame event alternatives), `:293`
  (idle fallback); `mllp.rs:64` (invalid bound), `:175` (encoder too long),
  `:178` (forbidden encoder byte). The new boundary tests address the latter
  two. Framing itself never authorizes an ACK.
- Dialog: `hl7.rs:254` (commit callback without pending work), `:487–488`
  (timeout retaining malformed evidence), `:500` (failed timeout refusal),
  `:504` (response timeout closes connection); `astm.rs:437–458` has unrun
  subregions in transfer completion/failure handling. These concern stale
  callbacks, bounded waits and custody before ACK (ADRs 0003/0010).
- Store: `revisions.rs:183–184` (same-batch revision history lookup),
  `:195–196` (revision overflow/corruption), `:326–335` (empty reprocessing
  result parking). Commit and revision safety also require fault-path tests,
  not simply executing successful SQL (ADR 0003).
- Mapping: `identity.rs:135–142` (missing LAW order identity), `:145–154`
  (assigning authority identity components). Preserve unresolved identities
  rather than guess (ADR 0003); mutation results remain unknown.
- Contract: `lib.rs:130–138` (receipt timestamp/digests/ID validation),
  `:319–320` (delivery document size refusal); `signing.rs:31,39,41,54,57`
  has unrun validation subregions. These protect trust, delivery identity and
  bounded authenticated input (ADR 0008).
- Sink: `delivery.rs:158,161` (park/pause outcomes), `:167–168` (retry-time
  overflow), `:174,177` (attempt persistence subregions); `polling.rs:145–158`
  has unrun error subregions. Retry identity and unknown order state require
  explicit assertions (ADRs 0003/0004/0008).

Use `target/coverage-before.txt` for annotated counts and
`target/coverage-before.json` for exact source regions. A high coverage
percentage does not close these gaps: framing already exceeded 97% while four
behavior-changing mutations survived.

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
New-Item -ItemType Directory -Force target/mutation-after/framing | Out-Null
cargo mutants -p analyte-framing --all-features --timeout 60 --build-timeout 180 -o target/mutation-after/framing
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

## Verification and remaining work

Local Windows checks: formatting, workspace Clippy with all targets/features,
workspace tests with all features, and cargo deny pass. Cargo deny reported
duplicate dependency warnings but all four policy checks passed. The coverage
command also completed successfully. No checks on Linux or the minimum Rust
version were run locally; those remain CI checks.

This is a completed framing round, not a completed six-crate mutation audit.
Next run dialog, then mapping, contract, sink and store and triage every miss.
Their existing tests and the safety regions above have not yet been challenged
by mutations. No files owned by the fuzz/timing builder were edited.
