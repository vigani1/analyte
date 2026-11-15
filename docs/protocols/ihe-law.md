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
  Host results and configured queries with their required work/ORL followups now have the step-2/3 matrices below. Unsolicited send-ahead, cancellation, originating-work result correlation and independent directional links remain; this is not complete actor conformance.
- [ ] **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
  Tests: `law_tf2b_3_27_4_query_forms_and_conditional_fields`.
  Pure query forms and broadcast rendering are tested. Host config declares container/all-pending only (`crates/config/tests/law.rs::law_tf2b_3_27_config_bounds_deadlines_and_declares_only_supported_query_forms`); the step-3 host matrix now covers both. Broadcast/send-ahead remains disabled. Isolate/parent needs app-supplied identities in a future contract version; rack/tray/informational-position forms are explicitly rejected by the host, without a no-work claim.
- [x] **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
  Tests: `law_tf2b_2_2_usage_cardinality_length_and_null_rules`, `law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings`.
  Field-profile tests also span law_control.rs, law_patient.rs, law_specimen.rs and law_order.rs; cross-message negotiation is separate.
- [ ] **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
  Tests: `law_tf2b_c4_msh_21_m_ei_required_components_lengths_first_transaction`, `law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results`.
  Explicit content/profile selection and host peer declarations are tested by `crates/config/tests/law.rs::law_tf2b_2_2_config_selects_mllp_251_manager_and_explicit_peer_options`. Query work now selects only supplied basic/specimen/priority content intersected with peer support (`law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support` in the program). Other enhanced host work content and automatic option/profile negotiation remain absent.
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
  All six pure query scopes have synthetic evidence. The running program supports container and all-pending only; known unsupported forms return QAK AR (`law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work`). This checked row is pure view/build support, not a claim of all-form host support.
- [ ] **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
  Tests: `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`.
  Supported builders and per-AWOS responses are covered, including query-triggered host work/ORL (`law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`). Unsolicited send-ahead, cancellation, related-observation and pooling work groups remain outside this checkpoint.
- [x] **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
  Tests: `law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively`.
  Every permitted/refused request-specific control/status pair is checked.
- [x] **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
  Tests: `law_tf2b_2_2_4_2_c11_c12_negative_query_echoes_each_scope_without_invented_ids`.
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
  Host LAB-29 receive/ACK is covered by the step-2 matrix below. Query work is now durably reserved, but validation of returned result content against that originating work is not yet wired; this combined row stays unchecked.
- [x] **R, host boundary** Store the exact LAB-29 MLLP block before interpretation/ACK; AA only after durable interpretation, with no partial batch delivery on a conflict (TF-2b §3.29.4; B.7; ADR 0003).
  Tests in `crates/analyte/tests/law/results.rs`: `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack`, `law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa`, `law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa`, `law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`.
- [x] **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
  Tests: `law_tf2b_3_29_4_unsolicited_null_awos_and_external_reference_are_not_invented`.
  No downloaded AWOS is invented or required for unsolicited observations.
- [x] **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
  Tests: `law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes`, `law_tf2b_c7_obx_18_first_second_later_equipment_ei_rules`.
  Original identities are retained alongside validated fields.
- [x] **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
  Tests: `law_tf2b_c7_same_primary_identity_requires_og_group_sequence`, `law_tf2b_c7_obx_11_closed_status_and_explicit_correction_only`.
  Host evidence: `crates/analyte/tests/law/results.rs::law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c` and `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`. AWOS takes precedence over analyzer order; the complete OG distinguishes runs. Retransmission creates no delivery, changed non-C content conflicts, and explicit C creates a revision. Pure status validation retains the closed LAW status set. Unsolicited explicit analyzer order/authority and unresolved identity are covered by `law_tf2b_3_29_4_unsolicited_uses_explicit_analyzer_order_not_invented_awos`. Restart qualification remains a separate gate.
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
- LAW host original-work correlation, work outcomes, restart and cancellation
  races remain absent. LAB-29 durable receiving, duplicate/correction delivery,
  result deadlines and storage failure now have the step-2 evidence below.
  Planned remaining test: `law_host_durable_lifecycle_recovery`.
- Related-observation sending/reference resolution, full pooling workflows and
  external-object retention/access/recovery: absent. Planned tests:
  `law_related_observation_work_roundtrip`, `law_pooling_workflow_roundtrip` and
  `law_external_object_custody_before_ack`.

These are planned names, not executable tests. Existing generic transport/store
checks do not close these LAW acceptance gaps. No row claims real-analyzer proof.

## Program integration checkpoint: step 1 (superseded by step 2 below)

The configuration layer now selects `law_manager` with MLLP, HL7 2.5.1,
CR segments, strict escapes and store-and-forward responsibility. All peer
capability flags are explicit and feed the existing builders' intersection.
Tests in `crates/config/tests/law.rs`:

- `law_tf2b_2_2_config_selects_mllp_251_manager_and_explicit_peer_options`
- `law_tf2b_2_2_config_rejects_impossible_stack_combinations`
- `law_tf2b_3_27_config_bounds_deadlines_and_declares_only_supported_query_forms`
- `law_tf2b_3_29_4_config_allows_corrections_only_by_explicit_c`
- `law_tf2b_3_27_3_28_config_rejects_non_law_order_layouts`

The supported host query forms are declared as container ID (SampleId lookup)
and all-pending for this analyzer. Isolate/parent and rack/tray-position lookup
are not supported by this integration; they must receive an unsupported-query
error, never no-work. Isolate/parent requires app-supplied identities in a future
contract version. The pure query parser's broader coverage is unchanged.

At step 1 no host checklist row closed: activation was refused pending host
transactions. Step 2 replaces that startup guard with the dedicated LAB-29
receiver and keeps query/work execution disabled, as documented below.

## Program integration checkpoint: step 2 — LAB-29 only (historical)

Evidence remains **documented**, using synthetic program/reference-receiver
traffic and temporary protected stores. No Gazelle or real-machine qualification.
The existing pure LAW views/ACK builders and shared revision/outbox path are
reused. UTF-8 is enforced by config (C.4); the dedicated result runtime accepts
only analyzer-initiated TCP server links. Query/work connections and work
lifecycle are not enabled in this step.

Tests in `crates/analyte/tests/law/results.rs` (compiled under `tests/delivery.rs`):

| Behavior/source | Test |
| --- | --- |
| Raw durable before interpretation/ACK, split input; §3.29.4, ADR 0003 | `law_tf2b_3_29_4_adr0003_raw_commit_precedes_interpretation_and_ack` |
| Raw/interpretation storage failures cannot send AA; §3.29.4, ADR 0003 | `law_tf2b_3_29_4_adr0003_storage_failures_never_send_aa` |
| Nearly full disk refuses visibly, no AA; §3.29.4, ADR 0003 | `law_tf2b_3_29_4_adr0003_nearly_full_disk_closes_without_aa` |
| Delivery, retransmission, explicit C revision; §3.29.4, B.3 | `law_tf2b_3_29_4_b3_results_deliver_once_retransmit_and_correct_only_c` |
| Full OG/run identity, long control ID, rejected batches register neither revisions nor delivery; accepted later retries still deliver; §3.29.4, B.3/B.7 | `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs` |
| Unsolicited explicit analyzer identity; ambiguous identity parked; §3.29.4 | `law_tf2b_3_29_4_unsolicited_uses_explicit_analyzer_order_not_invented_awos` |
| Unknown test/bad status AE, inconsistent TCD AR; bytes retained, no delivery; §3.29.4, B.7 | `law_tf2b_3_29_4_b7_uninterpretable_is_parked_with_error_ack_and_no_delivery` |
| Partial block deadline retains original without AA; §3.29.4, MLLP R1 | `law_tf2b_3_29_4_mllp_partial_block_deadline_parks_original_without_aa` |
| Disabled queries get error RSP, no work followup; work traffic retained/closed; §§3.27–3.28 | `law_tf2b_3_27_3_28_disabled_query_and_work_cannot_issue_work_or_acceptance` |

`crates/config/tests/law.rs::law_tf2b_3_29_4_b3_config_cannot_replace_awos_or_truncate_og_identity`
pins LAW identity selections. `crates/hl7/tests/law.rs::law_tf2b_c1_unknown_error_location_is_omitted_never_invented`
tests the small `AckError::unlocated` API: LAW omits optional ERR-2 when a failing
occurrence cannot be proven, rather than inventing one. Full causes remain in
protected interpretation evidence. Generic HL7 still requires a location.

Uninterpretable/mapping-unsupported or ambiguous content is parked with AE/AR.
Syntax without a trustworthy response header, unsupported work/response traffic,
and storage failure close without positive acceptance. LAB-27 cache lookup,
the all-pending store API, LAB-28, original-work correlation, directional work
links, cancellation/restart, full supplemental relationships and Gazelle remain.
No step-3 implementation is included.

## Program integration checkpoint: step 3 — LAB-27 and query-triggered LAB-28

Evidence remains **documented**. The program uses the existing LAW query/work
views and builders, shared order cache and durable per-AWOS item path. It does
not implement unsolicited send-ahead or cancellation in this step. Independent
LAW connection direction/topology and simulator qualification remain unchecked.

- [x] Store raw QBP before lookup/response. Test: `law_tf2b_3_27_4_adr0003_raw_commit_precedes_query_response_and_reservation`.
- [x] Support configured container/SampleId and analyzer-specific all-pending queries, with shared trust/freshness and bounded coherent paging. Tests: `law_tf2b_3_27_4_all_pending_pages_filter_other_analyzers_and_keep_every_awos`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.
- [x] Unsupported known forms use QAK AR, never no-work. Test: `law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work` (rack, tray, isolate/parent and informational rack). Isolate/parent still needs app-supplied identities in a future contract version.
- [x] Fresh trusted empty builds the existing restricted negative LAB-28 without items; its ORL is correlated/retained and audited, with no order changes. Tests: `law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items`, `law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects`.
- [x] Cache failure, staleness, absent/incomplete snapshot, unreachable app, restore pause and elapsed query budget are errors, not empty. Tests: `law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work`, `law_tf2b_3_27_4_unreachable_app_without_snapshot_never_answers_no_work`, `law_tf2b_3_27_4_stale_and_incomplete_cache_with_retained_orders_are_not_empty`, `law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage`.
- [x] Included work is preflighted and reserved through shared per-test items before positive RSP/work output; one item is one AWOS. Preparation failure is not a partial positive response. Tests: `law_tf2b_3_27_3_28_preflight_and_reservation_failure_cannot_emit_positive_rsp_or_work`, `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`.
- [x] ORL bytes are durable before per-AWOS OK/UA reaches `finish_order_items`. Matching uses AWOS IDs, including reordered replies; no reply/disconnect/invalid correlation becomes Uncertain, with no resend. Tests: `law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome`, `law_tf2b_3_28_query_followup_timeout_disconnect_and_bad_correlation_are_uncertain`.
- [x] Supplied specimen/priority work content is intersected with explicit peer support; repeat/reflex permissions are never generated. Test: `law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support`.

These thirteen tests live in `crates/analyte/tests/law/queries.rs`, compiled in
`tests/delivery.rs`, and use synthetic peers, a temporary store and the reference
receiver. The store read has its own two tests in `crates/store/src/order_tests.rs`:
`law_tf2b_3_27_4_pending_pages_filter_targets_without_false_empty_and_never_replay`
and `law_tf2b_3_27_4_sample_and_pending_share_freshness_generation_and_restore_trust`.
The one new public read, `pending_orders`, shares the existing age predicate with
SampleId lookup and reservations, and shares the restore-aware lookup gate.
No schema or contract changed.

Config requires explicit `[law.work]` bindings plus shared `[orders]` translations;
`send_ahead = true` is rejected. Test: `crates/config/tests/law.rs::law_tf2b_3_27_query_work_requires_explicit_bindings_and_rejects_send_ahead`.
Results-only configuration still returns a query error without work.

Small pure-code corrections use the LAW syntax entry point in both LAB-28
builders (C.4's 50-character control ID) and send only C.11-supported EI.1 in
negative SAC fields. The complete QPD is still echoed in RSP and retained raw.
Tests: `law_tf2b_c4_work_and_negative_builders_keep_fifty_character_control_ids`,
`law_tf2b_3_28_4_negative_lab28_for_genuine_no_work`, and the extended
`law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup` for QAK AR.
Source: [IHE PaLM TF-2b Rev 11, 3.27/3.28, B.7.7 and Appendix C](https://www.ihe.net/uploadedFiles/Documents/PaLM/IHE_PaLM_TF_Vol2b.pdf).

Remaining: unsolicited send-ahead and cancellation (step 4), returned-result
validation against durable originating work, independent directional links,
full enhanced work workflows, restart qualification and Gazelle. Step 4 was not
started. Earlier historical checkpoint gaps above are superseded only by the
explicitly named step-3 evidence; unchecked actor/conformance gates stay open.
