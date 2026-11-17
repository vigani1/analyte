# Protocol notes

Our own notes on each standard, written in our own words. Each folder holds
the scope Analyte supports and a conformance checklist that becomes the test
list for that standard's phase ([plan §13](../plan.md)). Standards documents
themselves are never committed.

| Protocol | Notes | Phase |
| --- | --- | --- |
| ASTM LIS01-A2 + LIS02-A2 | [astm](astm/README.md) | 2 |
| HL7 v2.3.1 + v2.5.1, MLLP | [hl7](hl7/README.md) | 3 |
| IHE LAW (PaLM Rev 11.0) | [law](law/README.md) | 4 |
| POCT01-A2 | [poct](poct/README.md) | 5 |
| VIDAS PC Literal | [vidas](vidas/README.md) | 6 |
| Autolyser internal | [autolyser](autolyser/README.md) | 6 |

Each folder contains `README.md`, `decisions.md`, `checklist.md` and
`versions.md`. See the [glossary](../glossary.md) for shared terms.

## Checklist test references

Every backticked test name in a `checklist.md` is a test reference, and
`scripts/check-protocol-tests.py` fails CI when the test does not exist. Future
work is written as `Planned test: ` followed by one backticked name, once per
name; only that name is skipped, and it must not name an existing test.
A referenced test must run in CI: an `#[ignore]` test backs a row only when
written as `Manual test: ` followed by its name, and then it must be ignored.

## Durable custody boundaries

[ADR 0003](../adr/0003-clinical-safety-rules.md) and [ADR 0010](../adr/0010-acknowledgment-boundaries.md)
decide these boundaries; this table names the test that guards each one.

| Protocol | Before positive acknowledgment or destructive command | Test evidence |
| --- | --- | --- |
| ASTM | Reserve capacity before ENQ ACK; commit each frame before data ACK | [astm.rs](../../crates/steps/dialog/tests/astm.rs): `adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack` |
| HL7/MLLP | Commit complete raw block; interpret before AA (enhanced CA gates raw custody) | [hl7/mod.rs](../../crates/steps/dialog/tests/hl7/mod.rs): `hl7_251_2_9_2_aa_only_after_store_and_interpret`; [dialog](../../crates/steps/dialog/tests/hl7/mod.rs) |
| LAW LAB-29 | Retain raw result and interpretation before AA | [results.rs](../../crates/analyte/tests/law/results.rs): `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack` |
| POCT | Complete raw receipt and semantic outcome before ACK | [chunk_receive/custody.rs](../../crates/analyte/tests/chunk_receive/custody.rs): `poct01_b3_2_adr0003_stored_before_ack_unresolved_parked_no_delivery` |
| VIDAS Literal | Retain complete transfer before data ACK | [chunk_receive/custody.rs](../../crates/analyte/tests/chunk_receive/custody.rs): `vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery` |
| Autolyser | Retain and validate poll response before destructive R; preserve uncertain R for reconciliation | [chunk_receive/custody.rs](../../crates/analyte/tests/chunk_receive/custody.rs): `autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l` |


### Host custody deadline

All six stream drivers bound each store-before-positive-effect operation with
`machine::custody::store_within`, using the validated transport
`operation_timeout_ms`, capped by VIDAS `commit_ms` or Autolyser `storage_ms`
where configured. The earliest configured limit wins. The budget covers the complete operation, including
interpretation where the protocol requires it before acknowledgment. ASTM uses
that same budget for its dialog commit timer. A timeout means custody is
uncertain, never permission to acknowledge or repeat an order. A submitted store
request may still commit after the caller stops waiting; retained identities and
disconnect/restart recovery own that outcome. Store queue requests retain their
own defensive deadline; the complete operation can finish earlier with a specific
store error. Protocol reply timers can impose an earlier refusal.

Tests: `adr0010_store_budget_refuses_uncertain_custody` and
`adr0010_store_reports_only_completed_custody_and_keeps_failure`.
