# Test quality measurements

Coverage measures which lines and smaller code regions ran during tests.
Running a line does not prove that a test checks its answer.

Mutation testing deliberately changes production code in a temporary copy and
runs the tests again. A caught mutation makes a test fail; a missed mutation
needs investigation because it may expose a missing check or may behave exactly
like the original code.

## Completed round results

Framing and dialog are complete historical rounds; this round stops after mapping.
Contract, sink and store remain unmeasured; their coverage is available below.
Percentages sum covered/count across each crate's `src/` files, excluding
integration test files. Regions are smaller than lines, so a covered line can
still contain an untested decision. These are baseline measurements, before the
new tests; they are not a prediction of the final coverage.

| Crate | Lines % | Regions % | Before caught / missed / unviable / timeout | After |
| --- | ---: | ---: | --- | --- |
| analyte-framing | 97.79 | 97.86 | 66 / 5 / 8 / 0 | 70 / 1 / 8 / 0 |
| analyte-dialog | 92.46 | 87.99 | 90 / 34 / 10 / 0 | 124 / 0 / 10 / 0 |
| analyte-mapping | 84.97 | 81.47 | 221 / 185 / 49 / 0 | 399 / 7 / 49 / 0 (combined replays) |
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

### Official-source audit after merging current main

The initial framing/dialog rounds had not read the official text directly.
That omission was corrected in this audit using local protected copies under
`C:/Users/super/projects/analyte/references/text/`; no source text was copied
into the repository. Read: `hl7-mllp-r1.txt` §1.2–1.3;
`clsi-lis01-a2.txt` §§6.2.4–6.4.1; HL7 v2.5.1 `V251_CH02.txt`
§§2.9.2–2.9.3, 2.15.5, 2.15.9.9–10; and `V251_CH02A.txt` §2.A.44.
ADRs 0003/0010 and clinical rule 7 supply the host-specific guarantees.

| Added tests audited | Source and conclusion |
| --- | --- |
| Framing encoder size boundary | Rule 7 supplies the ceiling; MLLP R1 §1.2 supplies the envelope. Expectation retained. |
| Framing payload repertoire | MLLP R1 §1.2 excludes 0x1F and permits CR and single-byte values above 0x1F. Expectation retained. |
| Framing partial disconnect evidence/reset | ADR 0003 requires evidence retention; MLLP R1 §1.2 defines when a block is complete. Reset is host decoder policy. Expectation retained. |
| ASTM transfer/request ordinals | ADRs 0003/0010 require matching durable callbacks; these ordinals are host identities, not LIS01 frame numbers. Expectation retained. |
| ASTM peer ENQ takeover | LIS01-A2 §§6.2.6–6.2.7.1 and 6.3.5.3 support busy/contention/interrupt takeover. Expectation retained. |
| ASTM EOT during pending commit | LIS01-A2 §6.4.1 returns the link to neutral; ADR 0003 prevents a late callback from ACKing it. Expectation retained. |
| ASTM sender byte/record ceilings | Rule 7 defines configured inclusive ceilings and rejection before transmission; §6.3 supplies framing only. Expectation retained. |
| ASTM split-frame ceiling | Rule 7 bounds encoded frames; LIS01-A2 §6.3.1 supplies splitting. Expectation retained. |
| ASTM receiver ceilings | Rule 7 bounds retained work; ADR 0003 requires refusal without commit/ACK. Expectation retained. |
| HL7 receipt namespace/ordinal | ADR 0003 host identity; not MSH-10. Fixed the shared fixture's missing MSH-9.3 per §§2.15.9.9/2.A.44. |
| HL7 partial-input deadline | Rule 7 host absolute deadline; MLLP R1 specifies no timeout value. Expectation retained. |
| HL7 busy send refusal | ADR 0010 custody isolation with response phases from §§2.9.2–2.9.3. Fixture corrected as above. |
| HL7 callback identity/phase matrix | ADRs 0003/0010 host callback rules, with §§2.9.2–2.9.3 response phases. Fixture corrected as above. |
| HL7 diagnostic bounds | Rule 7 host metadata ceiling; §2.15.5 does not prescribe a 64-byte application-code ceiling. Renamed to `rule7_host_error_metadata_bounds_refuse_without_losing_pending_receipt`; assertions retained as explicit host policy. |

The fixture previously implied that a two-component MSH-9 was valid for this
2.5.1 exchange. Main now correctly rejects that input; expectations were not
changed to accept it. After the fixture correction all added framing/dialog
tests pass on merged main. Comments distinguish standard wire requirements
from host safety policy. Historical mutation counts above remain historical;
they were not rerun against the newly merged production code.

### Dialog round: every missed mutation

Dialog baseline: `14fa7875572ec7da6d5084a0497aa26a795140c7`, with the same
production code as the coverage baseline. The baseline exercised 134 mutants:
90 caught, 34 missed, 10 unable to compile, no timeouts. Each miss below is a
real test gap; none is classified as equivalent or irrelevant. Locations refer
to unchanged production files under `crates/dialog/src/`. Grouped rows enumerate
every distinct mutation and give the test that checks its behavior.

| Missed mutation(s) | Behavior required and new test |
| --- | --- |
| `astm.rs:141:31`, `||` → `&&` | Reject an empty outgoing batch without starting a transfer: `rule7_lis01_6_3_sender_limits_are_inclusive_and_refuse_before_enq`. |
| `astm.rs:141:48`, `>` → `==`; `>` → `>=` | Accept a batch at the record/frame ceiling: same sender-limits test. Later split-frame checks also enforce the ceiling; these mutants wrongly reject valid batches at the ceiling. |
| `astm.rs:146:22`, `>` → `==`; `>` → `>=` | Accept exactly the byte ceiling and refuse larger payloads before ENQ: same sender-limits test. |
| `astm.rs:282:21`, `||` → `&&` | Either exhausted frame capacity or excessive record bytes must independently refuse before commit/ACK: `adr0003_rule7_lis01_6_3_receiver_limits_refuse_without_commit_or_ack`. |
| `astm.rs:286:43`, `>` → `==`; `>` → `>=` | Receive exact-limit records; reject over-limit records with raw evidence and NAK, no commit/ACK: same receiver-limits test. |
| `astm.rs:369:25`, `==` → `!=` | In computer contention, ignore unrelated controls and accept the peer's next ENQ: `lis01_6_2_6_6_2_7_6_3_5_peer_enq_can_take_over_waiting_sender`. |
| `astm.rs:436:25`, `==` → `!=` | EOT during a pending commit ends incomplete; unrelated controls do not. A late callback cannot ACK: `adr0003_lis01_6_4_eot_during_commit_ends_incomplete_and_late_commit_never_acks`. |
| `astm.rs:445:25`, `==` → `!=` | The interrupted sender must accept peer ENQ during holdoff: peer-ENQ takeover test above. |
| `astm.rs:450:25`, `==` → `!=` | The busy sender yields to peer ENQ but ignores unrelated controls: peer-ENQ takeover test above. |
| `hl7.rs:16:9`, epoch → `0`; epoch → `1` | Return the actual connection incarnation for host journaling: `adr0003_hl7_receipt_namespace_and_ordinal_survive_valid_and_malformed_input`. |
| `hl7.rs:21:9`, ordinal → `0`; ordinal → `1` | Advance receipt ordinals across valid and malformed messages: same namespace/ordinal test. |
| `hl7.rs:216:13`, `&&` → `||` | Further partial input must not postpone the existing absolute deadline: `rule7_mllp_r1_partial_input_does_not_extend_absolute_deadline`. |
| `hl7.rs:238:44`, `:239:21`, `:240:21`, each `||` → `&&` | Do not send over pending receipt, malformed input, outgoing work or a partial block: `adr0010_hl7_251_2_9_send_cannot_replace_active_custody_or_response`. |
| `hl7.rs:274:53`, `&&` → `||` | Failed-commit callbacks require both the matching receipt and commit phase: `adr0003_0010_hl7_251_2_9_callbacks_require_matching_receipt_and_phase`. |
| `hl7.rs:286:49`, `&&` → `||` | Interpretation-failure callbacks require both the matching receipt and interpretation phase: same callback test. |
| `hl7.rs:294:37`, `||` → `&&` | Response metadata requires the matching receipt in response phase: same callback test. |
| `hl7.rs:346:13`, `||` → `&&` | A completion for another receipt must not change the pending receipt: same callback test. |
| `hl7.rs:369:33`, `:369:53`, each `||` → `&&` | Each oversized diagnostic component independently refuses: `rule7_hl7_251_2_15_5_error_metadata_bounds_refuse_without_losing_pending_receipt`. |
| `hl7.rs:369:29`, `>` → `<`; `:369:49`, `>` → `<` | Valid segment/code widths must be accepted; oversize ones refused: same diagnostic-bounds test. |
| `hl7.rs:369:81`, `>` → `==`; `>` → `>=` | Accept 64-byte application diagnostics, refuse 65-byte ones: same diagnostic-bounds test. |
| `astm/types.rs:214:9`, transfer → `0`; transfer → `1` | Host-visible transfer identity must follow each accepted ENQ: `adr0003_0010_astm_commit_identity_tracks_transfer_and_failed_request_ordinals`. |
| `astm/types.rs:220:9`, frame → `0`; frame → `1` | Each durable request consumes an ordinal, including failed writes: same commit-identity test. |

The complete rerun caught 124 mutants, with zero missed, ten unable to compile
and no timeouts. Matching the before/after outcome records confirms that all
34 baseline misses became caught mutants. Both complete runs took about four
minutes. No equivalent-mutation exclusions or production changes were needed.

Eleven tests were added in `crates/dialog/tests/boundaries.rs` and
`crates/dialog/tests/hl7_boundaries.rs`. The additional split-frame test checks
that the configured frame ceiling applies after encoding, not just to the count
of supplied records. Tests use synthetic bytes and controlled time. No existing
test was weakened and no production bug or production change was needed.

The before/after dialog runs use cargo-mutants 27.1.0, the same unchanged
production source, all features, scratch copies and the same 60-second test /
180-second build deadlines. An intermediate after run was stopped when the
baseline's final identity-accessor misses became available, so those tests
could be included; only the subsequent complete run belongs in the table.
The original workspace LLVM coverage remains the comparison baseline; coverage
was not rerun for this round. Local formatting, workspace Clippy, workspace
tests (all features) and cargo deny passed after the new tests; deny retains
the existing duplicate-dependency warnings.

### Mapping round after the standards/store merge

Merged `origin/main` (`4acc00a18ed3538ebf37be649fa1dbb6b29935f4`) with sign-off
in merge commit `997834a151af75e7c0fed51e2a754d43f15351d5`. The mapping baseline
was run on that merged production code, before adding mapping tests. The full
455-mutant baseline took about 19 minutes: 221 caught, 185 missed, 49 unable to
compile, no timeouts. The complete rerun took about 13 minutes and measured
391 caught / 15 missed / 49 unviable / 0 timeout. Three focused replays after
strengthening assertions covered the same five `set`, 56 Autolyser/HL7-group,
and nine value-mapping mutations: respectively 4 caught / 1 equivalent,
54 caught / 2 equivalent, and 9 caught / 0 missed. They caught all eight
remaining behavior-changing survivors. The combined row above counts each
of the original 455 mutations once, using the latest replay for that exact
mutation: 399 caught, seven equivalent survivors, 49 unviable, no timeouts.
It is not presented as a second complete run with the final test revision.

All 185 baseline misses are individually accounted for in the
[mapping triage appendix](mapping-mutations.md): 178 real gaps became caught,
seven remain equivalent. The equivalence proofs rely on validated bounds,
repertoire checks, unique source-node indices and parser header validation;
they do not dismiss difficult-to-test clinical behavior. No production bug
was exposed and no production code changed. Thirty-eight deterministic tests
were added, with no weakened/skipped existing tests or dependency changes.

Coverage percentages remain the earlier workspace
snapshot, not a new measurement of this substantially changed main branch.

New tests check observable mapping outcomes: exact raw values, sample and AWOS
identities, complete observation-group IDs, source statuses/paths/times,
unknown-test parking, classification conflicts, independent resource ceilings,
outbound context/namespace refusal and exact-limit acceptance. Host limits are
cited as clinical rules/ADRs rather than invented standard limits. The LAW
test explicitly refuses routing a validated LAW message through generic HL7
mapping and checks that dedicated mapping produces its result.

Official text read for mapping expectations: LIS02-A2 §§5.6.2, 6.12, 6.14,
8.4.7, 8.4.26, 9.9–9.13; HL7 v2.5.1 ch.2 §§2.7, 2.15.9.11 and ch.2A
§§2.A.22, 2.A.70, 2.A.77; ch.7 ORU/OUL structures, §§7.4.1.7,
7.4.1.25, 7.4.2.2, 7.4.3.17–19; ch.4 §4.5.1.5; IHE PaLM TF-2b
rev.11 §3.29.4.1.2, B.3 and C.7; Autolyser operator manual rev.18
chapter 23 (patient download and script field definitions, including p.186).
These remain local protected sources; fixtures are synthetic.

Source findings to retain for follow-up:

- The Autolyser manual permits cup `00` for automatic positioning; Analyte
  currently refuses it in outbound mapping. No test was added claiming that
  the manual forbids `00`, and this round does not enable automatic assignment.
- HL7 SN supports additional forms, including the not-equal comparator and
  suffix-only categorical notation. Current mapping retains and parks forms
  it cannot interpret; the new unsupported-comparator test checks that they
  cannot silently become equality. It does not label all unsupported forms
  invalid under the standard.
- Numeric offset evidence and a legally defined timezone are separate things.
  The mapper preserves numeric components but does not establish that an offset
  identifies a real timezone for the source location/date. The arithmetic
  boundary test makes no timezone-validity or conversion claim.
- No VIDAS manual exists among the supplied text files. Its new mapping tests
  assert host download policy, identity preservation and configured bounds,
  with no new vendor-conformance claim or evidence-level change.

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
New-Item -ItemType Directory -Force target/mutation-before/dialog | Out-Null
cargo mutants -p analyte-dialog --all-features --timeout 60 --build-timeout 180 -o target/mutation-before/dialog
New-Item -ItemType Directory -Force target/mutation-after/dialog | Out-Null
cargo mutants -p analyte-dialog --all-features --timeout 60 --build-timeout 180 -o target/mutation-after/dialog
New-Item -ItemType Directory -Force target/mutation-before/mapping | Out-Null
cargo mutants -p analyte-mapping --all-features --timeout 60 --build-timeout 180 -o target/mutation-before/mapping
New-Item -ItemType Directory -Force target/mutation-after/mapping | Out-Null
cargo mutants -p analyte-mapping --all-features --timeout 60 --build-timeout 180 -j 4 -o target/mutation-after/mapping
New-Item -ItemType Directory -Force target/mutation-recheck/mapping | Out-Null
cargo mutants -p analyte-mapping --all-features --timeout 60 --build-timeout 180 -F 'in set$' -o target/mutation-recheck/mapping
New-Item -ItemType Directory -Force target/mutation-final-recheck/mapping | Out-Null
cargo mutants -p analyte-mapping --all-features --timeout 60 --build-timeout 180 -j 4 -F 'in autolyser$|in hl7_segments$' -o target/mutation-final-recheck/mapping
New-Item -ItemType Directory -Force target/mutation-value-recheck/mapping | Out-Null
cargo mutants -p analyte-mapping --all-features --timeout 60 --build-timeout 180 -F 'in values$' -o target/mutation-value-recheck/mapping
```

The mutation tool uses a scratch copy, not `--in-place`. Each mutant has a
60-second test deadline and a 180-second build deadline; a timeout is recorded
separately, never described as a caught mutation. Results are under
`target/mutation-before/framing/mutants.out/` (ignored build output).
Mapping runs use the analogous directories shown above. An initial quoted
empty-match-arm filter selected no mutations in PowerShell; it is not counted.
The subsequent `in values$` replay selected all nine value-mapping mutations.

Continue in this order: analyte-contract,
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

This is framing and dialog followed by the mapping round, not a completed
six-crate mutation audit. Next run contract, sink and store, one crate
per round, and triage every miss.
Their existing tests and the safety regions above have not yet been challenged
by mutations. No files owned by the fuzz/timing builder were edited.

### Program coverage remains a priority

The program crate `analyte` has the lowest application/runtime crate coverage:
64.6% of lines (4,647/7,194), with 2,547 uncovered lines. The simulator library
is separately reported at 0%; it is not the deployed program. No mutations
have been run on `analyte`. The largest uncovered modules in the baseline LLVM
output are listed by number of uncovered lines, not just by percentage:

| Module under `crates/analyte/src/` | Uncovered / total lines | Risk to check |
| --- | ---: | --- |
| `admin/pages.rs` | 408 / 408 | Operators may miss parked data or uncertain orders if the displayed status is wrong. |
| `service.rs` | 306 / 360 | Startup, shutdown and task ownership must preserve durable work through failures. |
| `admin/server.rs` | 216 / 225 | Authentication and operator request routing protect access to clinical data and actions. |
| `admin/actions.rs` | 201 / 201 | Reprocess/resend actions must stay audited and preserve result/delivery identities. |
| `admin/overview.rs` | 181 / 181 | Incorrect aggregation can hide stalled deliveries or storage pressure. |
| `orders/hl7.rs` | 126 / 587 | Query replies must distinguish unknown orders from no orders and reserve work before sending. |
| `windows_service.rs` | 111 / 111 | Service lifecycle failures could interrupt acquisition or leave shutdown incomplete. |
| `machine/serial.rs` | 88 / 88 | Serial disconnects, partial writes and bounded waits must preserve uncertainty and raw evidence. |
| `machine/poct.rs` | 85 / 217 | Receipt/interpretation and acknowledgment wiring must maintain durable custody. |
| `config.rs` | 67 / 267 | Invalid settings or activation changes must not silently alter queued work. |

These are coverage findings, not confirmed defects. OS integration and the
admin paths need separate behavioral validation; executing their code alone
would not prove safety. Counts come from each file's `summary.lines` in
`target/coverage-before.json` (`count - covered`), generated by the commands above.
