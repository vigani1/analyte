# LAW decisions

Scope choices and conformance requirements are catalogued together. Status
remains in [checklist.md](checklist.md); unchecked requirements are not claims
of implementation. Original checklist references are retained there. Source:
not found means the original section has not been independently verified for
this catalogue. Existing named tests may cover only part of a requirement.

## 1. Scope choice

- **what**: **Actor:** Analyte implements the **Analyzer Manager** (it receives results and distributes work the lab app authorized). Analyzer behavior lives only in the test peer. Being the manager does not make Analyte take clinical decisions: reruns and reflexes are only sent when the app authorized them.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 2. Scope choice

- **what**: **Sources:** PaLM TF Vol 1 and 2b, the PaLM TF-2x volume (§2.2.4), the IHE Technical Frameworks General Introduction (Appendix E) and HL7 v2.5.1 chapters 2, 2A, 3, 4, 7 and 13. Vol 2a and Vol 3 are not needed for LAW alone.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 3. Scope choice

- **what**: **Done:** every manager requirement and selected option in this list has tests, plus a recorded Gazelle run (tool build, validation profile revision, actor, options, scenarios). A simulator pass alone is not completeness.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 4. Conformance choice

- **what**: **R** Support LAB-27 responder, LAB-28 initiator and LAB-29 responder; preserve unique AWOS and work linkage (TF-1 §§5.1–5.1.1, pp.33–34; §5.4.1, pp.41–42).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`.

## 5. Conformance choice

- **what**: **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_3_27_config_bounds_deadlines_and_declares_only_supported_query_forms`, `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos`.

## 6. Conformance choice

- **what**: **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_2_2_usage_cardinality_length_and_null_rules`, `law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings`.

## 7. Conformance choice

- **what**: **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c4_msh_21_m_ei_required_components_lengths_first_transaction`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`, `law_tf2b_2_2_config_selects_mllp_251_manager_and_explicit_peer_options`, `law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support`.

## 8. Conformance choice

- **what**: **S** Send enhanced data only to supporting peers (TF-1 §5.1.1.2, p.34).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c11_c12_explicit_profile_cannot_enlarge_peer_send_capabilities`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.

## 9. Conformance choice

- **what**: **R** Use directional MLLP initiating connections; tolerate peer closure and independent establishment (TF-2b B.6, pp.79-80; TF-2x section 2.2.4 dependency).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`, `law_tf2b_b5_b6_declared_single_persistent_link_carries_all_transactions_in_both_directions`, `law_tf2b_b6_single_persistent_client_reconnects_without_replaying_work`.

## 10. Conformance choice

- **what**: **R** Use NE/AL application acknowledgement mode and correct header reversal/MSA-2 correlation; do not reject incoming application/facility names merely for differing from configured values (TF-2b B.5, pp.78–79; HL7 §2.9, pp.2-24–2-28, §2.15.8.2).
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `law_tf2b_c4_msh_15_16_c_m_x_predicate_is_trigger_versus_response`, `law_tf2b_c4_c3_b5_receive_query_reply_reversal_correlation_ignored_content`.

## 11. Conformance choice

- **what**: **R** Parse QBP and return correctly correlated RSP, distinguishing acceptance/error from actual work availability (TF-2b §3.27.4, pp.34–44).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`.

## 12. Conformance choice

- **what**: **R/C** Handle container, isolate, rack/position, tray/position, all-pending and informational location forms as applicable to connected peers (TF-1 §§5.2.1–5.2.5, pp.36–37; TF-2b QPD definition, pp.35–41).
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_2_2_4_2_c11_c12_negative_query_echoes_each_scope_without_invented_ids`, `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work`.

## 13. Conformance choice

- **what**: **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`, `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos`, `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos`.

## 14. Conformance choice

- **what**: **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`, `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos`.

## 15. Conformance choice

- **what**: **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_2_2_4_2_c11_c12_negative_query_echoes_each_scope_without_invented_ids`, `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items`, `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage`, `law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects`.

## 16. Conformance choice

- **what**: **R** Preserve uncertain prior acceptance; duplicate-AWOS UA must not undo the original accepted request (TF-2b §3.28.4.1.3, p.51).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`.

## 17. Conformance choice

- **what**: **R** Use separate AWOS for separately requested replicates (TF-2b B.8, p.83).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.

## 18. Conformance choice

- **what**: **R** Receive OUL and return ACK with required structures/fields; correlate known AWOS to originating work (TF-2b §3.29.4, pp.53–58).
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `law_tf2b_3_29_4_ack_r22_requires_complete_interpretation_and_durability`.

## 19. Conformance choice

- **what**: **R, host boundary** Store the exact LAB-29 MLLP block before interpretation/ACK; AA only after durable interpretation, with no partial batch delivery on a conflict (TF-2b §3.29.4; B.7; ADR 0003). Tests in `crates/analyte/tests/law/results.rs`: `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack`, `law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa`, `law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa`, `law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack`, `law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa`, `law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa`, `law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`.

## 20. Conformance choice

- **what**: **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_29_4_unsolicited_null_awos_and_external_reference_are_not_invented`.

## 21. Conformance choice

- **what**: **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`, `law_tf2b_c7_obx_18_first_second_later_equipment_ei_rules`.

## 22. Conformance choice

- **what**: **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`, `law_tf2b_c7_obx_11_closed_status_and_explicit_correction_only`, `law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`, `adr0011_law_c7_figure_1_every_permitted_and_forbidden_move`, `law_adr0011_c7_all_status_moves_and_unverified_delivery`, `law_adr0011_c7_only_one_final_run_in_history_or_batch`, `adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`, `law_tf2b_3_29_4_unsolicited_uses_explicit_analyzer_order_not_invented_awos`.

## 23. Conformance choice

- **what**: **R** Preserve supplemental/primary relationships, typed values, units, statuses and scoped notes; do not treat NTE prose as executable rules (TF-2b B.3; C.5/C.7, pp.95–113).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c7_obx_5_st_tx_and_rule5_original_values_units`, `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`.

## 24. Conformance choice

- **what**: **R** Implement every applicable manager usage for ERR, INV, MSA, MSH, NTE, OBR, OBX, ORC, PID, PV1, SAC, SPM, TCD and TQ1, including component usages and conditional predicates (TF-2b Appendix C, pp.84–141).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings`, `law_tf2b_c6_c8_c13_c14_x_fields_components_and_composite_ceiling`.

## 25. Conformance choice

- **what**: **R** Apply coding-system identification, test/result vocabulary and units rules without inventing codes (TF-2b §2.3, pp.24–31).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c7_obx_3_cwe_alternate_predicates_and_namespaces`, `law_tf2b_c7_obx_6_units_identifier_namespace_predicate_and_null`.

## 26. Conformance choice

- **what**: **R** Distinguish malformed-trigger AE, control-content AR, invalid ACK failure, inconsistent ORL failure and query processing rejection; report locations where required (TF-2b B.7.1–B.7.8, pp.80–82). Host ERR location test: `law_tf2b_c1_b7_second_obx_error_reports_actual_occurrence` covers malformed second OBX-11, second OBX-1 sequencing, and malformed first OBX among repeated segments, with raw retention and whole-message refusal.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `law_tf2b_c1_b7_second_obx_error_reports_actual_occurrence`, `law_tf2b_b7_1_b7_2_host_correlates_errors_without_ack_loops`, `law_tf2b_b7_trigger_rejection_and_query_processing_errors_have_distinct_actions`, `law_tf2b_b7_invalid_ack_and_inconsistent_orl_fail_without_ack_loop`, `law_tf2b_c7_remaining_component_boundaries_and_located_failures`.

## 27. Conformance choice

- **what**: **R** Reject inconsistent LAB-29 as a whole, including TCD-1/OBX-3 mismatch; retain bytes without applying a subset (TF-2b p.57; B.7.6, p.82).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_b7_6_inconsistent_tcd_rejects_whole_message_and_retains_original`, `law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`, `law_tf2b_b7_6_returned_work_must_match_downloaded_awos`, `law_tf2b_b7_6_original_command_is_owned_immutable_and_survives_restart`.

## 28. Conformance choice

- **what**: **R** Keep manager-sourced patient data authoritative; correct downloaded patient information through cancel/new-AWOS, not repeated work (TF-2b B.7.9, pp.82–83).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`.

## 29. Conformance choice

- **what**: **R** Handle unknown/non-HL7 input without compromising the application (TF-2b B.7.10, p.83).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c4_c1_hl7_251_2_7_malformed_escapes_do_not_bypass_profile_validation`.

## 30. Conformance choice

- **what**: **R/C** Preserve contributing substance/disposable identifiers, types and available lot/container data (TF-1 §5.2.6, p.37; TF-2b §2.2.4.3, p.17; C.2).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c2_inv_required_ce_and_2_2_3_identifier_lot_conformance`, `law_tf2b_c2_inv_4_12_re_an_container_expiry_and_option_direction`, `law_tf2b_c2_control_inventory_co_predicate_and_result_ownership`.

## 31. Conformance choice

- **what**: **R/C** Send explicit requested dilution and retain actual dilution/value meaning (TF-1 §5.2.7, pp.37–38; TF-2b §2.2.4.4, pp.17–18; C.13).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c13_safety_rule_6_reported_dilution_keeps_original_and_explicit_adjustment`, `law_tf2b_c13_safety_rule_6_type_only_dilution_has_adjustment_metadata`, `law_tf2b_c8_c6_c13_work_builder_preserves_supplied_event_and_validates_details`.

## 32. Conformance choice

- **what**: **R/C** Handle profiled patient/encounter demographics without invented values (TF-1 §5.2.8, p.38; TF-2b §2.2.4.5, pp.18–19).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c9_c10_builder_and_receive_validate_profile`.

## 33. Conformance choice

- **what**: **R/C** Preserve Analyzer reflex parent/child relationships (TF-1 §5.2.9, p.38; TF-2b §2.2.4.6, p.19; B.4, pp.77–78).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c8_orc_4_re_eip_and_8_reflex_parents_repeated`, `law_tf2b_c6_obr_11_reflex_g_and_49_result_handling_re`.

## 34. Conformance choice

- **what**: **R/C** Preserve Analyzer-initiated reruns and run identifiers (TF-1 §5.2.10, p.38; TF-2b §2.2.4.7, p.19; B.3).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`.

## 35. Conformance choice

- **what**: **R/C** Encode app-authorized manager rerun/reflex as new AWOS with repeat status (TF-1 §5.2.11, pp.38–39; TF-2b §2.2.4.8, p.20).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c13_tcd_6_7_r_permissions_8_r_repeat_status`, `law_tf2b_c8_c6_c13_work_builder_preserves_supplied_event_and_validates_details`.

## 36. Conformance choice

- **what**: **R/C** Encode authorized automatic-repeat/reflex permissions for supporting peers (TF-1 §5.2.12, p.39; TF-2b §2.2.4.9, p.20).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c13_tcd_6_7_r_permissions_8_r_repeat_status`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.

## 37. Conformance choice

- **what**: **R/C** Preserve requested and applied priority (TF-1 §5.2.13, p.39; TF-2b §2.2.4.10, pp.20–21).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`.

## 38. Conformance choice

- **what**: **R/C** Preserve specimen/parent identities and applicable collection/source/risk/time data (TF-1 §5.2.14, p.39; TF-2b §2.2.4.11, p.21).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c12_spm_2_3_eip_ei_identity_namespace_predicate_lengths_and_repetitions`, `law_tf2b_c12_spm_17_18_dr_ts_required_start_seconds_calendar_and_x`.

## 39. Conformance choice

- **what**: **R/C** Handle container carrier/volume/units/dilution data (TF-1 §5.2.15, pp.39–40; TF-2b §2.2.4.12, p.21).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c11_c12_work_builder_validates_container_details_and_option_fields`, `law_tf2b_c11_sac_29_re_sn_exact_ratio_and_positive_denominator`.

## 40. Conformance choice

- **what**: **R/C** Preserve mass-spectrometry probability (TF-1 §5.2.16, p.40; TF-2b §2.2.4.13, p.22).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c7_obx_9_mass_spec_probability_exact_range_option_and_manager_x`.

## 41. Conformance choice

- **what**: **R/C** Preserve external-result pointers and persistence classification (TF-1 §5.2.17, p.40; TF-2b §2.2.4.15, p.22; B.3.12, p.77).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c7_obx_5_rp_nested_hd_limits_and_no_pointer_fetch`, `law_tf2b_c6_obr_11_reflex_g_and_49_result_handling_re`.

## 42. Conformance choice

- **what**: **R/C** Support enhanced elements outside named options: error detail, comments, grouping, providers, ranges, instance IDs, timing and consumption (TF-2b §2.2.4.18, pp.23–24).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c5_work_builder_validates_and_keeps_truncatable_notes`, `law_tf2b_c7_obx_21_available_ei_identity_and_29_type_subset`, `law_tf2b_c13_tcd_8_required_text_and_9_nested_cq_limits`.

## 43. Conformance choice

- **what**: **O/C** Related observations: if selected, send supplied related observations to supporting peers (TF-1 §5.2.18, p.40; TF-2b §2.2.4.14, p.22).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c7_obx_5_ce_ei_na_components_and_lengths`.

## 44. Conformance choice

- **what**: **O/C** Pooling on Analyzer: if selected, request authorized pool size and interpret actual size (TF-1 §5.2.19, p.40; TF-2b §2.2.4.16, pp.22–23).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c13_tcd_9_re_cq_ucum_units_and_10_pool_size`.

## 45. Conformance choice

- **what**: **O/C** Pooling outside Analyzer: if selected, send pooled-specimen characteristics with restricted patient/specimen structures and required echoes (TF-1 §5.2.20, p.41; TF-2b §2.2.4.17, p.23).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_c12_spm_3_13_pooling_outside_r_x_count_and_role`, `law_tf2b_c11_c12_remaining_location_lengths_and_pool_receive_predicates`.

## 46. Conformance choice

- **what**: **O** Declare any security/cross-profile groupings separately; LAW itself lists no required actor grouping (TF-1 §§5.3,5.5–5.6, pp.41,54).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 47. Conformance choice

- **what**: **R, project gate** Trace requirements/field predicates to tests and qualified simulator evidence; include lost ACK, restart, storage failure, cancellation race and selected-option cases.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 48. Conformance choice

- **what**: Store raw QBP before lookup/response.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_adr0003_raw_commit_precedes_query_response_and_reservation`.

## 49. Conformance choice

- **what**: Support configured container/SampleId and analyzer-specific all-pending queries, with shared trust/freshness and bounded coherent paging.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_all_pending_pages_filter_other_analyzers_and_keep_every_awos`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.

## 50. Conformance choice

- **what**: Unsupported known forms use QAK AR, never no-work.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work`.

## 51. Conformance choice

- **what**: Fresh trusted empty builds the existing restricted negative LAB-28 without items; its ORL is correlated/retained and audited, with no order changes.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items`, `law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects`.

## 52. Conformance choice

- **what**: Cache failure, staleness, absent/incomplete snapshot, unreachable app, restore pause and elapsed query budget are errors, not empty.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work`, `law_tf2b_3_27_4_unreachable_app_without_snapshot_never_answers_no_work`, `law_tf2b_3_27_4_stale_and_incomplete_cache_with_retained_orders_are_not_empty`, `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage`.

## 53. Conformance choice

- **what**: Included work is preflighted and reserved through shared per-test items before positive RSP/work output; one item is one AWOS. Preparation failure is not a partial positive response.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_3_28_preflight_and_reservation_failure_cannot_emit_positive_rsp_or_work`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.

## 54. Conformance choice

- **what**: ORL bytes are durable before per-AWOS OK/UA reaches `finish_order_items`. Matching uses AWOS IDs, including reordered replies; no reply/disconnect/invalid correlation becomes Uncertain, with no resend.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `finish_order_items`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`, `law_tf2b_3_28_query_followup_timeout_disconnect_and_bad_correlation_are_uncertain`.

## 55. Conformance choice

- **what**: Supplied specimen/priority work content is intersected with explicit peer support; repeat/reflex permissions are never generated.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support`, `law_tf2b_3_27_4_pending_pages_filter_targets_without_false_empty_and_never_replay`, `law_tf2b_3_27_4_sample_and_pending_share_freshness_generation_and_restore_trust`, `pending_orders`, `law_tf2b_3_27_3_28_work_requires_explicit_bindings_and_allows_send_ahead`, `law_tf2b_c4_work_and_negative_builders_keep_fifty_character_control_ids`, `law_tf2b_3_28_4_negative_lab28_for_genuine_no_work`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`.

## 56. Conformance choice

- **what**: Reserve one shared item per AWOS before unsolicited OML; match ORL by retained AWOS ID through the same `finish_order_items` handler as queries (TF-2b 3.28.4, C.8).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `finish_order_items`, `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos`.

## 57. Conformance choice

- **what**: Use LAW CA control and original stored AWOS IDs for app-requested cancellation; CR confirms per item and UC reports refusal, preserving prior item state (TF-2b 3.28.4, C.8).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos`.

## 58. Conformance choice

- **what**: Query-triggered and unsolicited work reuse reservation, rendering, writing, exchange and ORL handling; queried work can be cancelled with send-ahead disabled (TF-2b 3.27/3.28).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_3_28_queried_work_uses_the_same_cancel_exchange_without_send_ahead`.

## 59. Conformance choice

- **what**: Missing ORL or disconnect produces Uncertain for original work and cancellation, with no replay (TF-2b 3.28.4/B.7; ADR 0003 rule 8).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_no_reply_and_disconnect_are_uncertain_never_replayed`.

## 60. Conformance choice

- **what**: Restart during Sending or admitted CancelRequested recovers retained items as Uncertain, never repeats work or cancel bytes (ADR 0003 rule 8).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_restart_during_sending_or_cancelling_is_uncertain_without_resend`.

## 61. Conformance choice

- **what**: Shared restore pause blocks both new work and cancel admission (ADR 0003, TF-2b 3.28.4).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_restore_pause_blocks_send_ahead_and_cancellation`.

## 62. Conformance choice

- **what**: An unmapped command is retained as Rejected without Sending or bytes (TF-2b 3.28.4; ADR 0003).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_28_adr0003_unmapped_send_is_rejected_before_reservation_or_bytes`.

## 63. Conformance choice

- **what**: Independent directional connections and per-transaction outbound lifetime (TF-2b B.6, pp.79-80).
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`.

## 64. Conformance choice

- **what**: Complete LAW actor qualification: remaining enhanced work groups/options and a recorded Gazelle run. Receive/send/query/ORL deadlines and both topologies have source-named local tests. The runtime uses the shared order store (schema 9) and operator controls.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 65. Conformance choice

- **what**: Both topology choices, required independent listener role/destination, and rejected unused/invalid destinations.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_b6_declared_topologies_require_explicit_direction_and_outbound_endpoint`.

## 66. Conformance choice

- **what**: Both roles validate without startup I/O.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_b6_single_persistent_manager_accepts_client_and_server_without_startup_io`.

## 67. Conformance choice

- **what**: All three transactions share one persistent connection in both configured directions (B.5 and the declared B.6 subset).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_b5_b6_declared_single_persistent_link_carries_all_transactions_in_both_directions`.

## 68. Conformance choice

- **what**: Client disconnect/first-byte timeout retains evidence, reconnects and never replays reserved work.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `law_tf2b_b6_single_persistent_client_reconnects_without_replaying_work`.

## 69. Conformance choice

- **what**: Independent query/result and work links, peer closure, concurrent inbound handling, restart without replay, restore pause and original cancellation identities.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`, `law_tf2b_b6_independent_crash_awaiting_orl_recovers_uncertain_without_resend`.

## 70. Conformance choice

- **what**: Gazelle qualification: record tool build, profile revision, actor/options, scenarios and validation results. Local synthetic tests are not simulator evidence.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 71. Conformance choice

- **what**: Full actor/workflow qualification: remaining enhanced content/groups, pooling/relationships and complete workflows remain as listed above. Isolate/parent needs app-supplied identities in a future contract; rack/tray forms are still explicitly rejected.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 72. Conformance choice

- **what**: Shared stored-byte reprocessing (ADR 0003 rules 2-4; TF-2b 3.29.4). The shared reprocessing path uses LAB-29 calls `evidence::interpret(store, machine, id)`; `machine/interpretation.rs` selects the LAW parser/views/mapper.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_29_4_adr0003_parked_result_reprocesses_original_after_mapping_fix`.

## 73. Host custody policy

- **what**: Retain original raw evidence before a positive acknowledgment; park uninterpretable input.
- **why**: A positive response must not promise custody of evidence the host can lose.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), Decision, LAW row; [ADR 0003](../../adr/0003-clinical-safety-rules.md), Decision.
- **test**: `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack`.

## 74. Result history policy

- **what**: Assign revisions through the store using trusted identity and syntax-owned status progression; park conflicting changes.
- **why**: A preliminary-to-final transition differs from an unmarked correction after final.
- **source**: [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`.
