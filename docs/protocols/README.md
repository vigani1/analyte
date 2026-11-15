# Protocol notes

Our own notes on each standard, written in our own words. Each file holds
the scope Analyte supports and a conformance checklist that becomes the test
list for that standard's phase ([plan §13](../plan.md)). Standards documents
themselves are never committed.

| Standard | File | Phase |
| --- | --- | --- |
| ASTM LIS01-A2 + LIS02-A2 | [astm.md](astm.md) | 2 |
| HL7 v2.3.1 + v2.5.1, MLLP | [hl7.md](hl7.md) | 3 |
| IHE LAW (PaLM Rev 11.0) | [ihe-law.md](ihe-law.md) | 4 |
| POCT01-A2 | [poct1-a.md](poct1-a.md) | 5 |

## Durable custody boundaries

ADR 0003 specifies ASTM and HL7. The other runtime boundaries and their tests
are listed here; extending that ADR is an owner decision.

| Protocol | Before positive acknowledgment or destructive command | Test evidence |
| --- | --- | --- |
| ASTM | Reserve capacity before ENQ ACK; commit each frame before data ACK | [link.rs](../../crates/dialog/tests/link.rs): `adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack` |
| HL7/MLLP | Commit complete raw block; interpret before AA (enhanced CA gates raw custody) | [receive.rs](../../crates/analyte/tests/receive.rs): `adr0003_hl7_aa_only_after_interpretation_and_ae_for_mapping_findings`; [dialog](../../crates/dialog/tests/hl7.rs) |
| LAW LAB-29 | Retain raw result and interpretation before AA | [results.rs](../../crates/analyte/tests/law/results.rs): `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack` |
| POCT | Complete raw receipt and semantic outcome before ACK | [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs): `poct01_b3_2_adr0003_stored_before_ack_unresolved_parked_no_delivery` |
| VIDAS Literal | Retain complete transfer before data ACK | [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs): `vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery` |
| Autolyser | Retain and validate poll response before destructive R; preserve uncertain R for reconciliation | [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs): `autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l` |
