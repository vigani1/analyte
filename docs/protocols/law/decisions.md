# LAW design decisions

These entries retain the declared scope and conformance choices. An unchecked
checklist item is a requirement or open decision, not a claim of implementation.
Source references retained in the checklist have not been independently reverified
for this migration; those entries say **source**: not found here.

## 1. Scope

- **what**: **Actor:** Analyte implements the **Analyzer Manager** (it receives
  results and distributes work the lab app authorized). Analyzer behavior
  lives only in the test peer. Being the manager does not make Analyte
  take clinical decisions: reruns and reflexes are only sent when the app
  authorized them.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 2. Scope

- **what**: **Sources:** PaLM TF Vol 1 and 2b, the PaLM TF-2x volume (§2.2.4), the IHE
  Technical Frameworks General Introduction (Appendix E) and HL7 v2.5.1
  chapters 2, 2A, 3, 4, 7 and 13. Vol 2a and Vol 3 are not needed for LAW
  alone.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 3. Scope

- **what**: **Done:** every manager requirement and selected option in this list has
  tests, plus a recorded Gazelle run (tool build, validation profile
  revision, actor, options, scenarios). A simulator pass alone is not
  completeness.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 4. Conformance choice

- **what**: **R** Support LAB-27 responder, LAB-28 initiator and LAB-29 responder; preserve unique AWOS and work linkage (TF-1 §§5.1–5.1.1, pp.33–34; §5.4.1, pp.41–42).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 5. Conformance choice

- **what**: **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 6. Conformance choice

- **what**: **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 7. Conformance choice

- **what**: **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 8. Conformance choice

- **what**: **S** Send enhanced data only to supporting peers (TF-1 §5.1.1.2, p.34).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 9. Conformance choice

- **what**: **R** Use directional MLLP initiating connections; tolerate peer closure and independent establishment (TF-2b B.6, pp.79-80; TF-2x ?2.2.4 dependency).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 10. Conformance choice

- **what**: **R** Use NE/AL application acknowledgement mode and correct header reversal/MSA-2 correlation; do not reject incoming application/facility names merely for differing from configured values (TF-2b B.5, pp.78–79; HL7 §2.9, pp.2-24–2-28, §2.15.8.2).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 11. Conformance choice

- **what**: **R** Parse QBP and return correctly correlated RSP, distinguishing acceptance/error from actual work availability (TF-2b §3.27.4, pp.34–44).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 12. Conformance choice

- **what**: **R/C** Handle container, isolate, rack/position, tray/position, all-pending and informational location forms as applicable to connected peers (TF-1 §§5.2.1–5.2.5, pp.36–37; TF-2b QPD definition, pp.35–41).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 13. Conformance choice

- **what**: **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 14. Conformance choice

- **what**: **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 15. Conformance choice

- **what**: **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 16. Conformance choice

- **what**: **R** Preserve uncertain prior acceptance; duplicate-AWOS UA must not undo the original accepted request (TF-2b §3.28.4.1.3, p.51).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 17. Conformance choice

- **what**: **R** Use separate AWOS for separately requested replicates (TF-2b B.8, p.83).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 18. Conformance choice

- **what**: **R** Receive OUL and return ACK with required structures/fields; correlate known AWOS to originating work (TF-2b §3.29.4, pp.53–58).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 19. Conformance choice

- **what**: **R, host boundary** Store the exact LAB-29 MLLP block before interpretation/ACK; AA only after durable interpretation, with no partial batch delivery on a conflict (TF-2b §3.29.4; B.7; ADR 0003).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 20. Conformance choice

- **what**: **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 21. Conformance choice

- **what**: **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 22. Conformance choice

- **what**: **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 23. Conformance choice

- **what**: **R** Preserve supplemental/primary relationships, typed values, units, statuses and scoped notes; do not treat NTE prose as executable rules (TF-2b B.3; C.5/C.7, pp.95–113).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 24. Conformance choice

- **what**: **R** Implement every applicable manager usage for ERR, INV, MSA, MSH, NTE, OBR, OBX, ORC, PID, PV1, SAC, SPM, TCD and TQ1, including component usages and conditional predicates (TF-2b Appendix C, pp.84–141).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 25. Conformance choice

- **what**: **R** Apply coding-system identification, test/result vocabulary and units rules without inventing codes (TF-2b §2.3, pp.24–31).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 26. Conformance choice

- **what**: **R** Distinguish malformed-trigger AE, control-content AR, invalid ACK failure, inconsistent ORL failure and query processing rejection; report locations where required (TF-2b B.7.1–B.7.8, pp.80–82).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 27. Conformance choice

- **what**: **R** Reject inconsistent LAB-29 as a whole, including TCD-1/OBX-3 mismatch; retain bytes without applying a subset (TF-2b p.57; B.7.6, p.82).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 28. Conformance choice

- **what**: **R** Keep manager-sourced patient data authoritative; correct downloaded patient information through cancel/new-AWOS, not repeated work (TF-2b B.7.9, pp.82–83).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 29. Conformance choice

- **what**: **R** Handle unknown/non-HL7 input without compromising the application (TF-2b B.7.10, p.83).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 30. Conformance choice

- **what**: **R/C** Preserve contributing substance/disposable identifiers, types and available lot/container data (TF-1 §5.2.6, p.37; TF-2b §2.2.4.3, p.17; C.2).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 31. Conformance choice

- **what**: **R/C** Send explicit requested dilution and retain actual dilution/value meaning (TF-1 §5.2.7, pp.37–38; TF-2b §2.2.4.4, pp.17–18; C.13).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 32. Conformance choice

- **what**: **R/C** Handle profiled patient/encounter demographics without invented values (TF-1 §5.2.8, p.38; TF-2b §2.2.4.5, pp.18–19).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 33. Conformance choice

- **what**: **R/C** Preserve Analyzer reflex parent/child relationships (TF-1 §5.2.9, p.38; TF-2b §2.2.4.6, p.19; B.4, pp.77–78).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 34. Conformance choice

- **what**: **R/C** Preserve Analyzer-initiated reruns and run identifiers (TF-1 §5.2.10, p.38; TF-2b §2.2.4.7, p.19; B.3).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 35. Conformance choice

- **what**: **R/C** Encode app-authorized manager rerun/reflex as new AWOS with repeat status (TF-1 §5.2.11, pp.38–39; TF-2b §2.2.4.8, p.20).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 36. Conformance choice

- **what**: **R/C** Encode authorized automatic-repeat/reflex permissions for supporting peers (TF-1 §5.2.12, p.39; TF-2b §2.2.4.9, p.20).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 37. Conformance choice

- **what**: **R/C** Preserve requested and applied priority (TF-1 §5.2.13, p.39; TF-2b §2.2.4.10, pp.20–21).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 38. Conformance choice

- **what**: **R/C** Preserve specimen/parent identities and applicable collection/source/risk/time data (TF-1 §5.2.14, p.39; TF-2b §2.2.4.11, p.21).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 39. Conformance choice

- **what**: **R/C** Handle container carrier/volume/units/dilution data (TF-1 §5.2.15, pp.39–40; TF-2b §2.2.4.12, p.21).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 40. Conformance choice

- **what**: **R/C** Preserve mass-spectrometry probability (TF-1 §5.2.16, p.40; TF-2b §2.2.4.13, p.22).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 41. Conformance choice

- **what**: **R/C** Preserve external-result pointers and persistence classification (TF-1 §5.2.17, p.40; TF-2b §2.2.4.15, p.22; B.3.12, p.77).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 42. Conformance choice

- **what**: **R/C** Support enhanced elements outside named options: error detail, comments, grouping, providers, ranges, instance IDs, timing and consumption (TF-2b §2.2.4.18, pp.23–24).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 43. Conformance choice

- **what**: **O/C** Related observations: if selected, send supplied related observations to supporting peers (TF-1 §5.2.18, p.40; TF-2b §2.2.4.14, p.22).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 44. Conformance choice

- **what**: **O/C** Pooling on Analyzer: if selected, request authorized pool size and interpret actual size (TF-1 §5.2.19, p.40; TF-2b §2.2.4.16, pp.22–23).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 45. Conformance choice

- **what**: **O/C** Pooling outside Analyzer: if selected, send pooled-specimen characteristics with restricted patient/specimen structures and required echoes (TF-1 §5.2.20, p.41; TF-2b §2.2.4.17, p.23).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 46. Conformance choice

- **what**: **O** Declare any security/cross-profile groupings separately; LAW itself lists no required actor grouping (TF-1 §§5.3,5.5–5.6, pp.41,54).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 47. Conformance choice

- **what**: **R, project gate** Trace requirements/field predicates to tests and qualified simulator evidence; include lost ACK, restart, storage failure, cancellation race and selected-option cases.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 48. Conformance choice

- **what**: Store raw QBP before lookup/response.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_adr0003_raw_commit_precedes_query_response_and_reservation`.

## 49. Conformance choice

- **what**: Support configured container/SampleId and analyzer-specific all-pending queries, with shared trust/freshness and bounded coherent paging.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_all_pending_pages_filter_other_analyzers_and_keep_every_awos`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.

## 50. Conformance choice

- **what**: Unsupported known forms use QAK AR, never no-work.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work`.

## 51. Conformance choice

- **what**: Fresh trusted empty builds the existing restricted negative LAB-28 without items; its ORL is correlated/retained and audited, with no order changes.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items`, `law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects`.

## 52. Conformance choice

- **what**: Cache failure, staleness, absent/incomplete snapshot, unreachable app, restore pause and elapsed query budget are errors, not empty.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work`, `law_tf2b_3_27_4_unreachable_app_without_snapshot_never_answers_no_work`, `law_tf2b_3_27_4_stale_and_incomplete_cache_with_retained_orders_are_not_empty`, `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage`.

## 53. Conformance choice

- **what**: Included work is preflighted and reserved through shared per-test items before positive RSP/work output; one item is one AWOS. Preparation failure is not a partial positive response.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_3_28_preflight_and_reservation_failure_cannot_emit_positive_rsp_or_work`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.

## 54. Conformance choice

- **what**: ORL bytes are durable before per-AWOS OK/UA reaches `finish_order_items`. Matching uses AWOS IDs, including reordered replies; no reply/disconnect/invalid correlation becomes Uncertain, with no resend.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `finish_order_items`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`, `law_tf2b_3_28_query_followup_timeout_disconnect_and_bad_correlation_are_uncertain`.

## 55. Conformance choice

- **what**: Supplied specimen/priority work content is intersected with explicit peer support; repeat/reflex permissions are never generated.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support`.

## 56. Conformance choice

- **what**: Reserve one shared item per AWOS before unsolicited OML; match ORL by retained AWOS ID through the same `finish_order_items` handler as queries (TF-2b 3.28.4, C.8).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `finish_order_items`, `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos`.

## 57. Conformance choice

- **what**: Use LAW CA control and original stored AWOS IDs for app-requested cancellation; CR confirms per item and UC reports refusal, preserving prior item state (TF-2b 3.28.4, C.8).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos`.

## 58. Conformance choice

- **what**: Query-triggered and unsolicited work reuse reservation, rendering, writing, exchange and ORL handling; queried work can be cancelled with send-ahead disabled (TF-2b 3.27/3.28).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_27_3_28_queried_work_uses_the_same_cancel_exchange_without_send_ahead`.

## 59. Conformance choice

- **what**: Missing ORL or disconnect produces Uncertain for original work and cancellation, with no replay (TF-2b 3.28.4/B.7; ADR 0003 rule 8).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_no_reply_and_disconnect_are_uncertain_never_replayed`.

## 60. Conformance choice

- **what**: Restart during Sending or admitted CancelRequested recovers retained items as Uncertain, never repeats work or cancel bytes (ADR 0003 rule 8).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_restart_during_sending_or_cancelling_is_uncertain_without_resend`.

## 61. Conformance choice

- **what**: Shared restore pause blocks both new work and cancel admission (ADR 0003, TF-2b 3.28.4).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_restore_pause_blocks_send_ahead_and_cancellation`.

## 62. Conformance choice

- **what**: An unmapped command is retained as Rejected without Sending or bytes (TF-2b 3.28.4; ADR 0003).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_unmapped_send_is_rejected_before_reservation_or_bytes`.

## 63. Conformance choice

- **what**: Independent directional connections and per-transaction outbound lifetime (TF-2b B.6, pp.79-80).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`.

## 64. Conformance choice

- **what**: Complete LAW actor qualification: remaining enhanced work groups/options and a recorded Gazelle run. Receive/send/query/ORL deadlines and both topologies have source-named local tests.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 65. Conformance choice

- **what**: Both topology choices, required independent listener role/destination, and rejected unused/invalid destinations.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 66. Conformance choice

- **what**: Both roles validate without startup I/O.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 67. Conformance choice

- **what**: All three transactions share one persistent connection in both configured directions (B.5 and the declared B.6 subset).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 68. Conformance choice

- **what**: Client disconnect/first-byte timeout retains evidence, reconnects and never replays reserved work.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 69. Conformance choice

- **what**: Independent query/result and work links, peer closure, concurrent inbound handling, restart without replay, restore pause and original cancellation identities.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`, `law_tf2b_b6_independent_crash_awaiting_orl_recovers_uncertain_without_resend`.

## 70. Conformance choice

- **what**: Gazelle qualification: record tool build, profile revision, actor/options, scenarios and validation results. Local synthetic tests are not simulator evidence.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 71. Conformance choice

- **what**: Full actor/workflow qualification: remaining enhanced content/groups, pooling/relationships and complete workflows remain as listed above. Isolate/parent needs app-supplied identities in a future contract; rack/tray forms are still explicitly rejected.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 72. Conformance choice

- **what**: Shared stored-byte reprocessing (ADR 0003 rules 2-4; TF-2b 3.29.4). The shared reprocessing path uses LAB-29 calls `evidence::interpret(store, machine, id)`; `machine/interpretation.rs` selects the LAW parser/views/mapper.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `law_tf2b_3_29_4_adr0003_parked_result_reprocesses_original_after_mapping_fix`.

