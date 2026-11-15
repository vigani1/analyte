# Store mutation triage

**Resumed; the custody/delivery/parked subset is complete; the round is incomplete.** Baseline:
`bdb6b6f7af375d5679db462af0fe070f3297bc1f` (origin/main). No production code changed.

## Earlier restore bug (fixed by #88)

On the original baseline, plan section 11 / clinical rule 7: `Store::restore` accepted `Limits { page_rows: 0,
.. }`, although `Store::open` rejected the same limits. Restore called `open_locked`
without the validation in `open_starting`, then published the staged database.
The zero limit violates the public opening invariant. No downstream listing/recovery
failure is claimed: restore drops its internal store before returning.
The reproducer confirms acceptance/publication; it does not claim observed patient loss.

On the original baseline, `plan11_rule7_restore_refuses_zero_page_limit_before_publishing` failed: expected `InvalidInput`, got `Ok(...before-restore)`.
[Failure output](evidence/store/restore-zero-page-limit.log). This failing test is
retained at the original stop as requested, not ignored or loosened; it now passes with #88. The PR stays draft. The owner reports this fixed on `fix/restore-limits` with a shared `Limits::validate()` and this regression unchanged. Fixed by [PR #88](https://github.com/vigani1/analyte/pull/88), merged at `b3270a4`. Merged origin/main with --signoff; retained main's exact restore regression once. Earlier failure logs and counts remain historical.

## Runs

Cargo-mutants 27.1.0; pinned Rust 1.100.0; all features, scratch copies;
60-second tests and 180-second builds. No dependencies added.

```powershell
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 4 -o target/store-mutation-before
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 1 --file crates/store/src/lib.rs -F '162:17' --cargo-test-arg adr0003_rule7_sqlite_lock_failures -o target/store-mutation-recheck-lock
```

The full run discovered 565 mutants, then was interrupted on the confirmed bug.
Only 22 completed outcomes are counted. In-flight/unrun mutations are not timeouts
or equivalent. [Retained partial outcomes](evidence/store/partial-baseline.json).

| Module | Caught | Missed | Equivalent | Unviable | Timeout |
| --- | ---: | ---: | ---: | ---: | ---: |
| lib.rs (partial baseline) | 11 | 4 | 0 classified | 2 | 0 |
| alerts.rs (partial baseline) | 1 | 1 | 0 classified | 3 | 0 |
| All other modules | Not completed | Not completed | Not classified | Not completed | Not completed |
| lib.rs exact lock-error replay | 1 | 0 | 0 | 0 | 0 |

The replay supersedes only its exact named mutation. It is not a full rerun.

| Baseline missed mutant (unchanged production location) | Decision and evidence |
| --- | --- |
| `lib.rs:162:17: delete match arm DatabaseBusy / DatabaseLocked in Error::from` | Real gap closed by `adr0003_rule7_sqlite_lock_failures_remain_specific_timeouts`. Exact mutant caught; [failing-test log](evidence/store/sqlite-lock-mutant.log). |
| `lib.rs:157:17: delete match arm DiskFull in Error::from` | Real gap; new `adr0003_storage_failures_distinguish_full_corrupt_conflict_and_unknown` asserts DiskFull. Replay not completed before stop; closure is not claimed. |
| `lib.rs:218:24: replace > with == in BoundedJson::write` | Behavior-changing size-bound gap; not closed before stop. No equivalence claim. |
| `lib.rs:218:24: replace > with >= in BoundedJson::write` | Behavior-changing inclusive-boundary gap; not closed before stop. No equivalence claim. |
| `alerts.rs:60:9: replace Store::active_alerts with Ok(vec![])` | Behavior-changing operator-visibility gap; not closed before stop. No equivalence claim. |

## Partial claim audit

Read clinical rules 1–4 and 8, ADRs 0003/0010/0011 and the plan. The audit
stopped at the restore bug, so no completed crate-wide source audit is claimed.

| Claim / source | Finding |
| --- | --- |
| Durable custody / ADRs 0003 and 0010 | Renamed `frame_committed_before_ack_survives_restart` to `committed_frames_survive_restart`: this store-only test checks reopening raw frames, not actual ACK timing. |
| Backup / plan section 11 | Renamed `online_backup_restores_exact_receipts` to `online_backup_copy_reopens_with_exact_receipt_bytes`: it opens the backup copy, not the offline restore workflow. |
| Specific refusal / ADR 0003 and rule 7 | Added synthetic SQLite lock/full/corruption/conflict classification checks and a deterministic startup exact-deadline/sticky-cancellation check. |
| Limits / plan section 11 and rule 7 | Restore reproducer exposes bypassed limit validation; stopped without a production fix. |
| Identities, revisions and lifecycle / rules 2–4 and 8, ADR 0011 | Existing suites were inventoried but full behavioral/source audit and mutation coverage remain unfinished. |

Four tests added, including the intentionally failing reproducer. New waits use
shared `tests::HANG_GUARD`; startup boundary uses an injected clock. No new real-time
short hang deadlines, source-standard claims or machine evidence levels were added.

## Verification at stop

Fetched origin/main and ran `git merge --signoff origin/main`: already up to date.
Windows, pinned Rust 1.100.0:

| Check | Output |
| --- | --- |
| `cargo fmt --check` | Exit 0. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Exit 0, final run finished in 24.44s. |
| `cargo test --workspace` | Exit 101 at store: 82 passed, 1 failed (the retained restore regression), 1 ignored benchmark. Cargo stops there, so this is not a complete passing workspace run. |
| `cargo deny check` | Exit 0: advisories ok, bans ok, licenses ok, sources ok; existing duplicate dependency warnings. |

Logs remain under `target/store-checks/`. Earlier focused startup/lock tests passed
(6 passed). The new restore regression also failed independently before the full
workspace run. Linux and minimum-supported-Rust checks were not run locally.
No production code or dependency changes were made. The full mutation run,
remaining miss closures and source audit were not completed because of the
requested stop on a production bug; the PR was not marked ready.

`cargo check --manifest-path fuzz/Cargo.toml --bins` exited 0; final output:
`Finished dev profile [unoptimized + debuginfo] target(s) in 11.96s`.
Fuzz targets were compiled, not executed.


## Resumed custody round after PR #88

Production custody files are unchanged between the initial baseline and merged main `b3270a4`.
All changes relative to main remain tests and documentation. The restore regression is main's exact copy, once, and now passes.
Toolchain and limits remain cargo-mutants 27.1.0, Rust 1.100.0, all features, scratch copies, 60-second tests and 180-second builds.
Test selections are explicit below. Before the fix merged, focused selections avoided the known unrelated failing restore case without editing, ignoring or weakening it.
After merging the fix, full-suite baselines under parallel load hit the tool deadline; narrower source-rule suites resolved them.
No new production bug or equivalent mutation was found in this continuation. No standard or machine evidence level was raised.

### Completed module results

These are combined exact-mutation results, not one full-crate run. A caught result always has an actual failing test log.
Focused suites supplement earlier catches; a later narrow suite omitting an existing test does not invalidate its prior failing-test evidence.
Baseline failures, unrun mutations and aborted in-flight mutations are never counted as caught, equivalent or timed out.

| Module | Caught | Missed | Equivalent | Unviable | Unresolved timeout |
| --- | ---: | ---: | ---: | ---: | ---: |
| `feed.rs` | 13 | 0 | 0 | 3 | 0 |
| `outbox.rs` | 9 | 0 | 0 | 1 | 0 |
| `parked.rs` | 8 | 0 | 0 | 0 | 0 |
| `receipts.rs` | 8 | 0 | 0 | 0 | 0 |
| `records.rs` | 1 | 0 | 0 | 2 | 0 |
| `resend.rs` | 9 | 0 | 0 | 2 | 0 |
| `revisions.rs` | 74 | 0 | 0 | 9 | 0 |
| `revisions/status.rs` | 29 | 0 | 0 | 4 | 0 |
| `transfers.rs` | 12 | 0 | 0 | 0 | 0 |

163 caught / 0 missed / 21 unviable / 0 unresolved timeouts across 184 distinct mutations.
Initial partial lib/alerts results above remain historical and are not added to this total.

### Every continuation run

Raw outputs are retained in `target/store-resume/<run>/mutants.out/`. [Recorded outcomes, commands and phases](evidence/store/resume/runs.json) preserve each completed result.
Aborted runs have a null end time; their completed outcomes only are shown. `parked-before` failed its unmutated baseline by tool timeout and tested no mutants.

| Run | Caught | Missed | Unviable | Timeout | Status |
| --- | ---: | ---: | ---: | ---: | --- |
| `receipts-before` | 5 | 1 | 0 | 2 | complete |
| `custody-before` | 69 | 47 | 15 | 0 | complete |
| `receipts-recheck` | 3 | 0 | 0 | 0 | complete |
| `receipts-raw-recheck` | 4 | 0 | 0 | 0 | complete |
| `custody-first-recheck` | 5 | 0 | 0 | 0 | complete |
| `custody-second-recheck` | 7 | 0 | 0 | 0 | complete |
| `custody-third-recheck` | 15 | 0 | 0 | 0 | complete |
| `custody-migration-recheck` | 10 | 0 | 0 | 0 | complete |
| `custody-status-recheck` | 12 | 9 | 3 | 0 | interrupted; partial |
| `delivery-before` | 2 | 2 | 3 | 3 | interrupted; partial |
| `parked-before` | 0 | 0 | 0 | 0 | baseline timeout; no mutants |
| `custody-status-final` | 32 | 0 | 4 | 0 | complete |
| `delivery-focused` | 26 | 5 | 6 | 0 | complete |
| `parked-focused` | 6 | 2 | 0 | 0 | complete |
| `parked-helper-recheck` | 2 | 0 | 0 | 0 | complete |
| `delivery-recheck` | 5 | 0 | 1 | 0 | complete |
| `resend-superseded-recheck` | 1 | 0 | 0 | 0 | complete |

Commands, from the repository root:

```powershell
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 4 --file crates/store/src/receipts.rs --cargo-test-arg receipt -o target/store-resume/receipts-before
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 4 --file crates/store/src/records.rs --file crates/store/src/transfers.rs --file 'crates/store/src/revisions*.rs' --file 'crates/store/src/revisions/*.rs' --cargo-test-arg adr -o target/store-resume/custody-before
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 1 --file crates/store/src/receipts.rs -F '21:' --cargo-test-arg adr0003_receipt_digests -o target/store-resume/receipts-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 1 --file crates/store/src/receipts.rs -F '35:9' --cargo-test-arg adr0003_receipt_digests -o target/store-resume/receipts-raw-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/transfers.rs --file crates/store/src/revisions.rs -F '141:31: replace match guard retained != evidence with false|156:41|183:17|184:17|192:39' --cargo-test-arg adr0003_ -o target/store-resume/custody-first-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/revisions.rs -F '192:48|196:27|276:50' --cargo-test-arg mutation_ -o target/store-resume/custody-second-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/revisions.rs -F '354:26|358:13|362:25|363:25|386:59|431:17|431:43|450:37|463:25' --cargo-test-arg mutation_ -o target/store-resume/custody-third-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/revisions.rs -F '520:|534:' --cargo-test-arg mutation_adr0003_migration_ -o target/store-resume/custody-migration-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 3 --file crates/store/src/revisions.rs --file crates/store/src/revisions/status.rs -F '586:26|status.rs:' --cargo-test-arg mutation_ -o target/store-resume/custody-status-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 3 --file crates/store/src/outbox.rs --file crates/store/src/feed.rs --file crates/store/src/resend.rs -o target/store-resume/delivery-before
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/parked.rs -o target/store-resume/parked-before
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 3 --file crates/store/src/revisions.rs --file crates/store/src/revisions/status.rs -F '586:26|status.rs:' --cargo-test-arg adr0011 -o target/store-resume/custody-status-final
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/outbox.rs --file crates/store/src/feed.rs --file crates/store/src/resend.rs --cargo-test-arg adr0008 -o target/store-resume/delivery-focused
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 1 --file crates/store/src/parked.rs --cargo-test-arg mutation_adr0003_park_ -o target/store-resume/parked-focused
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 1 --file crates/store/src/parked.rs -F '26:5' --cargo-test-arg mutation_adr0003_park_ -o target/store-resume/parked-helper-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 2 --file crates/store/src/outbox.rs --file crates/store/src/resend.rs -F '127:30|40:9|105:13' --cargo-test-arg mutation_adr0008_ -o target/store-resume/delivery-recheck
cargo mutants -p analyte-store --all-features --timeout 60 --build-timeout 180 -j 1 --file crates/store/src/resend.rs -F '143:34' --cargo-test-arg superseded_resend -o target/store-resume/resend-superseded-recheck
```

### Every missed or timed-out mutation in the continuation

Each row is matched by the full mutation name, not merely line number. Proof files contain the original log path and failing test output excerpts; original full logs remain under target.
Nine intermediate status misses and the superseded-resend miss were selection gaps: the narrow filter omitted existing source-named tests. The final status/source-case replays select those tests and catch them; they are not equivalent mutations or newly discovered production bugs.
Other misses exposed missing checks closed by the new mutation tests. Some proof logs predate the `mutation_` prefix; their assertions are unchanged.

| Missed mutation (unchanged production location) | Decision and evidence |
| --- | --- |
| `crates/store/src/feed.rs:11:30: replace > with < in Store::order_page` | Caught by `order_tests::adr0008_every_item_aggregation_row_is_atomic_and_emits_one_event`, `tests::mutation_adr0008_adr0004_feed_page_bounds_and_lookup_trust_are_independent`, `tests::adr0008_event_provenance_health_ordering_and_restart_attempts`. [Failure log](evidence/store/resume/delivery-focused-crates__store__src__feed.rs_line_11_col_30_001.log). |
| `crates/store/src/feed.rs:11:30: replace > with == in Store::order_page` | Caught by `tests::mutation_adr0008_adr0004_feed_page_bounds_and_lookup_trust_are_independent`. [Failure log](evidence/store/resume/delivery-focused-crates__store__src__feed.rs_line_11_col_30.log). |
| `crates/store/src/feed.rs:11:30: replace > with >= in Store::order_page` | Caught by `tests::mutation_adr0008_adr0004_feed_page_bounds_and_lookup_trust_are_independent`. [Failure log](evidence/store/resume/delivery-focused-crates__store__src__feed.rs_line_11_col_30_002.log). |
| `crates/store/src/feed.rs:11:9: replace Store::order_page -> Result<(), Error> with Ok(())` | Caught by `tests::mutation_adr0008_adr0004_feed_page_bounds_and_lookup_trust_are_independent`, `order_tests::adr0008_every_item_aggregation_row_is_atomic_and_emits_one_event`, `tests::adr0008_event_provenance_health_ordering_and_restart_attempts`. [Failure log](evidence/store/resume/delivery-focused-crates__store__src__feed.rs_line_11_col_9.log). |
| `crates/store/src/feed.rs:65:26: replace > with == in Store::orders` | Caught by `tests::mutation_adr0008_adr0004_feed_page_bounds_and_lookup_trust_are_independent`. [Failure log](evidence/store/resume/delivery-focused-crates__store__src__feed.rs_line_65_col_26.log). |
| `crates/store/src/outbox.rs:127:30: replace > with == in Store::recover_in_flight` | Caught by `tests::mutation_adr0008_recovery_audit_bound_is_inclusive_and_refusal_keeps_the_claim`. [Failure log](evidence/store/resume/delivery-recheck-crates__store__src__outbox.rs_line_127_col_30.log). |
| `crates/store/src/outbox.rs:127:30: replace > with >= in Store::recover_in_flight` | Caught by `tests::mutation_adr0008_recovery_audit_bound_is_inclusive_and_refusal_keeps_the_claim`. [Failure log](evidence/store/resume/delivery-recheck-crates__store__src__outbox.rs_line_127_col_30_002.log). |
| `crates/store/src/parked.rs:26:5: replace evidence -> Result<String, Error> with Ok("xyzzy".into())` | Caught by `tests::mutation_adr0003_park_resolve_and_audit_keep_exact_evidence_once_after_restart`. [Failure log](evidence/store/resume/parked-helper-recheck-crates__store__src__parked.rs_line_26_col_5_001.log). |
| `crates/store/src/parked.rs:26:5: replace evidence -> Result<String, Error> with Ok(String::new())` | Caught by `tests::mutation_adr0003_park_resolve_and_audit_keep_exact_evidence_once_after_restart`. [Failure log](evidence/store/resume/parked-helper-recheck-crates__store__src__parked.rs_line_26_col_5.log). |
| `crates/store/src/receipts.rs:21:24: replace != with == in Store::receive` | Caught by `tests::adr0003_receipt_digests_validate_raw_and_config_independently_without_writes`. [Failure log](evidence/store/resume/receipts-recheck-crates__store__src__receipts.rs_line_21_col_24.log). |
| `crates/store/src/receipts.rs:21:47: replace &#124;&#124; with && in Store::receive` | Caught by `tests::adr0003_receipt_digests_validate_raw_and_config_independently_without_writes`. [Failure log](evidence/store/resume/receipts-recheck-crates__store__src__receipts.rs_line_21_col_47.log). |
| `crates/store/src/receipts.rs:35:9: replace Store::raw -> Result<Option<Vec<u8>>, Error> with Ok(Some(vec![0]))` | Caught by `tests::adr0003_receipt_digests_validate_raw_and_config_independently_without_writes`. [Failure log](evidence/store/resume/receipts-raw-recheck-crates__store__src__receipts.rs_line_35_col_9_002.log). |
| `crates/store/src/resend.rs:105:13: replace &#124;&#124; with && in Store::resend_delivery` | Caught by `tests::mutation_adr0008_resend_requires_both_action_and_exact_delivery_target`. [Failure log](evidence/store/resume/delivery-recheck-crates__store__src__resend.rs_line_105_col_13.log). |
| `crates/store/src/resend.rs:143:34: delete ! in Store::resend_delivery` | Caught by `tests::plan11_adr0003_superseded_resend_requires_explicit_delivery`. [Failure log](evidence/store/resume/resend-superseded-recheck-crates__store__src__resend.rs_line_143_col_34.log). |
| `crates/store/src/resend.rs:40:9: replace Store::delivery -> Result<Option<StoredDelivery>, Error> with Ok(None)` | Caught by `tests::mutation_adr0008_recovery_audit_bound_is_inclusive_and_refusal_keeps_the_claim`, `tests::mutation_adr0008_resend_requires_both_action_and_exact_delivery_target`. [Failure log](evidence/store/resume/delivery-recheck-crates__store__src__resend.rs_line_40_col_9.log). |
| `crates/store/src/revisions.rs:156:41: replace != with == in Store::reprocess_with_lifecycle` | Caught by `tests::mutation_adr0003_adr0011_lifecycle_reprocessing_requires_audit_and_keeps_original`. [Failure log](evidence/store/resume/custody-first-recheck-crates__store__src__revisions.rs_line_156_col_41.log). |
| `crates/store/src/revisions.rs:183:17: replace &#124;&#124; with && in Store::reconcile` | Caught by `tests::mutation_adr0003_reconciliation_rejects_each_provenance_mismatch_without_interpretation`. [Failure log](evidence/store/resume/custody-first-recheck-crates__store__src__revisions.rs_line_183_col_17.log). |
| `crates/store/src/revisions.rs:184:17: replace &#124;&#124; with && in Store::reconcile` | Caught by `tests::adr0003_reconciliation_rejects_each_provenance_mismatch_without_interpretation`. [Failure log](evidence/store/resume/custody-first-recheck-crates__store__src__revisions.rs_line_184_col_17.log). |
| `crates/store/src/revisions.rs:192:39: replace &#124;&#124; with && in Store::reconcile` | Caught by `tests::adr0003_reconciliation_refuses_wrong_result_receipt_and_correction_count_atomically`, `tests::adr0003_rule7_reconciliation_bounds_results_and_findings_independently`. [Failure log](evidence/store/resume/custody-first-recheck-crates__store__src__revisions.rs_line_192_col_39.log). |
| `crates/store/src/revisions.rs:192:48: replace > with == in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_reconciliation_bounds_results_and_findings_independently`. [Failure log](evidence/store/resume/custody-second-recheck-crates__store__src__revisions.rs_line_192_col_48.log). |
| `crates/store/src/revisions.rs:192:48: replace > with >= in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_reconciliation_bounds_results_and_findings_independently`. [Failure log](evidence/store/resume/custody-second-recheck-crates__store__src__revisions.rs_line_192_col_48_002.log). |
| `crates/store/src/revisions.rs:196:27: replace > with == in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_reconciliation_bounds_results_and_findings_independently`. [Failure log](evidence/store/resume/custody-second-recheck-crates__store__src__revisions.rs_line_196_col_27.log). |
| `crates/store/src/revisions.rs:196:27: replace > with >= in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_reconciliation_bounds_results_and_findings_independently`. [Failure log](evidence/store/resume/custody-second-recheck-crates__store__src__revisions.rs_line_196_col_27_002.log). |
| `crates/store/src/revisions.rs:276:50: replace == with != in Store::reconcile` | Caught by `tests::mutation_adr0003_batch_duplicates_reuse_the_matching_revision_and_keep_runs_distinct`. [Failure log](evidence/store/resume/custody-second-recheck-crates__store__src__revisions.rs_line_276_col_50.log). |
| `crates/store/src/revisions.rs:354:26: replace > with == in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_354_col_26.log). |
| `crates/store/src/revisions.rs:354:26: replace > with >= in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_354_col_26_002.log). |
| `crates/store/src/revisions.rs:358:13: replace && with &#124;&#124; in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`, `tests::mutation_adr0003_reprocessing_never_resolves_failure_empty_unresolved_or_flagged_evidence`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_358_col_13.log). |
| `crates/store/src/revisions.rs:362:25: replace && with &#124;&#124; in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`, `tests::mutation_adr0003_reprocessing_never_resolves_failure_empty_unresolved_or_flagged_evidence`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_362_col_25.log). |
| `crates/store/src/revisions.rs:363:25: replace && with &#124;&#124; in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`, `tests::mutation_adr0003_reprocessing_never_resolves_failure_empty_unresolved_or_flagged_evidence`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_363_col_25.log). |
| `crates/store/src/revisions.rs:386:59: replace > with == in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_386_col_59.log). |
| `crates/store/src/revisions.rs:386:59: replace > with >= in Store::reconcile` | Caught by `tests::mutation_adr0003_rule7_interpretation_and_audit_byte_limits_are_inclusive`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_386_col_59_002.log). |
| `crates/store/src/revisions.rs:431:17: replace &#124;&#124; with && in Store::reconcile` | Caught by `tests::mutation_adr0003_delivery_bounds_and_machine_owner_refuse_atomically`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_431_col_17.log). |
| `crates/store/src/revisions.rs:431:43: replace > with == in Store::reconcile` | Caught by `tests::mutation_adr0003_delivery_bounds_and_machine_owner_refuse_atomically`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_431_col_43.log). |
| `crates/store/src/revisions.rs:431:43: replace > with >= in Store::reconcile` | Caught by `tests::mutation_adr0003_delivery_bounds_and_machine_owner_refuse_atomically`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_431_col_43_002.log). |
| `crates/store/src/revisions.rs:450:37: replace && with &#124;&#124; in Store::reconcile` | Caught by `tests::mutation_adr0003_reprocessing_never_resolves_failure_empty_unresolved_or_flagged_evidence`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_450_col_37.log). |
| `crates/store/src/revisions.rs:463:25: replace &#124;&#124; with && in Store::reconcile` | Caught by `tests::mutation_adr0003_reprocessing_never_resolves_failure_empty_unresolved_or_flagged_evidence`. [Failure log](evidence/store/resume/custody-third-recheck-crates__store__src__revisions.rs_line_463_col_25.log). |
| `crates/store/src/revisions.rs:520:35: replace > with < in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_refuses_conflicting_legacy_revision_and_accepts_repetition`, `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_35_001.log). |
| `crates/store/src/revisions.rs:520:35: replace > with == in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_35.log). |
| `crates/store/src/revisions.rs:520:35: replace > with >= in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_35_002.log). |
| `crates/store/src/revisions.rs:520:43: replace &#124;&#124; with && in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_43.log). |
| `crates/store/src/revisions.rs:520:61: replace > with < in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_refuses_conflicting_legacy_revision_and_accepts_repetition`, `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_61_001.log). |
| `crates/store/src/revisions.rs:520:61: replace > with == in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_61.log). |
| `crates/store/src/revisions.rs:520:61: replace > with >= in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_bounds_metadata_and_interpretation_independently_and_inclusively`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_520_col_61_002.log). |
| `crates/store/src/revisions.rs:534:51: replace match guard previous != digest.0 with false in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_refuses_conflicting_legacy_revision_and_accepts_repetition`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_534_col_51_001.log). |
| `crates/store/src/revisions.rs:534:51: replace match guard previous != digest.0 with true in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_refuses_conflicting_legacy_revision_and_accepts_repetition`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_534_col_51.log). |
| `crates/store/src/revisions.rs:534:60: replace != with == in Store::migrate_v3` | Caught by `tests::mutation_adr0003_migration_refuses_conflicting_legacy_revision_and_accepts_repetition`. [Failure log](evidence/store/resume/custody-migration-recheck-crates__store__src__revisions.rs_line_534_col_60.log). |
| `crates/store/src/revisions.rs:586:26: replace > with == in same_retained_content` | Caught by `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-recheck-crates__store__src__revisions.rs_line_586_col_26.log). |
| `crates/store/src/revisions.rs:586:26: replace > with >= in same_retained_content` | Caught by `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-recheck-crates__store__src__revisions.rs_line_586_col_26_002.log). |
| `crates/store/src/revisions/status.rs:104:33: replace == with != in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_104_col_33.log). |
| `crates/store/src/revisions/status.rs:105:13: replace && with &#124;&#124; in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_105_col_13.log). |
| `crates/store/src/revisions/status.rs:105:39: replace == with != in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_105_col_39.log). |
| `crates/store/src/revisions/status.rs:106:13: replace && with &#124;&#124; in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_106_col_13.log). |
| `crates/store/src/revisions/status.rs:106:74: replace != with == in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_106_col_74.log). |
| `crates/store/src/revisions/status.rs:124:19: replace > with < in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_124_col_19_001.log). |
| `crates/store/src/revisions/status.rs:124:19: replace > with == in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_124_col_19.log). |
| `crates/store/src/revisions/status.rs:124:19: replace > with >= in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_124_col_19_002.log). |
| `crates/store/src/revisions/status.rs:20:5: replace status -> Option<&str> with None` | Caught by `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`, `tests::adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice`, `tests::mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_20_col_5.log). |
| `crates/store/src/revisions/status.rs:20:5: replace status -> Option<&str> with Some("")` | Caught by `tests::mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status`, `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`, `tests::adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_20_col_5_001.log). |
| `crates/store/src/revisions/status.rs:20:5: replace status -> Option<&str> with Some("xyzzy")` | Caught by `tests::adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice`, `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`, `tests::mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_20_col_5_002.log). |
| `crates/store/src/revisions/status.rs:24:43: replace == with != in status` | Caught by `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`, `tests::adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice`, `tests::mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_24_col_43.log). |
| `crates/store/src/revisions/status.rs:29:9: replace StatusLifecycle<'_>::permits -> bool with false` | Caught by `tests::mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status`, `tests::adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice`, `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_29_col_9_001.log). |
| `crates/store/src/revisions/status.rs:29:9: replace StatusLifecycle<'_>::permits -> bool with true` | Caught by `tests::adr0011_latest_status_uses_exact_revision_when_one_batch_advances_twice`, `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`, `tests::mutation_adr0011_progression_uses_the_requested_observation_and_unanimous_source_status`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_29_col_9.log). |
| `crates/store/src/revisions/status.rs:36:9: replace StatusLifecycle<'_>::may_deliver -> bool with false` | Caught by `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_36_col_9_001.log). |
| `crates/store/src/revisions/status.rs:36:9: replace StatusLifecycle<'_>::may_deliver -> bool with true` | Caught by `tests::adr0011_withheld_delivery_builder_is_refused_atomically`, `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_36_col_9.log). |
| `crates/store/src/revisions/status.rs:40:25: delete ! in StatusLifecycle<'_>::may_deliver` | Caught by `tests::adr0011_withheld_delivery_builder_is_refused_atomically`, `tests::adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_40_col_25.log). |
| `crates/store/src/revisions/status.rs:43:9: replace StatusLifecycle<'_>::exclusive -> bool with false` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-recheck-crates__store__src__revisions__status.rs_line_43_col_9_001.log). |
| `crates/store/src/revisions/status.rs:60:22: replace > with == in latest` | Caught by `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-recheck-crates__store__src__revisions__status.rs_line_60_col_22.log). |
| `crates/store/src/revisions/status.rs:60:22: replace > with >= in latest` | Caught by `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-recheck-crates__store__src__revisions__status.rs_line_60_col_22_002.log). |
| `crates/store/src/revisions/status.rs:88:5: replace other_final_run -> Result<bool, Error> with Ok(false)` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_88_col_5_001.log). |
| `crates/store/src/revisions/status.rs:98:12: delete ! in other_final_run` | Caught by `tests::mutation_adr0011_single_final_run_is_scoped_by_machine_order_and_test`, `tests::mutation_adr0003_adr0011_retained_interpretation_byte_bound_is_inclusive`. [Failure log](evidence/store/resume/custody-status-final-crates__store__src__revisions__status.rs_line_98_col_12.log). |
| `crates/store/src/transfers.rs:141:31: replace match guard retained != evidence with false in Store::journal_frame` | Caught by `tests::adr0003_transfer_provenance_conflicts_preserve_original_frames_and_configuration`. [Failure log](evidence/store/resume/custody-first-recheck-crates__store__src__transfers.rs_line_141_col_31.log). |

### Exactly what remains

Only the first custody group plus outbox/feed/resend and parked are complete. The round remains draft. Resume in the requested custody-risk order.
The following current inventory has not received a complete run and triage. Counts are candidates, not results; early partial lib/alerts outcomes do not complete those modules.

| Remaining production file | Candidate mutations |
| --- | ---: |
| `alerts.rs` | 23 |
| `backup.rs` | 5 |
| `capacity.rs` | 10 |
| `lib.rs` | 26 |
| `machine_orders.rs` | 90 |
| `machine_orders/cancel.rs` | 1 |
| `machine_orders/eligibility.rs` | 8 |
| `machine_orders/items.rs` | 22 |
| `machine_orders/listing.rs` | 20 |
| `machine_orders/ownership.rs` | 16 |
| `machine_orders/reserve.rs` | 14 |
| `migration.rs` | 12 |
| `open.rs` | 20 |
| `polls.rs` | 20 |
| `query.rs` | 38 |
| `reprocess.rs` | 5 |
| `restore.rs` | 22 |
| `startup.rs` | 13 |
| `worker.rs` | 17 |

382 candidates remain in the current 566-mutation inventory. This is not the old 565-mutation baseline: main moved limit validation into lib.rs.
All machine_orders submodules, polls, backup/restore, standalone migration, open/startup, worker, alerts, capacity, query, reprocess and remaining lib rules still need completion.

### Source-claim audit for the completed subset

Clinical rules 1–4 / ADRs 0003 and 0010: receipt digest refusals leave no receipt; provenance conflicts retain original frames/config; tests reopen evidence and compare exact bytes. Store-only tests do not claim wire ACK timing.
Clinical rule 4 / ADR 0003 / plan sections 6–7: same-batch duplicates retain the matching revision, corrections advance once and distinct runs remain distinct; delivery refusal rolls interpretation/history/outbox back together.
ADR 0011: generic caller-owned status tables, unanimous part status, exact observation selection, single-final-run scope and retained-history bounds are tested. No analyzer status-code interpretation is invented by store tests.
Clinical rules 2–3 / plan section 11: failed/empty/unresolved/flagged reprocessing never resolves the original parked item and adds operator evidence; old raw/config/interpretations remain unchanged. Migration boundary/conflict cases retain original rows on refusal.
ADR 0008: retry/recovery and resend keep exact app bytes and delivery ID; audit boundaries and action/target binding are checked independently. This is app delivery, not authority to resend a machine order.
ADR 0004 / rule 9: incomplete, stale, future-clock and reset feeds remain untrusted; an authoritative empty replacement is distinct from lookup failure.
Rule 8 machine lifecycle audit and all lower-priority modules remain unfinished; no expanded claim is made for them.
New tests use synthetic data, temporary stores and no added wall-clock waits. Shared HANG_GUARD is retained. Existing tests and assertions were not skipped or weakened.

### Final continuation verification

Twenty tests were added in this continuation (23 test additions relative to current main, including the three from the first stop). The restore regression is now owned by main and is not counted as a new branch test.

After `git merge --signoff origin/main`, the final five commands all exited 0 on Windows with Rust 1.100.0:

| Check | Output |
| --- | --- |
| `cargo fmt --check` | Exit 0; no diagnostics. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Exit 0; dev profile finished successfully. |
| `cargo test --workspace` | Exit 0; store: 103 passed, 1 manual benchmark ignored. All other workspace test targets and doc tests completed successfully. |
| `cargo deny check` | Exit 0; advisories ok, bans ok, licenses ok, sources ok. Existing duplicate-dependency warnings remain. |
| `cargo check --manifest-path fuzz/Cargo.toml --bins` | Exit 0; dev profile finished successfully. Fuzz targets compiled, not executed. |

[Retained check outputs and exit codes](evidence/store/resume/checks/exits.json) and sibling logs preserve the evidence. No Linux or minimum-Rust run was performed locally; those remain CI work. No production code, dependency or plan claim changed in this PR.

The final fetch also merged main's ADR 0012 documentation update (#90) with sign-off. All five checks were rerun after that merge and again exited 0; the retained continuation check logs are those final runs.
