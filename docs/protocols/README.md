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

## Durable custody boundaries

[ADR 0003](../adr/0003-clinical-safety-rules.md) and [ADR 0010](../adr/0010-acknowledgment-boundaries.md)
decide these boundaries; this table names the test that guards each one.

| Protocol | Before positive acknowledgment or destructive command | Test evidence |
| --- | --- | --- |
| ASTM | Reserve capacity before ENQ ACK; commit each frame before data ACK | [link.rs](../../crates/steps/dialog/tests/link.rs): `adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack` |
| HL7/MLLP | Commit complete raw block; interpret before AA (enhanced CA gates raw custody) | [hl7.rs](../../crates/steps/dialog/tests/hl7.rs): `hl7_251_2_9_2_aa_only_after_store_and_interpret`; [dialog](../../crates/steps/dialog/tests/hl7.rs) |
| LAW LAB-29 | Retain raw result and interpretation before AA | [results.rs](../../crates/analyte/tests/law/results.rs): `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack` |
| POCT | Complete raw receipt and semantic outcome before ACK | [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs): `poct01_b3_2_adr0003_stored_before_ack_unresolved_parked_no_delivery` |
| VIDAS Literal | Retain complete transfer before data ACK | [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs): `vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery` |
| Autolyser | Retain and validate poll response before destructive R; preserve uncertain R for reconciliation | [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs): `autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l` |
