# IHE LAW (PaLM Technical Framework Rev 11.0)

Evidence level: documented, then simulated (IHE Gazelle). Phase 4.

Conformance checklist, written in our own words from the standard and checked
in an independent review on 2026-11-15. Field-profile implementation now covers
all five Appendix C groups. Evidence is still **documented**, not simulated.
Section/page references identify sources; no source tables or examples are copied.

Checked rows mean their stated pure-code behavior has named local tests. An
unchecked row can have partial evidence; its remaining work is stated explicitly.
A planned test is not evidence and does not exist yet. Even a checked field row
does not establish simulator, host integration or overall actor conformance.
The crate README records exact fields and source discrepancies. Test names below
are in `crates/hl7/tests/` unless a different path is given.

## Scope decisions to confirm at phase start

- **Actor:** Analyte implements the **Analyzer Manager** (it receives
  results and distributes work the lab app authorized). Analyzer behavior
  lives only in the test peer. Being the manager does not make Analyte
  take clinical decisions: reruns and reflexes are only sent when the app
  authorized them.
- **Sources:** PaLM TF Vol 1 and 2b, the PaLM TF-2x volume (§2.2.4), the IHE
  Technical Frameworks General Introduction (Appendix E) and HL7 v2.5.1
  chapters 2, 2A, 3, 4, 7 and 13. Vol 2a and Vol 3 are not needed for LAW
  alone.
- **Done:** every manager requirement and selected option in this list has
  tests, plus a recorded Gazelle run (tool build, validation profile
  revision, actor, options, scenarios). A simulator pass alone is not
  completeness.

## Checklist

Legend: **R** required; **O** selectable manager option; **C** conditional obligation; **S** recommendation/SHOULD. Requirements below are grouped; expand the cited schemas into tests for each manager-column field/component predicate. This is not a replacement for normative tables.

### Actor/profile/connection

- [ ] **R** Support LAB-27 responder, LAB-28 initiator and LAB-29 responder; preserve unique AWOS and work linkage (TF-1 §§5.1–5.1.1, pp.33–34; §5.4.1, pp.41–42).
  Tests: `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`.
  Pure transaction APIs only; LAW host routing, durable AWOS correlation and connection integration remain.
- [ ] **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
  Tests: `law_tf2b_3_27_4_query_forms_and_conditional_fields`.
  Query forms and broadcast rendering are tested. Default query-mode configuration/negotiation remains untested.
- [x] **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
  Tests: `law_tf2b_2_2_usage_cardinality_length_and_null_rules`, `law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings`.
  Field-profile tests also span law_control.rs, law_patient.rs, law_specimen.rs and law_order.rs; cross-message negotiation is separate.
- [ ] **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
  Tests: `law_tf2b_c4_msh_21_m_ei_required_components_lengths_first_transaction`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`.
  Explicit content/profile selection is tested; automatic option/profile negotiation is absent.
- [x] **S** Send enhanced data only to supporting peers (TF-1 §5.1.1.2, p.34).
  Tests: `law_tf2b_c11_c12_explicit_profile_cannot_enlarge_peer_send_capabilities`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.
  Outgoing supported builders intersect explicit content selection with peer capabilities.
- [ ] **R** Use directional MLLP initiating connections; tolerate persistent/per-transaction connections and independent establishment (TF-2b B.6, pp.79–80; TF-2x §2.2.4 dependency).
  Tests: `mllp_r1_1_2_roundtrip_chunk_boundaries`.
  Partial transport evidence in crates/framing/tests/codecs.rs. LAW initiating direction, independent links and persistent/per-transaction host sessions still need integration tests.
- [x] **R** Use NE/AL application acknowledgement mode and correct header reversal/MSA-2 correlation; do not reject incoming application/facility names merely for differing from configured values (TF-2b B.5, pp.78–79; HL7 §2.9, pp.2-24–2-28, §2.15.8.2).
  Tests: `law_tf2b_c4_msh_15_16_c_m_x_predicate_is_trigger_versus_response`, `law_tf2b_c4_c3_b5_receive_query_reply_reversal_correlation_ignored_content`.
  Pure header/acknowledgment policy; host storage evidence is a separate gate.

### Query/work

- [x] **R** Parse QBP and return correctly correlated RSP, distinguishing acceptance/error from actual work availability (TF-2b §3.27.4, pp.34–44).
  Tests: `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`.
  Tests distinguish genuine empty work from unavailable lookup.
- [x] **R/C** Handle container, isolate, rack/position, tray/position, all-pending and informational location forms as applicable to connected peers (TF-1 §§5.2.1–5.2.5, pp.36–37; TF-2b QPD definition, pp.35–41).
  Tests: `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_2_2_4_2_c11_c12_negative_query_echoes_each_scope_without_invented_ids`.
  All six supported query scopes have synthetic evidence.
- [ ] **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
  Tests: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`.
  Supported builders and per-AWOS responses are covered. Related-observation and pooling work groups are not built.
- [x] **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
  Tests: `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`.
  Every permitted/refused request-specific control/status pair is checked.
- [ ] **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
  Tests: `law_tf2b_2_2_4_2_c11_c12_negative_query_echoes_each_scope_without_invented_ids`.
  All restricted negative-query forms are covered; analyzer-specific host deadline remains an integration gap.
- [ ] **R** Preserve uncertain prior acceptance; duplicate-AWOS UA must not undo the original accepted request (TF-2b §3.28.4.1.3, p.51).
  Tests: `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`.
  Parser decisions preserve uncertainty; persistent duplicate/retry/lifecycle integration remains.
- [x] **R** Use separate AWOS for separately requested replicates (TF-2b B.8, p.83).
  Tests: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.
  Separate supplied AWOS identities are required; renderer does not create replicate identities.

### Results/schema/errors

- [ ] **R** Receive OUL and return ACK with required structures/fields; correlate known AWOS to originating work (TF-2b §3.29.4, pp.53–58).
  Tests: `law_tf2b_3_29_4_ack_r22_requires_complete_interpretation_and_durability`.
  Pure OUL/ACK field and responsibility checks exist. Correlation to a durable originating-work store remains host work.
- [x] **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
  Tests: `law_tf2b_3_29_4_unsolicited_null_awos_and_external_reference_are_not_invented`.
  No downloaded AWOS is invented or required for unsolicited observations.
- [x] **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
  Tests: `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`, `law_tf2b_c7_obx_18_first_second_later_equipment_ei_rules`.
  Original identities are retained alongside validated fields.
- [ ] **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
  Tests: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`, `law_tf2b_c7_obx_11_closed_status_and_explicit_correction_only`.
  Identity and declared status are covered; persistent retransmission/revision reconciliation remains a host obligation.
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
  Tests: `law_tf2b_b7_trigger_rejection_and_query_processing_errors_have_distinct_actions`, `law_tf2b_b7_invalid_ack_and_inconsistent_orl_fail_without_ack_loop`, `law_tf2b_c7_remaining_component_boundaries_and_located_failures`.
  Failures preserve original input and carry located content errors.
- [x] **R** Reject inconsistent LAB-29 as a whole, including TCD-1/OBX-3 mismatch; retain bytes without applying a subset (TF-2b p.57; B.7.6, p.82).
  Tests: `law_tf2b_b7_6_inconsistent_tcd_rejects_whole_message_and_retains_original`.
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
  Tests: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`.
  Run identifiers and ambiguity rejection are tested; persistent run/retransmission reconciliation remains.
- [ ] **R/C** Encode app-authorized manager rerun/reflex as new AWOS with repeat status (TF-1 §5.2.11, pp.38–39; TF-2b §2.2.4.8, p.20).
  Tests: `law_tf2b_c13_tcd_6_7_r_permissions_8_r_repeat_status`, `law_tf2b_c8_c6_c13_work_builder_preserves_supplied_event_and_validates_details`.
  Explicit repeat status and unique supplied AWOS rendering are tested. Host authorization and durable new-work lifecycle still need integration evidence.
- [x] **R/C** Encode authorized automatic-repeat/reflex permissions for supporting peers (TF-1 §5.2.12, p.39; TF-2b §2.2.4.9, p.20).
  Tests: `law_tf2b_c13_tcd_6_7_r_permissions_8_r_repeat_status`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`.
  No permission defaults are synthesized.
- [x] **R/C** Preserve requested and applied priority (TF-1 §5.2.13, p.39; TF-2b §2.2.4.10, pp.20–21).
  Tests: `law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`.
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
  Tests: `law_tf2b_c7_obx_5_rp_nested_hd_limits_and_no_pointer_fetch`, `law_tf2b_c6_obr_11_reflex_g_and_49_result_handling_re`.
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
  Planned test (not implemented): `law_gazelle_rev11_manager_recorded_scenarios`. No qualified simulator run or LAW-specific restart/storage-failure/cancellation-race recording exists.

## Extra acceptance cases

Add explicit acceptance cases for: result counts differing from message/topic counts; characters split across reads; socket EOF; remote test-tool access to a SUT; changing validation profiles; one saved downstream fragment versus custody of the complete acknowledged message; duplicate observations after restart; uncertain order acceptance; and external-result pointers whose data has not been retained. General safety rules already cover some risks, but phase gates need concrete protocol cases.

## Remaining acceptance work

- Qualified Gazelle run with retained reports/recordings: not performed. Evidence
  stays documented. Planned test: `law_gazelle_rev11_manager_recorded_scenarios`.
- Complete capability/option/profile negotiation and default connection mode:
  absent. Planned test: `law_negotiated_options_and_default_query_mode`.
- LAW host/dialog integration for durable original-work correlation, duplicate
  acceptance, retries, restart, deadline, storage failure and cancellation races:
  absent. Planned test: `law_host_durable_lifecycle_recovery`.
- Related-observation sending/reference resolution, full pooling workflows and
  external-object retention/access/recovery: absent. Planned tests:
  `law_related_observation_work_roundtrip`, `law_pooling_workflow_roundtrip` and
  `law_external_object_custody_before_ack`.

These are planned names, not executable tests. Existing generic transport/store
checks do not close these LAW acceptance gaps. No row claims real-analyzer proof.
