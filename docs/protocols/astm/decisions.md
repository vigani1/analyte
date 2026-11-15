# ASTM design decisions

These entries retain the declared scope and conformance choices. An unchecked
checklist item is a requirement or open decision, not a claim of implementation.
Source references retained in the checklist have not been independently reverified
for this migration; those entries say **source**: not found here.

## 1. Scope

- **what**: **Edition:** LIS01-A2 (2008) and LIS02-A2 (2004, printed as LIS2-A2).
  Both are archived by CLSI but still the current editions.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 2. Scope

- **what**: **Frames:** strict mode follows LIS01-A2: up to 64,000 characters per
  frame including 7 bytes of overhead (63,993 of text). Smaller vendor
  limits (for example 247) are machine config.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 3. Scope

- **what**: **Units of data:** a link session (ENQ to EOT) carries one or more H…L
  messages; a message is made of records; a long record is split across
  frames. Strict mode never packs two records in one frame; tolerating that
  is a vendor compatibility option.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 4. Scope

- **what**: **TCP:** strict LIS01 over TCP keeps the full handshake, checksums and
  timers, with the computer as server by default. Unframed LIS02 records are supported only through `watched_folder`;
  direct unframed LIS02 over TCP is rejected by config. Reversed client/server
  roles may be selected explicitly. TCP packet boundaries never frame ASTM.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 5. Scope

- **what**: **Queries:** either side may query; Analyte supports the search criteria
  it lists here and declares the rest unsupported.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 6. Scope

- **what**: **Content:** the record hierarchy, sequence numbers, all result status
  codes and the processing ID (production, training, debug, QC) are kept;
  training and debug messages never reach production delivery.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 7. Conformance choice

- **what**: M — §1/§3.3 p.1: distinguish session, record/message and frame.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 8. Conformance choice

- **what**: C — §4.3 p.2: agree encoding out of band; preserve raw bytes before decoding.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 9. Conformance choice

- **what**: O — §4.4.1 p.2: restrict large frames preferentially to TCP.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 10. Conformance choice

- **what**: C — §4.5 pp.2–3: document physical adaptations such as DB9/USB separately.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 11. Conformance choice

- **what**: M/hardware — §§5.1–5.2.1 p.3: qualify point-to-point EIA-232 signaling.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 12. Conformance choice

- **what**: M — §5.2.2 pp.3–4: support/default 8N1; use serial API for prescribed bit order and marking idle.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 13. Conformance choice

- **what**: O — §5.2.2.5 p.3: alternative data/parity/stop-bit settings are optional.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 14. Conformance choice

- **what**: M — §5.2.3.1 p.4: host supports 1200/2400/4800/9600; instrument at least one; prefer/default 9600 when multiple rates.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 15. Conformance choice

- **what**: O — §5.2.3.2 p.4: other rates optional.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 16. Conformance choice

- **what**: C/hardware — §§5.2.4–5.3 pp.4–5: qualify DB25 roles, signal contacts, shield grounding, connectors, locks and cable installation; explicit vendor adaptations.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 17. Conformance choice

- **what**: C/hardware — §§7.1–7.3 pp.10–12: qualify IEEE 802.3 network/cabling; do not apply serial-like start/stop wording in this section as application TCP bits.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 18. Conformance choice

- **what**: M — §8.2.1.1 p.13: strict profile computer server/instrument client; configurable destination address/port.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 19. Conformance choice

- **what**: M — §6.1 pp.5–6 / §8.1 pp.12–13: neutral/establish/transfer/terminate; simplex stop-and-wait with alternate supervision.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_1_to_7_neutral_readiness_and_busy`.

## 20. Conformance choice

- **what**: M — §6.2.1–.5 p.6 / §8.2.1–.5 p.13: initiate ENQ from neutral with data; ACK readiness before data; ignore establishment responses except ACK/NAK/ENQ; idle receiver ignores irrelevant bytes.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_1_to_7_neutral_readiness_and_busy`.

## 21. Conformance choice

- **what**: M — §6.2.6–.7 p.6 / §8.2.6–.7 p.13: busy NAK, ≥10 s retry ENQ delay; receive-incapable NAKs, send-incapable never initiates.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_1_to_7_neutral_readiness_and_busy`.

## 22. Conformance choice

- **what**: M — §6.2.7.1 p.6 / §8.2.7.1 pp.13–14: instrument priority, computer yields until new ENQ, instrument repeats after ≥1 s.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_7_1_6_5_2_2_contention_both_roles`.

## 23. Conformance choice

- **what**: M — §6.3.1 p.7 / §8.3.1 p.14: ≤64,000 total, ≤63,993 text; split longer records, never combine separate records in strict frames.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_1_6_3_2_6_3_3_6_6_large_records_rollover_separate_records`.

## 24. Conformance choice

- **what**: M — §6.3.1.2 p.7 / §8.3.1.2 p.14: STX/FN/text/ETB-or-ETX/checksum/CR/LF; intermediate versus final segmentation.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_1_6_3_2_6_3_3_6_6_large_records_rollover_separate_records`.

## 25. Conformance choice

- **what**: M — §6.3.2 p.7 / §8.3.2 p.14: ASCII 0–7, first 1, increment modulo eight only for new frame; same number on retransmission.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `adr0003_lis01_6_3_2_6_5_1_commit_duplicate_and_conflict`.

## 26. Conformance choice

- **what**: M — §6.3.3 pp.7–8 / §8.3.3 p.15: raw-byte sum modulo 256, frame digit through terminator; exclude STX/checksum/trailer; two uppercase hex digits, high first.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_3_6_5_1_6_6_reject_checksum_sequence_and_controls`.

## 27. Conformance choice

- **what**: M — §6.3.4 p.8 / §8.3.4 p.15: wait per frame; ACK advances, NAK repeats, EOT response accepts and requests interruption.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_4_6_4_6_5_1_2_sender_six_failures_and_eot`.

## 28. Conformance choice

- **what**: M — §6.3.5 pp.8–9 / §8.3.5 pp.15–16: interpret interrupt; ignoring requires repeat request to remain active; honoring terminates and yields for ≥15 s or peer send-and-release.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_5_interrupt_honor_ignore_and_issue`.

## 29. Conformance choice

- **what**: O — same: issuing/honoring an interrupt optional; positive acknowledgment meaning mandatory.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_5_interrupt_honor_ignore_and_issue`.

## 30. Conformance choice

- **what**: M — §6.4 p.9 / §8.4 p.16: sender EOT returns both neutral; no EOT ACK exchange.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_4_6_4_6_5_1_2_sender_six_failures_and_eot`.

## 31. Conformance choice

- **what**: M — §6.5.1 p.9 / §8.5.1 p.16: reject character/checksum/sequence defects; accept prior or next modulo-eight number; irrelevant outside-frame bytes cannot pollute checksum.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_3_6_5_1_6_6_reject_checksum_sequence_and_controls`.

## 32. Conformance choice

- **what**: M — §6.5.1.2 p.9 / §8.5.1.2 p.16: unexpected reply except ACK/EOT is NAK condition; abort at six unaccepted sends of one frame.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_4_6_4_6_5_1_2_sender_six_failures_and_eot`.

## 33. Conformance choice

- **what**: M — §6.5.2.1 p.9 / §8.5.2.1 p.16: 15 s ENQ response timeout → termination.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_6_6_5_2_1_3_busy_and_reply_timeouts`.

## 34. Conformance choice

- **what**: M — §6.5.2.2 p.9 / §8.5.2.2 p.16: computer contention 20 s without new ENQ → neutral.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_7_1_6_5_2_2_contention_both_roles`.

## 35. Conformance choice

- **what**: M — §6.5.2.3 p.9 / §8.5.2.3 p.17: 15 s after last frame character without response → terminate; remember failed content for repetition.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_6_6_5_2_1_3_busy_and_reply_timeouts`.

## 36. Conformance choice

- **what**: M — §6.5.2.4 p.10 / §8.5.2.4 p.17: receive timer starts at transfer entry/each reply; 30 s without frame/EOT → discard incomplete assembly, neutral.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_1_2_6_5_2_4_etb_timeout_and_split_merge`, `lis01_6_5_2_4_8_5_2_4_nak_at_29_expires_at_59`.

## 37. Conformance choice

- **what**: M — §6.5.2.5 p.10 / §8.5.2.5 p.17: reply within 15 s including processing/busy delay.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack`.

## 38. Conformance choice

- **what**: O — §6.5.2.6 p.10 / §8.5.2.6 p.17: more than normal one outgoing-message buffer optional.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_5_2_6_one_message_buffer_and_validated_vendor_values`.

## 39. Conformance choice

- **what**: M — §6.6 p.10 / §8.6 p.17: exclude SOH/STX/ETX/EOT/ENQ/ACK/DLE/NAK/SYN/ETB/LF/DC1–DC4 from payload; distinguish record CR from trailer.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_3_6_5_1_6_6_reject_checksum_sequence_and_controls`.

## 40. Conformance choice

- **what**: M — Appendix A p.19: reconcile every state/arrow/timer visually before signoff; visually inspected, with transition coverage.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_7_1_6_5_2_2_contention_both_roles`.

## 41. Conformance choice

- **what**: M — §4.1 pp.3–4: H/L level zero, P/Q/S level one, O two, R three; O needs P, R needs O; same/higher level closes prior scope.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_4_1_parents_and_context_closure`.

## 42. Conformance choice

- **what**: M — §4.1 p.3, §5.5.5/.8 pp.8–9: C/M contextual attachment, variable level, never after L.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_4_1_parents_and_context_closure`.

## 43. Conformance choice

- **what**: M — §4.2.1–.2 pp.4–5: save on upward hierarchy transition; repeat ancestors needed to restart last unsaved logical record.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 44. Conformance choice

- **what**: M — §5.1 p.5: specified Latin-1 repertoire and allowed controls, excluded values; scalar data excludes active separators; case-sensitive content except record IDs.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_5_1_latin1_is_not_utf8_or_lossy`.

## 45. Conformance choice

- **what**: C — §§5.2–5.3 pp.5–6: no standard maximum field/record; document agreed implementation limits and never silently truncate.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_5_2_5_3_limits_reject_instead_of_truncating`.

## 46. Conformance choice

- **what**: M — §§5.4.1–.4 p.6,5.4.6 p.7,6.2 p.11: CR termination, distinct header-selected field/repeat/component/escape, consistent until L.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_6_2_bootstrap_and_5_4_6_message_delimiters`.

## 47. Conformance choice

- **what**: M — §5.4.5 pp.6–7: escape-aware parsing, no nesting.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_5_4_5_escapes_never_change_boundaries`.

## 48. Conformance choice

- **what**: O/C — §5.4.5.1 p.7: preserve/support optional H/N, F/S/R/E, variable-length X hex and Z local operations without changing boundaries.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 49. Conformance choice

- **what**: M — §§5.4.7–.9 pp.7–8,5.7 p.11: positional internal empties; tolerate omitted/present trailing null delimiters; null versus explicit two-quote deletion.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_5_4_7_through_9_null_positions_and_deletion`.

## 50. Conformance choice

- **what**: C — §5.4.8 p.7: semantically unused fields may be ignored, positions still preserved.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 51. Conformance choice

- **what**: M — §5.5 p.8: type first, record IDs case-insensitive, CR end.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 52. Conformance choice

- **what**: C — §5.6.1 pp.9–10: four positional test-ID components with documented local extensions; sufficient identity for battery/specimen association.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 53. Conformance choice

- **what**: C — §5.6.2 p.10: YYYYMMDD, HHMMSS, combined up to fourteen digits, optional signed timezone; absent zone is sender's, not UTC.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 54. Conformance choice

- **what**: C — §5.6.3 p.10: phones free text, repeats allowed, first primary/daytime.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 55. Conformance choice

- **what**: C — §5.6.4 p.10: field-specific fixed-observation default units; alternate units in components, distinct from absent result-unit guessing.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 56. Conformance choice

- **what**: C — §§5.6.5–.6 pp.10–11: address/provider/name positions and repeated identities retained.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 57. Conformance choice

- **what**: M — §5.6.7 p.11: record sequence required for repeatable types, resets on higher context/changed level; independent of frame number.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_5_6_7_sequence_numbers_reset_with_context`.

## 58. Conformance choice

- **what**: M — §6 pp.11–13: H first; L before another H/session end; bootstrap header delimiters specially.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 59. Conformance choice

- **what**: C — §6.3 p.11: control ID identifies transmission, not persistent observation.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 60. Conformance choice

- **what**: C — §6.4 p.12: agreed password failure aborts and notifies access violation.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_3_4_6_4_6_5_1_2_sender_six_failures_and_eot`.

## 61. Conformance choice

- **what**: C — §§6.5–.11 p.12: sender/receiver, sender details, reserved position and instructions retained.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 62. Conformance choice

- **what**: C — §6.12 p.12: P/T/D/Q processing; training has no production effect.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_6_12_and_12_3_processing_and_termination_codes`.

## 63. Conformance choice

- **what**: C — §§6.13–.14 p.13: declared LIS2-A2 version and transmission time, vendor substitutions explicit.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis01_6_2_1_to_7_neutral_readiness_and_busy`, `lis02_section11_cancel_is_distinct_and_unsupported_filters_are_not_empty_orders`, `lis02_6_14_p13_generated_header_time`.

## 64. Conformance choice

- **what**: C — §§7.1–.35 pp.13–17: distinct patient IDs and all defined demographics retained where used; never require every optional demographic.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 65. Conformance choice

- **what**: C — §§7.8–.10 pp.13–14: preserve date precision and specified sex/race coding without inference.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 66. Conformance choice

- **what**: C — §§7.17–.18 pp.14–15: height/weight defaults and explicit alternate units.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 67. Conformance choice

- **what**: C — §§7.24–.35 pp.15–17: admission/discharge and coded clinical/administrative context distinct from specimen overrides.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 68. Conformance choice

- **what**: C — §§8.1–.3 pp.17–18: multiple tests share O only if all other attributes same; repeated specimen IDs allowed; reporting uses separate O per battery and R per observation.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 69. Conformance choice

- **what**: C — §§8.2–.3 pp.17–18: not-done O with X and no R is meaningful, not empty noise.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 70. Conformance choice

- **what**: C — §§8.4.1–.5 pp.18–19: O type/sequence, specimen ID distinct from instrument specimen ID and complete test identification.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 71. Conformance choice

- **what**: C — §8.4.6 p.19: S/A/R/C/P priorities; multiple priorities repeat-separated.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 72. Conformance choice

- **what**: C — §§8.4.7–.11 p.19: distinguish ordered/future requested time, actual collection/start/end, volume and collector.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 73. Conformance choice

- **what**: C — §8.4.12 p.20: cancel/add/new/pending/in-process/QC distinct, L reserved.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 74. Conformance choice

- **what**: C — §§8.4.13–.18 p.20: hazards, clinical context, receipt time, type/source and ordering clinician.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 75. Conformance choice

- **what**: C — §§8.4.19–.22 p.21: requester user fields roundtrip; lab fields retained.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 76. Conformance choice

- **what**: C — §§8.4.23–.25 p.21: report-change time, charge reference and performing section distinct.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 77. Conformance choice

- **what**: C — §8.4.26 p.21: O/C/P/F/X/I/Y/Z/Q; distinguish no order/no patient/query response.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 78. Conformance choice

- **what**: C — §§8.4.27–.31 pp.21–22: reserved position, collection location, infection flag, specimen service/institution.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 79. Conformance choice

- **what**: C — §§9.1–.4 p.22: analytic determinations separately identified; raw numeric/text/code values preserved; no implied independent subvalues.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 80. Conformance choice

- **what**: C — §9.5 p.23: retain units, standard abbreviations where available; not necessarily UCUM.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 81. Conformance choice

- **what**: C — §9.6 p.23: repeated ranges with component descriptions retained.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 82. Conformance choice

- **what**: C — §§9.7–.8 p.23: flags including LL/HH, combined A/S/R population basis or generic N.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 83. Conformance choice

- **what**: C — §9.9 pp.23–24: C/P/F/X/I/S/M/R/N/Q/V/W status meanings distinct.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `lis02_9_9_all_result_codes_remain_distinct`.

## 84. Conformance choice

- **what**: C — §9.10 p.24: normative/units change date empty if irrelevant; relevant changes trigger review rather than silent historical comparability.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 85. Conformance choice

- **what**: C — §§9.11–.14 p.24: operator/verifier, started/completed and instrument identities distinct.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 86. Conformance choice

- **what**: C — §§10.1–.5 pp.24–25: comment sequence/source/type and coded/text components with correct parent.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 87. Conformance choice

- **what**: O/C — §§13.1–.21 pp.28–29: scientific methods/instrument/reagent/QC/specimen/result/patient context preserved, not clinically interpreted.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 88. Conformance choice

- **what**: O/C — §14 p.30: M plus sequence and documented manufacturer-defined remainder; preserve unknown structures.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 89. Conformance choice

- **what**: M — §11 p.25: either side queries; one outstanding request; L completion or cancellation before next.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 90. Conformance choice

- **what**: C — §§11.1–.4 pp.25–26: Q sequence, positional patient/specimen/vendor ID components, repeated IDs, range end and direction-sensitive ALL.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 91. Conformance choice

- **what**: O — §11.3 p.26: full historical search/retention not mandated; declare practical retrieval subset.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 92. Conformance choice

- **what**: C — §11.5 p.26: repeated test IDs/ALL within other selection constraints.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 93. Conformance choice

- **what**: C — §§11.6–.8 p.26: S collection/R result basis, default R; beginning/repeated dates, open past and latest bound.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 94. Conformance choice

- **what**: C — §§11.9–.12 pp.26–27: requesting physician/phone/user fields.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 95. Conformance choice

- **what**: C — §11.13 p.27: result-state filters, pending/partial/new-only, MIC/previously sent; A cancel, O orders/demographics, D demographics only.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 96. Conformance choice

- **what**: M — §§12.1–.2 p.27: L last, sequence normally 1, next H a new message.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 97. Conformance choice

- **what**: C — §12.3 pp.27–28: normal/sender-abort/receiver-abort/system-error/query-error/no-information/finished distinct; I/Q terminate request.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 98. Conformance choice

- **what**: Frame commit before data ACK, capacity reservation before ENQ ACK, disk-full/commit failure refusal.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack`, `lis01_6_3_adr0003_astm_journal_precedes_ack_and_message_is_interpreted`, `adr0003_full_disk_refuses_astm_enq_and_shutdown_is_bounded`.

## 99. Conformance choice

- **what**: Crash after write/before ACK, repeated previous frame, same-number conflicting payload, incomplete ETB record and repeated hierarchy ancestors.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 100. Conformance choice

- **what**: Split/merge every byte boundary; high-bit checksum before character decode; rollover and separate frame/record/message/session boundaries.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 101. Conformance choice

- **what**: Fixed bounds for trickles, frames, records and sessions, while retaining raw evidence.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 102. Conformance choice

- **what**: Application deadline versus link timer, late reply/cancel/cache uncertainty and no unintended worklist resend.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 103. Conformance choice

- **what**: No payload/credential logs, even trace.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

