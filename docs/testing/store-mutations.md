# Store mutation triage

**Stopped on a production bug; this round is incomplete.** Baseline:
`bdb6b6f7af375d5679db462af0fe070f3297bc1f` (origin/main). No production code changed.

## Blocking bug

Plan section 11 / clinical rule 7: `Store::restore` accepts `Limits { page_rows: 0,
.. }`, although `Store::open` rejects the same limits. Restore calls `open_locked`
without the validation in `open_starting`, then publishes the staged database.
The zero limit violates the public opening invariant. No downstream listing/recovery
failure is claimed: restore drops its internal store before returning.
The reproducer confirms acceptance/publication; it does not claim observed patient loss.

`plan11_rule7_restore_refuses_zero_page_limit_before_publishing` fails on unchanged
production code: expected `InvalidInput`, got `Ok(...before-restore)`.
[Failure output](evidence/store/restore-zero-page-limit.log). This failing test is
intentionally retained as requested, not ignored or loosened. The PR stays draft.

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
