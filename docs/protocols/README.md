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

Each folder has the same four files ([ADR 0012](../adr/0012-uniform-layout-and-naming.md)).
See the [glossary](../glossary.md) for shared terms.

| File | Holds |
| --- | --- |
| `README.md` | What the protocol is, Analyte's role, source documents and editions, scope, evidence level |
| `decisions.md` | Each choice the standard leaves open or Analyte adds (role, strictness, options, readings of unclear text, defaults, safety departures): what, why, source and guarding test |
| `checklist.md` | Plain requirements, one or more tests per row |
| `versions.md` | Supported versions, the difference table with sources, and the rule for unlisted versions |

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
| ASTM | Reserve capacity before ENQ ACK; commit each frame before data ACK | [astm.rs](../../crates/steps/dialog/tests/astm.rs): `failed_or_unanswered_commit_never_acks` |
| HL7/MLLP | Commit complete raw block; interpret before AA (enhanced CA gates raw custody) | [hl7/mod.rs](../../crates/steps/dialog/tests/hl7/mod.rs): `aa_only_after_store_and_interpret`; [dialog](../../crates/steps/dialog/tests/hl7/mod.rs) |
| LAW LAB-29 | Retain raw result and interpretation before AA | [results/custody.rs](../../crates/analyte/tests/law/results/custody.rs): `raw_commit_precedes_interpretation_and_ack` |
| POCT | Complete raw receipt and semantic outcome before ACK | [chunk_receive/custody.rs](../../crates/analyte/tests/chunk_receive/custody.rs): `poct_stores_before_ack_and_parks_unresolved` |
| VIDAS Literal | Retain complete transfer before data ACK | [chunk_receive/custody.rs](../../crates/analyte/tests/chunk_receive/custody.rs): `vidas_stores_before_ack_and_parks_unresolved` |
| Autolyser | Retain and validate poll response before destructive R; preserve uncertain R for reconciliation | [chunk_receive/custody.rs](../../crates/analyte/tests/chunk_receive/custody.rs): `autolyser_corrupt_reply_blocks_r_until_valid_l` |

### Startup recovery

Before any machine connection opens, one sweep finishes work a stop cut off.
An ASTM transfer still open is closed as incomplete and parked, keeping every
acknowledged frame. A receipt without an interpretation is interpreted with its
retained configuration, or parked for audited reprocessing when that
configuration changed. POCT, VIDAS and Autolyser are skipped: their input chunks
are receipts that are never interpreted, and they acknowledge a message only
after interpreting it, so the machine still holds anything unfinished. Answered
queries, order responses and display acknowledgments are recorded as handled,
so the sweep leaves them alone. Test:
`killed_process_parks_transfer_and_interprets_receipt`.


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
store error. Protocol reply timers can impose an earlier refusal; the HL7 host
still writes that refusal before it ends the session. The TCP read-idle limit
uses the same `operation_timeout_ms` but counts only while the host waits for the
machine, so a slow commit never closes the socket before its reply.

Tests: `store_budget_refuses_uncertain_custody`,
`store_reports_only_completed_custody`,
`stalled_store_commit_reaches_machine_as_refusal` and
`busy_host_keeps_connection_until_its_reply`.
