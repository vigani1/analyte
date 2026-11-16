# LAW conformance

## Checklist

Legend: **R** required; **O** selectable manager option; **C** conditional obligation; **S** recommendation/SHOULD. Requirements below are grouped; expand the cited schemas into tests for each manager-column field/component predicate. This is not a replacement for normative tables.

### Actor/profile/connection

- [ ] **R** Support LAB-27 responder, LAB-28 initiator and LAB-29 responder; preserve unique AWOS and work linkage (TF-1 §§5.1–5.1.1, pp.33–34; §5.4.1, pp.41–42).
  Tests: `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`.
  Host results, queries, send-ahead and cancellation have the program matrices below. Originating-work result correlation and both link topologies have the named host tests below; simulator and complete actor qualification remain.
- [ ] **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
  Tests: `law_tf2b_3_27_4_query_forms_and_conditional_fields`.
  Pure query forms and broadcast rendering are tested. Host config declares container/all-pending only (`crates/core/config/tests/law.rs::law_tf2b_3_27_config_bounds_deadlines_and_declares_only_supported_query_forms`); the query host matrix now covers both. Send-ahead is explicitly selected by `orders.send_ahead` (default query example: false); host evidence: `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos` in `crates/analyte/tests/law/work.rs`. Independent broadcast links have the B.6 matrix below. Isolate/parent needs app-supplied identities in a future contract version; rack/tray/informational-position forms are explicitly rejected by the host, without a no-work claim.
- [x] **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
  Tests: `law_tf2b_2_2_usage_cardinality_length_and_null_rules`, `law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings`.
  Field-profile tests also span law_control.rs, law_patient.rs, law_specimen.rs and law_order.rs; cross-message negotiation is separate.
- [ ] **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
  Tests: `law_tf2b_c4_msh_21_m_ei_required_components_lengths_first_transaction`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`.
  Explicit content/profile selection and host peer declarations are tested by `crates/core/config/tests/law.rs::law_tf2b_2_2_config_selects_mllp_251_manager_and_explicit_peer_options`. Query work now selects only supplied basic/specimen/priority content intersected with peer support (`law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support` in the program). Other enhanced host work content and automatic option/profile negotiation remain absent.
- [x] **S** Send enhanced data only to supporting peers (TF-1 §5.1.1.2, p.34).
  Tests: `law_tf2b_c11_c12_explicit_profile_cannot_enlarge_peer_send_capabilities`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.
  Outgoing supported builders intersect explicit content selection with peer capabilities.
- [x] **R** Use directional MLLP initiating connections; tolerate peer closure and independent establishment (TF-2b B.6, pp.79-80; TF-2x §2.2.4 dependency).
  Tests: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos` in `crates/analyte/tests/law/connections.rs`. The existing persistent topology is covered by `law_tf2b_b5_b6_declared_single_persistent_link_carries_all_transactions_in_both_directions` and `law_tf2b_b6_adr0003_client_partial_deadline_retains_exact_bytes_across_reconnect`. Independent mode listens for LAB-27/LAB-29 and opens a separate LAB-28 connection per transaction. Synthetic tests remain documented evidence, not simulator qualification.
- [x] **R** Use NE/AL application acknowledgement mode and correct header reversal/MSA-2 correlation; do not reject incoming application/facility names merely for differing from configured values (TF-2b B.5, pp.78–79; HL7 §2.9, pp.2-24–2-28, §2.15.8.2).
  Tests: `law_tf2b_c4_msh_15_16_c_m_x_predicate_is_trigger_versus_response`, `law_tf2b_c4_c3_b5_receive_query_reply_reversal_correlation_ignored_content`.
  Pure header/acknowledgment policy; host storage evidence is a separate gate.

### Query/work

- [x] **R** Parse QBP and return correctly correlated RSP, distinguishing acceptance/error from actual work availability (TF-2b §3.27.4, pp.34–44).
  Tests: `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`.
  Tests distinguish genuine empty work from unavailable lookup.
- [x] **R/C** Handle container, isolate, rack/position, tray/position, all-pending and informational location forms as applicable to connected peers (TF-1 §§5.2.1–5.2.5, pp.36–37; TF-2b QPD definition, pp.35–41).
  Tests: `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_3_28_4_1_2_1_negative_query_echoes_all_sac_identity_values`.
  All six pure query scopes have synthetic evidence. The running program supports container and all-pending only; known unsupported forms return QAK AR (`law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work`). This checked row is pure view/build support, not a claim of all-form host support.
- [ ] **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
  Tests: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`.
  Supported builders and per-AWOS responses are covered, including query-triggered host work/ORL (`law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`). Host send-ahead and cancellation now share the same path, covered in `crates/analyte/tests/law/work.rs` by `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos` and `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos`. Related-observation and pooling work groups, broader enhanced work content remain outside the supported scope; both link topologies are covered below.
- [x] **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
  Tests: `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`.
  Every permitted/refused request-specific control/status pair is checked. Host item decisions are covered by `crates/analyte/tests/law/work.rs::law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos`.
- [x] **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
  Tests: `law_tf2b_3_28_4_1_2_1_negative_query_echoes_all_sac_identity_values`.
  Host evidence for the selected forms: `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items`, `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage`, and `law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects` in `crates/analyte/tests/law/queries.rs`. No items are reserved for negative work. Unsupported forms return an error rather than negative work.
- [ ] **R** Preserve uncertain prior acceptance; duplicate-AWOS UA must not undo the original accepted request (TF-2b §3.28.4.1.3, p.51).
  Tests: `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`.
  Parser decisions preserve uncertainty; persistent duplicate/retry/lifecycle integration remains.
- [x] **R** Use separate AWOS for separately requested replicates (TF-2b B.8, p.83).
  Tests: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.
  Separate supplied AWOS identities are required; renderer does not create replicate identities.

### Results/schema/errors

- [ ] **R** Receive OUL and return ACK with required structures/fields; correlate known AWOS to originating work (TF-2b §3.29.4, pp.53–58).
  Tests: `law_tf2b_3_29_4_ack_r22_requires_complete_interpretation_and_durability`.
  Host LAB-29 receive/ACK is covered by the result matrix below. Returned work is correlated against retained AWOS, specimen, container and test content, including after restart. The remaining actor/structure qualification is open, so this combined row stays unchecked.
- [x] **R, host boundary** Store the exact LAB-29 MLLP block before interpretation/ACK; AA only after durable interpretation, with no partial batch delivery on a conflict (TF-2b §3.29.4; B.7; ADR 0003).
  Tests in `crates/analyte/tests/law/results.rs`: `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack`, `law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa`, `law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa`, `law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`.
- [x] **R, host boundary** Retain empty SC/IP/CA as per-item execution evidence, independently of command and observation state (TF-2b §3.29.4.1.2.1; C.8 Table C.8-9/Figure C.8-1; B.7.6; ADRs 0003, 0011, 0016).
  Protocol/mapping tests: `law_tf2b_3_29_4_1_2_1_status_only_ip_sc_ca_is_typed_without_results`, `law_tf2b_c8_table_c8_9_estimate_preserves_spelling_null_and_header_zone`, `law_tf2b_3_29_4_1_2_1_mixed_status_groups_do_not_hide_result_groups`.
  Host tests in `crates/analyte/tests/law/work.rs`: `palm_tf2b_b7_6_c8_adr0003_0011_status_custody_repeats_restart_and_conflicts`, `palm_tf2b_b7_6_adr0003_0011_mixed_status_result_accepts_or_rejects_as_one_message`. Store tests in `crates/core/store/src/orders/machine_orders/tests/custody.rs` cover projection rollback, sparse items, stable cancellation conditions, pending cancel CR/UC, admission refusal and bounded history. Status notices allocate no result/outbox row. Exact receipts and dispositions survive restart. Admin-only handling requires a named responder; contract v1 is unchanged. Official simulator status/cancel-race qualification remains open; this row records documented host implementation, not actor qualification.
- [x] **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
  Tests: `law_tf2b_3_29_4_unsolicited_null_awos_and_external_reference_are_not_invented`.
  No downloaded AWOS is invented or required for unsolicited observations.
- [x] **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
  Tests: `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`, `law_tf2b_c7_obx_18_first_second_later_equipment_ei_rules`.
  Original identities are retained alongside validated fields.
- [x] **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
  Tests: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`, `law_tf2b_c7_repeated_observation_requires_unique_complete_og`, `law_tf2b_c7_obx_11_closed_status_and_explicit_correction_only`.
  Host evidence: `crates/analyte/tests/law/results.rs::law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c` and `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`. AWOS takes precedence over analyzer order; OG.1 identifies the run and the complete OG distinguishes observations within it (C.7 pp.102–103). Retransmission creates no delivery. ADR 0011 and Figure C.7-1 (p.111) allow P->P/R/F, R->R/F, F->C and C->C for changed content; every other changed-content move conflicts. R is retained without delivery, including across restart before its final. At most one run per order/test may reach F/C; conflicting batches are rejected whole. Tests: `adr0011_law_c7_figure_1_every_permitted_and_forbidden_move`, `law_adr0011_c7_all_status_moves_and_unverified_delivery`, `law_adr0011_c7_only_one_final_run_in_history_or_batch`. Generic store test: `adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`. HL7 v2 and ASTM still use their existing correction/withdrawal rules; their lifecycle tables are not enabled. Pure status validation retains the closed LAW status set. Unsolicited explicit analyzer order/authority and unresolved identity are covered by `law_tf2b_3_29_4_unsolicited_uses_explicit_analyzer_order_not_invented_awos`. Restart qualification remains a separate gate.
- [ ] **R** Preserve supplemental/primary relationships, typed values, units, statuses and scoped notes; do not treat NTE prose as executable rules (TF-2b B.3; C.5/C.7, pp.95–113).
  Tests: `law_tf2b_c7_obx_5_st_tx_and_rule5_original_values_units`, `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`.
  Typed/raw values and note scope are covered. Full cross-message supplemental/primary relationship reconciliation remains.
- [x] **R** Implement every applicable manager usage for ERR, INV, MSA, MSH, NTE, OBR, OBX, ORC, PID, PV1, SAC, SPM, TCD and TQ1, including component usages and conditional predicates (TF-2b Appendix C, pp.84–141).
  Tests: `law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings`, `law_tf2b_c6_c8_c13_c14_x_fields_components_and_composite_ceiling`.
  All five field-profile suites provide per-rule positive/negative cases. Read the README source-discrepancy policies; this row is field validation, not complete message/lifecycle conformance.
- [ ] **R** Apply coding-system identification, test/result vocabulary and units rules without inventing codes (TF-2b §2.3, pp.24–31).
  Tests: `law_tf2b_c7_obx_3_cwe_alternate_predicates_and_namespaces`, `law_tf2b_c7_obx_6_units_identifier_namespace_predicate_and_null`.
  Namespaces and unit preservation are tested. Complete vocabulary binding and partner-specific supplemental/reference relationships remain.
- [x] **R** Distinguish malformed-trigger AE, control-content AR, invalid ACK failure, inconsistent ORL failure and query processing rejection; report locations where required (TF-2b B.7.1–B.7.8, pp.80–82).
  Host ERR location test: `law_tf2b_c1_b7_second_obx_error_reports_actual_occurrence` covers malformed second OBX-11, second OBX-1 sequencing, and malformed first OBX among repeated segments, with raw retention and whole-message refusal. Host test: `law_tf2b_b7_1_b7_2_host_correlates_errors_without_ack_loops` covers retained malformed queries, syntax errors, unsupported triggers, invalid version/profile and silence for invalid ACK/ORL/RSP. Tests: `law_tf2b_b7_trigger_rejection_and_query_processing_errors_have_distinct_actions`, `law_tf2b_b7_invalid_ack_and_inconsistent_orl_fail_without_ack_loop`, `law_tf2b_c7_remaining_component_boundaries_and_located_failures`.
  Failures preserve original input and carry located content errors.
- [x] **R** Reject inconsistent LAB-29 as a whole, including TCD-1/OBX-3 mismatch; retain bytes without applying a subset (TF-2b p.57; B.7.6, p.82).
  Tests: `law_tf2b_b7_6_inconsistent_tcd_rejects_whole_message_and_retains_original`.
  Host revision conflicts return AR (B.7.6, p.82), while mapping failures remain AE. Tests: `law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`. Originating-work checks are covered by `law_tf2b_c6_2_x_awos_components_do_not_change_retained_work_match` (B.3.1 pp.63–64; B.7.6 p.82): unknown/nonmatching placer, specimen identity/type, container and requested test reject the entire batch with AR and an ERR location; null-AWOS unsolicited results remain allowed. Prepared outbound work is retained with the reservation before I/O and survives restart. Missing legacy command evidence is rejected for operator review. `law_tf2b_b7_6_original_command_is_owned_immutable_and_survives_restart` covers store ownership and immutability.
  No partial Results view escapes a failed message.
- [ ] **R** Keep manager-sourced patient data authoritative; correct downloaded patient information through cancel/new-AWOS, not repeated work (TF-2b B.7.9, pp.82–83).
  Tests: `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`.
  Only request-specific decisions are tested. Persistent patient-authority/cancel-new correction workflow has no LAW integration test yet.
- [x] **R** Handle unknown/non-HL7 input without compromising the application (TF-2b B.7.10, p.83).
  Tests: `law_tf2b_c4_c1_hl7_251_2_7_malformed_escapes_do_not_bypass_profile_validation`.
  Bounded syntax failure retains the host-owned original; generic parser property tests add arbitrary-input evidence.

### Enhanced content and manager options

These are optional Analyzer capabilities, but relevant manager enhanced-content support is required; outgoing use is conditional on the peer and available app-supplied information. They do not require Analyte to measure or make clinical rerun/reflex decisions.

- [x] **R/C** Preserve contributing substance/disposable identifiers, types and available lot/container data (TF-1 §5.2.6, p.37; TF-2b §2.2.4.3, p.17; C.2).
  Tests: `law_tf2b_c2_inv_required_ce_and_2_2_3_identifier_lot_conformance`, `law_tf2b_c2_inv_4_12_re_an_container_expiry_and_option_direction`, `law_tf2b_c2_control_inventory_co_predicate_and_result_ownership`.
  Inventory fields, option handling and ownership are covered without interpreting substance data clinically.
- [x] **R/C** Send explicit requested dilution and retain actual dilution/value meaning (TF-1 §5.2.7, pp.37–38; TF-2b §2.2.4.4, pp.17–18; C.13).
  Tests: `law_tf2b_c13_safety_rule_6_reported_dilution_keeps_original_and_explicit_adjustment`, `law_tf2b_c13_safety_rule_6_type_only_dilution_has_adjustment_metadata`, `law_tf2b_c8_c6_c13_work_builder_preserves_supplied_event_and_validates_details`.
  Exact factors/types and host-supplied adjustment meaning are retained; no observation is multiplied.
- [x] **R/C** Handle profiled patient/encounter demographics without invented values (TF-1 §5.2.8, p.38; TF-2b §2.2.4.5, pp.18–19).
  Tests: `law_tf2b_c9_c10_builder_and_receive_validate_profile`.
  Demographic field validation and null/authority bindings are tested; no patient reconciliation is performed.
- [ ] **R/C** Preserve Analyzer reflex parent/child relationships (TF-1 §5.2.9, p.38; TF-2b §2.2.4.6, p.19; B.4, pp.77–78).
  Tests: `law_tf2b_c8_orc_4_re_eip_and_8_reflex_parents_repeated`, `law_tf2b_c6_obr_11_reflex_g_and_49_result_handling_re`.
  Parent/action fields are validated and retained. Cross-message parent resolution and reflex lifecycle integration remain.
- [ ] **R/C** Preserve Analyzer-initiated reruns and run identifiers (TF-1 §5.2.10, p.38; TF-2b §2.2.4.7, p.19; B.3).
  Tests: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`, `law_tf2b_c7_repeated_observation_requires_unique_complete_og`.
  Run identifiers and ambiguity rejection are tested; persistent run/retransmission reconciliation remains.
- [ ] **R/C** Encode app-authorized manager rerun/reflex as new AWOS with repeat status (TF-1 §5.2.11, pp.38–39; TF-2b §2.2.4.8, p.20).
  Tests: `law_tf2b_c13_9_repeat_status_output_preserves_profiled_tuple`, `law_tf2b_c13_9_repeat_status_text_matches_code`, `law_tf2b_c8_c6_c13_work_builder_preserves_supplied_event_and_validates_details`.
  Explicit repeat status and unique supplied AWOS rendering are tested. Host authorization and durable new-work lifecycle still need integration evidence.
- [x] **R/C** Encode authorized automatic-repeat/reflex permissions for supporting peers (TF-1 §5.2.12, p.39; TF-2b §2.2.4.9, p.20).
  Tests: `law_tf2b_c13_9_repeat_status_text_matches_code`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.
  No permission defaults are synthesized.
- [x] **R/C** Preserve requested and applied priority (TF-1 §5.2.13, p.39; TF-2b §2.2.4.10, pp.20–21).
  Tests: `law_tf2b_c14_requested_and_applied_priority_preserve_exact_code`, `law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`.
  Explicit peer/receive option selection is required; no automatic negotiation is claimed.
- [x] **R/C** Preserve specimen/parent identities and applicable collection/source/risk/time data (TF-1 §5.2.14, p.39; TF-2b §2.2.4.11, p.21).
  Tests: `law_tf2b_c12_spm_2_3_eip_ei_identity_namespace_predicate_lengths_and_repetitions`, `law_tf2b_c12_spm_17_18_dr_ts_required_start_seconds_calendar_and_x`.
  Other selected specimen field cases are in law_specimen.rs.
- [x] **R/C** Handle container carrier/volume/units/dilution data (TF-1 §5.2.15, pp.39–40; TF-2b §2.2.4.12, p.21).
  Tests: `law_tf2b_c11_c12_work_builder_validates_container_details_and_option_fields`, `law_tf2b_c11_sac_29_re_sn_exact_ratio_and_positive_denominator`.
  Container information remains as supplied; no dilution conversion is performed.
- [x] **R/C** Preserve mass-spectrometry probability (TF-1 §5.2.16, p.40; TF-2b §2.2.4.13, p.22).
  Tests: `law_tf2b_c7_obx_9_mass_spec_probability_exact_range_option_and_manager_x`.
  Probability is range-checked without floating-point arithmetic and retained exactly.
- [ ] **R/C** Preserve external-result pointers and persistence classification (TF-1 §5.2.17, p.40; TF-2b §2.2.4.15, p.22; B.3.12, p.77).
  Tests: `law_tf2b_c7_10_tf2x_a4_rp_application_hd_profile`, `law_tf2b_c6_obr_11_reflex_g_and_49_result_handling_re`.
  Pointer/classification fields are retained. Durable external-object custody and access/recovery integration are not implemented.
- [x] **R/C** Support enhanced elements outside named options: error detail, comments, grouping, providers, ranges, instance IDs, timing and consumption (TF-2b §2.2.4.18, pp.23–24).
  Tests: `law_tf2b_c5_work_builder_validates_and_keeps_truncatable_notes`, `law_tf2b_c7_obx_21_available_ei_identity_and_29_type_subset`, `law_tf2b_c13_tcd_8_required_text_and_9_nested_cq_limits`.
  Other error/provider/grouping/timing cases are in law_control.rs and law_order.rs.
- [ ] **O/C** Related observations: if selected, send supplied related observations to supporting peers (TF-1 §5.2.18, p.40; TF-2b §2.2.4.14, p.22).
  Tests: `law_tf2b_c7_obx_5_ce_ei_na_components_and_lengths`.
  Only EI value shape is covered. Related-observation sending and cross-observation reference resolution are not implemented.
- [ ] **O/C** Pooling on Analyzer: if selected, request authorized pool size and interpret actual size (TF-1 §5.2.19, p.40; TF-2b §2.2.4.16, pp.22–23).
  Tests: `law_tf2b_c13_tcd_9_re_cq_ucum_units_and_10_pool_size`.
  Pool-size field validation only; no authorized pooling workflow/build support.
- [ ] **O/C** Pooling outside Analyzer: if selected, send pooled-specimen characteristics with restricted patient/specimen structures and required echoes (TF-1 §5.2.20, p.41; TF-2b §2.2.4.17, p.23).
  Tests: `law_tf2b_c12_spm_3_13_pooling_outside_r_x_count_and_role`, `law_tf2b_c11_c12_remaining_location_lengths_and_pool_receive_predicates`.
  Outside-pooling field/receive predicates only; no complete pooling workflow.
- [ ] **O** Declare any security/cross-profile groupings separately; LAW itself lists no required actor grouping (TF-1 §§5.3,5.5–5.6, pp.41,54).
  Planned test (not implemented): `law_tf1_5_3_security_grouping_declaration`. No LAW security/grouping declaration is claimed.
- [ ] **R, project gate** Trace requirements/field predicates to tests and qualified simulator evidence; include lost ACK, restart, storage failure, cancellation race and selected-option cases.
  Planned test (not implemented): `law_gazelle_rev11_manager_recorded_scenarios`. No qualified simulator run or full cancellation-race recording exists. Local program tests now cover restart, storage failures and cancellation outcomes in the matrix below; those tests do not close this simulator/recording gate.

## Extra acceptance cases

Add explicit acceptance cases for: result counts differing from message/topic counts; characters split across reads; socket EOF; remote test-tool access to a SUT; changing validation profiles; one saved downstream fragment versus custody of the complete acknowledged message; duplicate observations after restart; uncertain order acceptance; and external-result pointers whose data has not been retained. General safety rules already cover some risks, but phase gates need concrete protocol cases.

## Remaining acceptance work

Recorded message-level Gazelle validation passes all eight Analyzer Manager
messages and found a SAC-3/SAC-10 defect that was fixed. This is not a recorded
simulator conversation; see [validation evidence](../../conformance/gazelle/README.md).

- Recorded simulator conversation: not performed. Message-level validation is
  recorded separately in [Gazelle reports](../../conformance/gazelle/README.md). Evidence
  stays documented. Planned test: `law_gazelle_rev11_manager_recorded_scenarios`.
- Complete capability/option/profile negotiation is absent. Explicit peer options
  and the single persistent-link scope have named tests below; automatic full-profile
  negotiation remains. Planned test: `law_negotiated_options_and_default_query_mode`.
- Full cancellation-race qualification remains. Work outcomes, restart, cancellation, durable receiving, duplicate/C
  delivery, deadlines and storage failures now have the named program matrix below.
  Planned broader qualification: `law_host_durable_lifecycle_recovery`.
- Related-observation sending/reference resolution, full pooling workflows and
  external-object retention/access/recovery: absent. Planned tests:
  `law_related_observation_work_roundtrip`, `law_pooling_workflow_roundtrip` and
  `law_external_object_custody_before_ack`.

These are planned names, not executable tests. Existing generic transport/store
checks do not close these LAW acceptance gaps. No row claims real-analyzer proof.

## Program configuration and result custody

Configuration selects `law_manager`, MLLP, HL7 2.5.1 and explicit peer options.
The runtime retains LAB-29 bytes before interpretation and positive ACK; its
source-named tests are indexed below. Unsupported queries return an error.

## Query-triggered work

Evidence remains **documented**. The program uses the existing LAW query/work
views and builders, shared order cache and durable per-AWOS item path.
Send-ahead and cancellation reuse that path. Both link topologies have local tests; simulator qualification remains unchecked.

- [x] Store raw QBP before lookup/response. Test: `law_tf2b_3_27_4_adr0003_raw_commit_precedes_query_response_and_reservation`.
- [x] Support configured container/SampleId and analyzer-specific all-pending queries, with shared trust/freshness and bounded coherent paging. Tests: `law_tf2b_3_27_4_all_pending_pages_filter_other_analyzers_and_keep_every_awos`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.
- [x] Unsupported known forms use QAK AR, never no-work. Test: `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work` (rack, tray, isolate/parent and informational rack). Isolate/parent still needs app-supplied identities in a future contract version.
- [x] Fresh trusted empty builds the existing restricted negative LAB-28 without items; its ORL is correlated/retained and audited, with no order changes. Tests: `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items`, `law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects`.
- [x] Cache failure, staleness, absent/incomplete snapshot, unreachable app, restore pause and elapsed query budget are errors, not empty. Tests: `law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work`, `law_tf2b_3_27_4_unreachable_app_without_snapshot_never_answers_no_work`, `law_tf2b_3_27_4_stale_and_incomplete_cache_with_retained_orders_are_not_empty`, `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage`.
- [x] Included work is preflighted and reserved through shared per-test items before positive RSP/work output; one item is one AWOS. Preparation failure is not a partial positive response. Tests: `law_tf2b_3_27_3_28_preflight_and_reservation_failure_cannot_emit_positive_rsp_or_work`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.
- [x] ORL bytes are durable before per-AWOS OK/UA reaches `finish_order_items`. Matching uses AWOS IDs, including reordered replies; no reply/disconnect/invalid correlation becomes Uncertain, with no resend. Tests: `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`, `law_tf2b_3_28_query_followup_timeout_disconnect_and_bad_correlation_are_uncertain`.
- [x] Supplied specimen/priority work content is intersected with explicit peer support; repeat/reflex permissions are never generated. Test: `law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support`.

These tests live in `crates/analyte/tests/law/queries/`, compiled in
`tests/delivery.rs`, and use synthetic peers, a temporary store and the reference
receiver. The store read has its own two tests in `crates/core/store/src/order_tests.rs`:
`law_tf2b_3_27_4_pending_pages_filter_targets_without_false_empty_and_never_replay`
and `law_tf2b_3_27_4_sample_and_pending_share_freshness_generation_and_restore_trust`.
The one new public read, `pending_orders`, shares the existing age predicate with
SampleId lookup and reservations, and shares the restore-aware lookup gate.
No schema or contract changed.

Config requires explicit `[law.work]` bindings plus shared `[orders]` translations.
Configuration permits explicit send-ahead; test: `crates/core/config/tests/law.rs::law_tf2b_3_27_3_28_work_requires_explicit_bindings_and_allows_send_ahead`.
Results-only configuration still returns a query error without work.

Small pure-code corrections use the LAW syntax entry point in both LAB-28
builders (C.4's 50-character control ID) and send only C.11-supported EI.1 in
negative SAC fields. The complete QPD is still echoed in RSP and retained raw.
Tests: `law_tf2b_c4_work_and_negative_builders_keep_fifty_character_control_ids`,
`law_tf2b_3_28_4_negative_lab28_for_genuine_no_work`, and the extended
`law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup` for QAK AR.
Source: IHE PaLM TF-2b Rev 11, 3.27/3.28, B.7.7 and Appendix C.

The send-ahead and cancellation behavior below shares this query path.


## Send-ahead and cancellation

Evidence remains **documented**. These tests use synthetic peers, not Gazelle or real analyzers. Tests below
live in `crates/analyte/tests/law/work.rs`, compiled under `tests/delivery.rs`, and
run the program with temporary stores and the reference receiver.

- [x] Reserve one shared item per AWOS before unsolicited OML; match ORL by retained AWOS ID through the same `finish_order_items` handler as queries (TF-2b 3.28.4, C.8). Test: `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos` (OK/UA/mixed, including reversed replies and restart without resend).
- [x] Use LAW CA control and original stored AWOS IDs for app-requested cancellation; CR confirms per item and UC reports refusal, preserving prior item state (TF-2b 3.28.4, C.8). Test: `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos` (CR/UC/mixed; each version admitted once, including after refusal and restart).
- [x] Query-triggered and unsolicited work reuse reservation, rendering, writing, exchange and ORL handling; queried work can be cancelled with send-ahead disabled (TF-2b 3.27/3.28). Test: `law_tf2b_3_27_3_28_queried_work_uses_the_same_cancel_exchange_without_send_ahead`.
- [x] Missing ORL or disconnect produces Uncertain for original work and cancellation, with no replay (TF-2b 3.28.4/B.7; ADR 0003 rule 8). Test: `law_tf2b_3_28_adr0003_no_reply_and_disconnect_are_uncertain_never_replayed`.
- [x] Restart during Sending or admitted CancelRequested recovers retained items as Uncertain, never repeats work or cancel bytes (ADR 0003 rule 8). Test: `law_tf2b_3_28_adr0003_restart_during_sending_or_cancelling_is_uncertain_without_resend`. The shared startup recovery now covers both states.
- [x] Shared restore pause blocks both new work and cancel admission (ADR 0003, TF-2b 3.28.4). Test: `law_tf2b_3_28_adr0003_restore_pause_blocks_send_ahead_and_cancellation`. Cancellation still rechecks the fresh trusted feed inside the shared transaction.
- [x] An unmapped command is retained as Rejected without Sending or bytes (TF-2b 3.28.4; ADR 0003). Test: `law_tf2b_3_28_adr0003_unmapped_send_is_rejected_before_reservation_or_bytes`.
- [x] Independent directional connections and per-transaction outbound lifetime (TF-2b B.6, pp.79-80). Tests: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`. Query follow-up ownership leaves the inbound session before a positive RSP; the outbound owner waits for RSP issuance, then dials once. Disconnect, failed connection, cancellation or expiration settles owned reservations without resend. Persistent mode remains selectable.
- [ ] Complete LAW actor qualification: remaining enhanced work groups/options and a recorded Gazelle run. Receive/send/query/ORL deadlines and both topologies have source-named local tests.

The runtime uses the shared order store (schema 9) and operator controls.


## Link topologies

Evidence remains **documented**. `law.connection_mode = "persistent"` keeps all three transactions on one configured TCP client/server link. `independent_per_transaction` requires `transport.kind = "tcp_server"` and an explicit `law.outbound_endpoint` for the analyzer's LAB-28 listener. The site endpoint is the LAB-27/LAB-29 listener. Neither direction needs its opposite connection to be established first. The host accepts at most four concurrent inbound sessions and owns one outbound transaction at a time, with at most eight queued follow-ups. All sessions, queued reservations and recording taps have an owner and are settled on shutdown. Peers may retain an inbound connection or close after each transaction; the host opens and closes its own outbound link per LAB-28 attempt. No failed or expired command is automatically repeated.

- [x] Both topology choices, required independent listener role/destination, and rejected unused/invalid destinations. Test: `crates/core/config/tests/law.rs::law_tf2b_b6_declared_topologies_require_explicit_direction_and_outbound_endpoint`.
- [x] Both roles validate without startup I/O. Test: `crates/analyte/tests/receive.rs::law_tf2b_b6_single_persistent_manager_accepts_client_and_server_without_startup_io`.
- [x] All three transactions share one persistent connection in both configured directions (B.5 and the declared B.6 subset). Test: `crates/analyte/tests/law/connections.rs::law_tf2b_b5_b6_declared_single_persistent_link_carries_all_transactions_in_both_directions`.
- [x] Client disconnect/first-byte timeout retains evidence, reconnects and never replays reserved work. Test: `crates/analyte/tests/law/connections.rs::law_tf2b_b6_adr0003_client_partial_deadline_retains_exact_bytes_across_reconnect`.
- [x] Independent query/result and work links, peer closure, concurrent inbound handling, restart without replay, restore pause and original cancellation identities. Tests: `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos`, `law_tf2b_b6_independent_crash_awaiting_orl_recovers_uncertain_without_resend`.
- [ ] Gazelle qualification: record tool build, profile revision, actor/options, scenarios and validation results. Local synthetic tests are not simulator evidence.
- [ ] Full actor/workflow qualification: remaining enhanced content/groups, pooling/relationships and complete workflows remain as listed above. Isolate/parent needs app-supplied identities in a future contract; rack/tray forms are still explicitly rejected.
- [x] Shared stored-byte reprocessing (ADR 0003 rules 2-4; TF-2b 3.29.4). The shared reprocessing path uses LAB-29 calls `evidence::interpret(store, machine, id)`; `machine/interpretation.rs` selects the LAW parser/views/mapper. Test: `law_tf2b_3_29_4_adr0003_parked_result_reprocesses_original_after_mapping_fix`. It repairs a mapping, preserves original raw/config and interpretation version 1, appends audited versions, resolves parking, delivers once and creates no second delivery or machine traffic on repeated reprocessing. Reprocessing a retained non-C conflict adds one failed version/audit and no revisions or deliveries. A 50-character LAW control ID prevents generic-HL7 fallback.

### Review pass 4 regression evidence

Evidence remains documented; no open qualification row is promoted.

- L8: `law_tf2b_c9_c10_builder_and_receive_validate_profile` now compares emitted PID/PV1 values and absent demographics. `law_tf2b_c11_c12_work_builder_validates_container_details_and_option_fields` compares each supplied SPM/SAC detail and omitted optional details, including unchanged volume units and dilution.
- L9: `law_tf2b_3_28_4_adr0003_orl_commit_precedes_each_item_outcome` holds raw ORL custody while every item remains Sending, then checks mixed indexed outcomes. `law_tf2b_3_28_4_adr0003_failed_orl_write_never_finishes_items` rejects the write and requires every item Uncertain. The preflight/reservation failure case now fails the second item; the failed query follow-up case requeries after restart without reissuing the original AWOS. `gates/lookup.rs::law_tf2b_3_27_4_lookup_and_reservation_commit_precede_first_response_byte` observes durable QBP at lookup entry and both committed Sending items before the first response byte. `law_tf2b_3_28_4_cancel_rechecks_trust_after_candidate_selection` invalidates trust after selection, waits for transactional admission to return, and checks that no CancelRequested state or bytes appeared.
- L10: `law_tf2b_3_27_4_pending_aggregate_bound_never_returns_partial_work` exceeds the aggregate bound and requires AE, no positive work and no reservations. `gates/paging.rs::law_tf2b_3_27_4_competing_feed_during_pages_refuses_instead_of_mixing` holds collection after the first of multiple pages, queues a generation-changing feed request on the same store owner, and requires AE with zero reservations rather than mixed or false-empty output. It also compares all 101 resulting cache sample IDs exactly.
- L11: negative follow-up tests compare exact retained ORL frames and audit actor, correlation, acceptance and both receipt targets. `law_tf2b_3_29_4_adr0003_parked_result_reprocesses_original_after_mapping_fix` checks audit action, actor, reason and receipt/version targets for success, repeated interpretation and conflict rollback.
- L12: the specimen/priority intersection test also enables peer repeat options and asserts empty TCD-6/7/8 and ORC-8 without app authorization.
- L13: independent successful query, negative work and send-ahead peers remain open until manager-owned EOF. Send-ahead is exercised before any inbound establishment. `gates/handover.rs::law_tf2b_b6_rsp_issuance_precedes_outbound_dial` holds transferred ownership before any RSP byte and again after RSP issuance, requiring no dial until release. `law_tf2b_b6_shutdown_during_handover_settles_all_without_reissue` and `law_tf2b_b6_queued_followup_expiry_settles_all_without_reissue` use a worker-received gate with injected queued deadline expiry, or shutdown while ownership is held, and check every reserved AWOS becomes Uncertain and retains its identity across restart/requery, which sends only negative work.
- L14: `law_tf2b_b6_adr0003_client_partial_deadline_retains_exact_bytes_across_reconnect` selects the exact partial frame, its receipt and incomplete-block parking reason with an unrelated receipt present, then checks the same receipt after restart and absence of replay. It waits for the exact frame and linked parking reason rather than inferring custody from EOF. `law_tf2b_b6_adr0003_wire_deadline_does_not_cancel_raw_custody` holds custody across wire expiry and requires the original frame to survive without positive output.

### Program end-to-end evidence index

All tests below run with temporary protected stores and the repository reference
receiver. Tests name their TF-2b sections/ADR boundaries. Each index row concerns
only the stated behavior; unchecked actor and simulator gates stay open.

| Behavior | Named program test |
| --- | --- |
| Parked LAW result reprocessed after mapping repair, original retained, repeat produces no delivery | `law_tf2b_3_29_4_adr0003_parked_result_reprocesses_original_after_mapping_fix` |
| LAB-29 store before ACK, split input, AA only after interpretation | `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack` |
| LAB-29 storage failure and near-full disk never AA | `law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa`, `law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa` |
| Delivered once, retransmission no delivery, explicit C new revision | `law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c` |
| Uninterpretable retained with error ACK | `law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery` |
| Unsolicited explicit identity, ambiguous data parked | `law_tf2b_3_29_4_unsolicited_uses_explicit_analyzer_order_not_invented_awos` |
| Query found, one item/AWOS, reordered ORL outcomes | `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome` |
| Trusted empty, restricted negative work, correlated ORL without items | `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items` |
| Untrusted/read failure/restore pause never empty | `law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work` |
| Unreachable without trusted snapshot never empty | `law_tf2b_3_27_4_unreachable_app_without_snapshot_never_answers_no_work` |
| Late query includes first-byte and storage budget | `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage` |
| Unsupported query forms are AR | `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work` |
| Send-ahead accepted/rejected/mixed AWOS; restart without resend | `law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos` |
| No reply/disconnect, including cancellation, becomes Uncertain | `law_tf2b_3_28_adr0003_no_reply_and_disconnect_are_uncertain_never_replayed` |
| Crash during Sending or CancelRequested, no resend | `law_tf2b_3_28_adr0003_restart_during_sending_or_cancelling_is_uncertain_without_resend` |
| Cancel confirmed/refused/mixed per AWOS, retained identities | `law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos` |
| Restore pause blocks send-ahead and cancellation | `law_tf2b_3_28_adr0003_restore_pause_blocks_send_ahead_and_cancellation` |
| Query work can be cancelled with send-ahead disabled | `law_tf2b_3_27_3_28_queried_work_uses_the_same_cancel_exchange_without_send_ahead` |
| Independent query/work/result sockets, closure, restart and restore pause | `law_tf2b_b6_independent_query_followup_survives_inbound_close_and_restarts`, `law_tf2b_b6_independent_failed_link_or_lost_orl_never_replays_on_restart`, `law_tf2b_b6_adr0003_independent_restore_pause_sends_no_work`, `law_tf2b_b6_independent_send_ahead_and_cancel_keep_original_awos` |
| Both TCP roles, all transactions on one persistent link | `law_tf2b_b5_b6_declared_single_persistent_link_carries_all_transactions_in_both_directions` |
| Client query and ORL deadlines use config | `law_tf2b_3_27_3_28_single_client_uses_configured_query_and_orl_deadlines` |
| Client reconnect/deadline retains partial data, no replay | `law_tf2b_b6_adr0003_client_partial_deadline_retains_exact_bytes_across_reconnect` |

LAB-29 cases are in `crates/analyte/tests/law/results.rs`, query cases in
`queries.rs`, send/cancel cases in `work.rs`, and role cases in `connections.rs`.
All compile under `crates/analyte/tests/delivery.rs`. Store trust/paging tests,
config validation and pure profile tests remain listed in earlier rows.


The shared reprocessing path preserves original evidence and never sends machine work.

| State | Source | Restriction |
| --- | --- | --- |
| Open restriction | TF-2b section 3.29.4.1.2.1 | Status-only IP/SC/CA is retained and specifically parked; durable lifecycle registration awaits an ADR. |
