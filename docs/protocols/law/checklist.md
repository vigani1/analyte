# LAW conformance

## Checklist

Legend: **R** required; **O** selectable manager option; **C** conditional obligation; **S** recommendation/SHOULD. Requirements below are grouped; expand the cited schemas into tests for each manager-column field/component predicate. This is not a replacement for normative tables.

### Actor/profile/connection

- [ ] **R** Support LAB-27 responder, LAB-28 initiator and LAB-29 responder; preserve unique AWOS and work linkage (TF-1 §§5.1–5.1.1, pp.33–34; §5.4.1, pp.41–42).
  Tests: `rsp_separates_no_work_from_failed_lookup`, `work_needs_unique_awos_and_authorized_options`, `results_keep_identity_values_status_and_notes`.
  Host results, queries, send-ahead and cancellation have the program matrices below. Originating-work result correlation and both link topologies have the named host tests below; simulator and complete actor qualification remain.
- [ ] **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
  Tests: `query_forms_select_their_scope`.
  Pure query forms and broadcast rendering are tested. Host config declares container/all-pending only (`crates/core/config/tests/law.rs::bounds_deadlines_and_declares_supported_query_forms`); the query host matrix now covers both. Send-ahead is explicitly selected by `orders.send_ahead` (default query example: false); host evidence: `send_ahead_accepted_rejected_and_mixed` in `crates/analyte/tests/law/work.rs`. Independent broadcast links have the B.6 matrix below. Isolate/parent needs app-supplied identities in a future contract version; rack/tray/informational-position forms are explicitly rejected by the host, without a no-work claim.
- [x] **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
  Tests: `field_rules_check_usage_count_and_length`, `x_fields_and_composite_send_ceilings`.
  Field-profile tests also span law_control.rs, law_patient.rs, law_specimen.rs and law_order.rs; cross-message negotiation is separate.
- [ ] **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
  Tests: `msh21_required_components_and_lengths`, `selected_options_require_details_and_priority`.
  Explicit content/profile selection and host peer declarations are tested by `crates/core/config/tests/law.rs::selects_mllp_manager_and_explicit_peer_options`. Query work now selects only supplied basic/specimen/priority content intersected with peer support (`query_work_limits_content_to_peer_support` in the program). Other enhanced host work content and automatic option/profile negotiation remain absent.
- [x] **S** Send enhanced data only to supporting peers (TF-1 §5.1.1.2, p.34).
  Tests: `profile_cannot_widen_peer_send_capabilities`, `work_needs_unique_awos_and_authorized_options`.
  Outgoing supported builders intersect explicit content selection with peer capabilities.
- [x] **R** Use directional MLLP initiating connections; tolerate peer closure and independent establishment (TF-2b B.6, pp.79-80; TF-2x §2.2.4 dependency).
  Tests: `independent_query_followup_survives_inbound_close`, `independent_failed_link_never_replays_on_restart`, `independent_restore_pause_sends_no_work`, `independent_send_ahead_and_cancel_keep_awos` in `crates/analyte/tests/law/connections.rs`. The existing persistent topology is covered by `single_persistent_link_carries_every_transaction` and `partial_deadline_keeps_exact_bytes_across_reconnect`. Independent mode listens for LAB-27/LAB-29 and opens a separate LAB-28 connection per transaction. Synthetic tests remain documented evidence, not simulator qualification.
- [x] **R** Use NE/AL application acknowledgement mode and correct header reversal/MSA-2 correlation; do not reject incoming application/facility names merely for differing from configured values (TF-2b B.5, pp.78–79; HL7 §2.9, pp.2-24–2-28, §2.15.8.2).
  Tests: `msh15_16_predicate_trigger_versus_response`, `query_reply_correlation_ignores_content`.
  Pure header/acknowledgment policy; host storage evidence is a separate gate.

### Query/work

- [x] **R** Parse QBP and return correctly correlated RSP, distinguishing acceptance/error from actual work availability (TF-2b §3.27.4, pp.34–44).
  Tests: `rsp_separates_no_work_from_failed_lookup`.
  Tests distinguish genuine empty work from unavailable lookup.
- [x] **R/C** Handle container, isolate, rack/position, tray/position, all-pending and informational location forms as applicable to connected peers (TF-1 §§5.2.1–5.2.5, pp.36–37; TF-2b QPD definition, pp.35–41).
  Tests: `query_forms_select_their_scope`, `negative_reply_echoes_sac_identity_values`.
  All six pure query scopes have synthetic evidence. The running program supports container and all-pending only; known unsupported forms return QAK AR (`unsupported_query_forms_get_ar`). This checked row is pure view/build support, not a claim of all-form host support.
- [ ] **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
  Tests: `work_needs_unique_awos_and_authorized_options`, `orl_control_status_pairs_match_request_type`.
  Supported builders and per-AWOS responses are covered, including query-triggered host work/ORL (`found_query_reserves_and_finishes_each_awos`). Host send-ahead and cancellation now share the same path, covered in `crates/analyte/tests/law/work.rs` by `send_ahead_accepted_rejected_and_mixed` and `cancel_confirmed_refused_and_mixed_per_awos`. Related-observation and pooling work groups, broader enhanced work content remain outside the supported scope; both link topologies are covered below.
- [x] **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
  Tests: `orl_control_status_pairs_match_request_type`.
  Every permitted/refused request-specific control/status pair is checked. Host item decisions are covered by `crates/analyte/tests/law/work.rs::cancel_confirmed_refused_and_mixed_per_awos`.
- [x] **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
  Tests: `negative_reply_echoes_sac_identity_values`.
  Host evidence for the selected forms: `trusted_empty_sends_negative_work_without_items`, `query_budget_counts_first_byte_and_storage`, and `bad_negative_followup_orl_changes_no_orders` in `crates/analyte/tests/law/queries.rs`. No items are reserved for negative work. Unsupported forms return an error rather than negative work.
- [ ] **R** Preserve uncertain prior acceptance; duplicate-AWOS UA must not undo the original accepted request (TF-2b §3.28.4.1.3, p.51).
  Tests: `orl_outcomes_are_per_request_not_retractions`.
  Parser decisions preserve uncertainty; persistent duplicate/retry/lifecycle integration remains.
- [x] **R** Use separate AWOS for separately requested replicates (TF-2b B.8, p.83).
  Tests: `work_needs_unique_awos_and_authorized_options`.
  Separate supplied AWOS identities are required; renderer does not create replicate identities.

### Results/schema/errors

- [ ] **R** Receive OUL and return ACK with required structures/fields; correlate known AWOS to originating work (TF-2b §3.29.4, pp.53–58).
  Tests: `ack_r22_needs_storage_and_interpretation`.
  Host LAB-29 receive/ACK is covered by the result matrix below. Returned work is correlated against retained AWOS, specimen, container and test content, including after restart. The remaining actor/structure qualification is open, so this combined row stays unchecked.
- [x] **R, host boundary** Store the exact LAB-29 MLLP block before interpretation/ACK; AA only after durable interpretation, with no partial batch delivery on a conflict (TF-2b §3.29.4; B.7; ADR 0003).
  Tests in `crates/analyte/tests/law/results.rs`: `raw_commit_precedes_interpretation_and_ack`, `storage_failure_never_sends_aa`, `nearly_full_disk_closes_without_aa`, `uninterpretable_result_is_parked_with_error_ack`, `conflict_rejects_batch_and_og_separates_runs`.
- [x] **R, host boundary** Retain empty SC/IP/CA as per-item execution evidence, independently of command and observation state (TF-2b §3.29.4.1.2.1; C.8 Table C.8-9/Figure C.8-1; B.7.6; ADRs 0003, 0011, 0016).
  Protocol/mapping tests: `status_only_ip_sc_ca_become_notices`, `estimate_keeps_spelling_null_and_header_zone`, `status_groups_never_hide_result_groups`.
  Host tests in `crates/analyte/tests/law/work.rs`: `status_notices_handle_repeats_restart_and_conflicts`, `mixed_status_result_is_judged_whole`. Store tests in `crates/core/store/src/orders/machine_orders/tests/custody.rs` cover projection rollback, sparse items, stable cancellation conditions, pending cancel CR/UC, admission refusal and bounded history. Status notices allocate no result/outbox row. Exact receipts and dispositions survive restart. Admin-only handling requires a named responder; contract v1 is unchanged. Official simulator status/cancel-race qualification remains open; this row records documented host implementation, not actor qualification.
- [x] **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
  Tests: `unsolicited_results_invent_no_awos_or_reference`.
  No downloaded AWOS is invented or required for unsolicited observations.
- [x] **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
  Tests: `results_keep_identity_values_status_and_notes`, `obx18_equipment_repetition_rules`.
  Original identities are retained alongside validated fields.
- [x] **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
  Tests: `same_primary_identity_needs_og_sequence`, `repeated_observation_needs_unique_og`, `obx11_closed_statuses_only_c_corrects`.
  Host evidence: `crates/analyte/tests/law/results.rs::results_deliver_once_and_c_corrects` and `conflict_rejects_batch_and_og_separates_runs`. AWOS takes precedence over analyzer order; OG.1 identifies the run and the complete OG distinguishes observations within it (C.7 pp.102–103). Retransmission creates no delivery. ADR 0011 and Figure C.7-1 (p.111) allow P->P/R/F, R->R/F, F->C and C->C for changed content; every other changed-content move conflicts. R is retained without delivery, including across restart before its final. At most one run per order/test may reach F/C; conflicting batches are rejected whole. Tests: `every_permitted_and_forbidden_status_move`, `status_transitions_and_unverified_delivery`, `one_final_run_per_history_or_batch`. Generic store test: `correction_cannot_bypass_status_table`. HL7 v2 and ASTM still use their existing correction/withdrawal rules; their lifecycle tables are not enabled. Pure status validation retains the closed LAW status set. Unsolicited explicit analyzer order/authority and unresolved identity are covered by `unsolicited_result_never_invents_awos`. Restart qualification remains a separate gate.
- [ ] **R** Preserve supplemental/primary relationships, typed values, units, statuses and scoped notes; do not treat NTE prose as executable rules (TF-2b B.3; C.5/C.7, pp.95–113).
  Tests: `obx5_text_keeps_original_values_and_units`, `results_keep_identity_values_status_and_notes`.
  Typed/raw values and note scope are covered. Full cross-message supplemental/primary relationship reconciliation remains.
- [x] **R** Implement every applicable manager usage for ERR, INV, MSA, MSH, NTE, OBR, OBX, ORC, PID, PV1, SAC, SPM, TCD and TQ1, including component usages and conditional predicates (TF-2b Appendix C, pp.84–141).
  Tests: `x_fields_and_composite_send_ceilings`, `order_x_fields_and_composite_length_ceiling`.
  All five field-profile suites provide per-rule positive/negative cases. Read the README source-discrepancy policies; this row is field validation, not complete message/lifecycle conformance.
- [x] **Deliberate tolerance** — MSH-7 without its UTC offset (TF-2b Appendix C.4, Table C.4-6). C.4 requires the offset. When the site file sets `time_zone` (an IANA name or a fixed `+HH:MM` offset), a received MSH-7 with seconds precision and no offset is accepted and read in that zone instead of being refused. The original MSH-7 text is kept unchanged; no stored or delivered time gains an offset. Result receipts get a `law_header_zone_from_site` note (`ZoneFromSite`) in their departure evidence naming the zone and, when the local time has exactly one, its offset; queries and order replies, which have no interpretation record, get an audit entry with the same note and the receipt ID. A local time skipped or repeated by a daylight-saving change is noted as having no single offset, never placed on one side. Without `time_zone`, and for every header Analyte sends, the offset stays required. Reason: the IHE Gazelle simulator and some analyzers send a local MSH-7 without an offset; refusing the whole exchange loses the reply, while the lab's own zone is the agreed reading and every use is visible. Tests: `msh7_offset_or_site_zone`, `zoneless_msh7_uses_site_zone_with_note`, `iana_or_fixed_offset_only`, `daylight_saving_changes_give_no_offset`.
- [ ] **R** Apply coding-system identification, test/result vocabulary and units rules without inventing codes (TF-2b §2.3, pp.24–31).
  Tests: `obx3_alternate_codes_and_namespaces`, `obx6_units_namespace_and_null`.
  Namespaces and unit preservation are tested. Complete vocabulary binding and partner-specific supplemental/reference relationships remain.
- [x] **R** Distinguish malformed-trigger AE, control-content AR, invalid ACK failure, inconsistent ORL failure and query processing rejection; report locations where required (TF-2b B.7.1–B.7.8, pp.80–82).
  Host ERR location test: `second_obx_error_names_actual_occurrence` covers malformed second OBX-11, second OBX-1 sequencing, and malformed first OBX among repeated segments, with raw retention and whole-message refusal. Host test: `host_correlates_errors_without_ack_loops` covers retained malformed queries, syntax errors, unsupported triggers, invalid version/profile and silence for invalid ACK/ORL/RSP. Tests: `trigger_and_query_errors_get_distinct_acks`, `bad_ack_or_orl_fails_without_ack_loop`, `component_boundaries_and_located_failures`.
  Failures preserve original input and carry located content errors.
- [x] **R** Reject inconsistent LAB-29 as a whole, including TCD-1/OBX-3 mismatch; retain bytes without applying a subset (TF-2b p.57; B.7.6, p.82).
  Tests: `inconsistent_tcd_rejects_whole_message`.
  Host revision conflicts return AR (B.7.6, p.82), while mapping failures remain AE. Tests: `results_deliver_once_and_c_corrects`, `conflict_rejects_batch_and_og_separates_runs`. Originating-work checks are covered by `ignored_awos_components_do_not_change_work_match` (B.3.1 pp.63–64; B.7.6 p.82): unknown/nonmatching placer, specimen identity/type, container and requested test reject the entire batch with AR and an ERR location; null-AWOS unsolicited results remain allowed. Prepared outbound work is retained with the reservation before I/O and survives restart. Missing legacy command evidence is rejected for operator review. `original_command_owned_immutable_survives_restart` covers store ownership and immutability.
  No partial Results view escapes a failed message.
- [ ] **R** Keep manager-sourced patient data authoritative; correct downloaded patient information through cancel/new-AWOS, not repeated work (TF-2b B.7.9, pp.82–83).
  Tests: `orl_outcomes_are_per_request_not_retractions`.
  Only request-specific decisions are tested. Persistent patient-authority/cancel-new correction workflow has no LAW integration test yet.
- [x] **R** Handle unknown/non-HL7 input without compromising the application (TF-2b B.7.10, p.83).
  Tests: `malformed_escapes_do_not_bypass_profile_checks`.
  Bounded syntax failure retains the host-owned original; generic parser property tests add arbitrary-input evidence.

### Enhanced content and manager options

These are optional Analyzer capabilities, but relevant manager enhanced-content support is required; outgoing use is conditional on the peer and available app-supplied information. They do not require Analyte to measure or make clinical rerun/reflex decisions.

- [x] **R/C** Preserve contributing substance/disposable identifiers, types and available lot/container data (TF-1 §5.2.6, p.37; TF-2b §2.2.4.3, p.17; C.2).
  Tests: `inv_required_codes_and_lot_identifiers`, `inv4_container_and_inv12_expiry_by_direction`, `control_inventory_co_type_and_ownership`.
  Inventory fields, option handling and ownership are covered without interpreting substance data clinically.
- [x] **R/C** Send explicit requested dilution and retain actual dilution/value meaning (TF-1 §5.2.7, pp.37–38; TF-2b §2.2.4.4, pp.17–18; C.13).
  Tests: `reported_dilution_keeps_original_and_adjustment`, `type_only_dilution_keeps_adjustment_metadata`, `work_builder_keeps_event_time_and_checks_details`.
  Exact factors/types and host-supplied adjustment meaning are retained; no observation is multiplied.
- [x] **R/C** Handle profiled patient/encounter demographics without invented values (TF-1 §5.2.8, p.38; TF-2b §2.2.4.5, pp.18–19).
  Tests: `patient_builder_and_receive_check_profile`.
  Demographic field validation and null/authority bindings are tested; no patient reconciliation is performed.
- [ ] **R/C** Preserve Analyzer reflex parent/child relationships (TF-1 §5.2.9, p.38; TF-2b §2.2.4.6, p.19; B.4, pp.77–78).
  Tests: `orc4_group_and_orc8_reflex_parents`, `obr11_reflex_and_obr49_result_handling`.
  Parent/action fields are validated and retained. Cross-message parent resolution and reflex lifecycle integration remain.
- [ ] **R/C** Preserve Analyzer-initiated reruns and run identifiers (TF-1 §5.2.10, p.38; TF-2b §2.2.4.7, p.19; B.3).
  Tests: `same_primary_identity_needs_og_sequence`, `repeated_observation_needs_unique_og`.
  Run identifiers and ambiguity rejection are tested; persistent run/retransmission reconciliation remains.
- [ ] **R/C** Encode app-authorized manager rerun/reflex as new AWOS with repeat status (TF-1 §5.2.11, pp.38–39; TF-2b §2.2.4.8, p.20).
  Tests: `repeat_status_output_keeps_supplied_code`, `repeat_status_text_matches_code`, `work_builder_keeps_event_time_and_checks_details`.
  Explicit repeat status and unique supplied AWOS rendering are tested. Host authorization and durable new-work lifecycle still need integration evidence.
- [x] **R/C** Encode authorized automatic-repeat/reflex permissions for supporting peers (TF-1 §5.2.12, p.39; TF-2b §2.2.4.9, p.20).
  Tests: `repeat_status_text_matches_code`, `work_needs_unique_awos_and_authorized_options`.
  No permission defaults are synthesized.
- [x] **R/C** Preserve requested and applied priority (TF-1 §5.2.13, p.39; TF-2b §2.2.4.10, pp.20–21).
  Tests: `requested_and_applied_priority_keep_exact_code`, `tq1_priority_code_subset_and_option`, `selected_options_require_details_and_priority`.
  Explicit peer/receive option selection is required; no automatic negotiation is claimed.
- [x] **R/C** Preserve specimen/parent identities and applicable collection/source/risk/time data (TF-1 §5.2.14, p.39; TF-2b §2.2.4.11, p.21).
  Tests: `spm2_spm3_identity_namespaces_and_lengths`, `spm17_spm18_collection_and_receipt_times`.
  Other selected specimen field cases are in law_specimen.rs.
- [x] **R/C** Handle container carrier/volume/units/dilution data (TF-1 §5.2.15, pp.39–40; TF-2b §2.2.4.12, p.21).
  Tests: `work_builder_checks_container_details`, `sac29_dilution_ratio_needs_positive_denominator`.
  Container information remains as supplied; no dilution conversion is performed.
- [x] **R/C** Preserve mass-spectrometry probability (TF-1 §5.2.16, p.40; TF-2b §2.2.4.13, p.22).
  Tests: `obx9_mass_spec_probability_range_and_option`.
  Probability is range-checked without floating-point arithmetic and retained exactly.
- [ ] **R/C** Preserve external-result pointers and persistence classification (TF-1 §5.2.17, p.40; TF-2b §2.2.4.15, p.22; B.3.12, p.77).
  Tests: `obx5_reference_pointer_application_rules`, `obr11_reflex_and_obr49_result_handling`.
  Pointer/classification fields are retained. Durable external-object custody and access/recovery integration are not implemented.
- [x] **R/C** Support enhanced elements outside named options: error detail, comments, grouping, providers, ranges, instance IDs, timing and consumption (TF-2b §2.2.4.18, pp.23–24).
  Tests: `work_builder_keeps_truncatable_notes`, `obx21_instance_id_and_obx29_type_subset`, `tcd8_status_text_and_tcd9_quantity_limits`.
  Other error/provider/grouping/timing cases are in law_control.rs and law_order.rs.
- [ ] **O/C** Related observations: if selected, send supplied related observations to supporting peers (TF-1 §5.2.18, p.40; TF-2b §2.2.4.14, p.22).
  Tests: `obx5_coded_id_and_array_value_limits`.
  Only EI value shape is covered. Related-observation sending and cross-observation reference resolution are not implemented.
- [ ] **O/C** Pooling on Analyzer: if selected, request authorized pool size and interpret actual size (TF-1 §5.2.19, p.40; TF-2b §2.2.4.16, pp.22–23).
  Tests: `tcd9_ucum_quantity_and_tcd10_pool_size`.
  Pool-size field validation only; no authorized pooling workflow/build support.
- [ ] **O/C** Pooling outside Analyzer: if selected, send pooled-specimen characteristics with restricted patient/specimen structures and required echoes (TF-1 §5.2.20, p.41; TF-2b §2.2.4.17, p.23).
  Tests: `pooling_outside_needs_parents_count_and_role`, `location_lengths_and_pool_predicates`.
  Outside-pooling field/receive predicates only; no complete pooling workflow.
- [ ] **O** Declare any security/cross-profile groupings separately; LAW itself lists no required actor grouping (TF-1 §§5.3,5.5–5.6, pp.41,54).
  Planned test: `law_tf1_5_3_security_grouping_declaration` (not implemented). No LAW security/grouping declaration is claimed.
- [ ] **R, project gate** Trace requirements/field predicates to tests and qualified simulator evidence; include lost ACK, restart, storage failure, cancellation race and selected-option cases.
  Planned test: `law_gazelle_rev11_manager_recorded_scenarios` (not implemented). No qualified simulator run or full cancellation-race recording exists. Local program tests now cover restart, storage failures and cancellation outcomes in the matrix below; those tests do not close this simulator/recording gate.

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
  Planned test: `law_host_durable_lifecycle_recovery` (broader qualification).
- Related-observation sending/reference resolution, full pooling workflows and
  external-object retention/access/recovery: absent.
  Planned test: `law_related_observation_work_roundtrip`.
  Planned test: `law_pooling_workflow_roundtrip`.
  Planned test: `law_external_object_custody_before_ack`.

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

- [x] Store raw QBP before lookup/response. Test: `raw_commit_precedes_query_response`.
- [x] Support configured container/SampleId and analyzer-specific all-pending queries, with shared trust/freshness and bounded coherent paging. Tests: `all_pending_pages_filter_other_analyzers`, `found_query_reserves_and_finishes_each_awos`.
- [x] Unsupported known forms use QAK AR, never no-work. Test: `unsupported_query_forms_get_ar` (rack, tray, isolate/parent and informational rack). Isolate/parent still needs app-supplied identities in a future contract version.
- [x] Fresh trusted empty builds the existing restricted negative LAB-28 without items; its ORL is correlated/retained and audited, with no order changes. An ORL after the response deadline is retained and parked for an operator, never audited as a correlated reply. Tests: `trusted_empty_sends_negative_work_without_items`, `bad_negative_followup_orl_changes_no_orders`, `late_negative_followup_orl_is_parked`.
- [x] Cache failure, staleness, absent/incomplete snapshot, unreachable app, restore pause and elapsed query budget are errors, not empty. Tests: `untrusted_or_paused_lookup_is_an_error`, `unreachable_app_never_answers_no_work`, `stale_or_incomplete_cache_is_not_empty`, `query_budget_counts_first_byte_and_storage`.
- [x] Included work is preflighted and reserved through shared per-test items before positive RSP/work output; one item is one AWOS. Preparation failure is not a partial positive response. Tests: `preparation_failure_sends_no_positive_answer`, `found_query_reserves_and_finishes_each_awos`.
- [x] ORL bytes are durable before per-AWOS OK/UA reaches finish_order_items. Matching uses AWOS IDs, including reordered replies; no reply/disconnect/invalid correlation becomes Uncertain, with no resend. Tests: `found_query_reserves_and_finishes_each_awos`, `failed_query_followup_is_uncertain`.
- [x] Supplied specimen/priority work content is intersected with explicit peer support; repeat/reflex permissions are never generated. Test: `query_work_limits_content_to_peer_support`.

These tests live in `crates/analyte/tests/law/queries/`, compiled in
`tests/delivery.rs`, and use synthetic peers, a temporary store and the reference
receiver. The store read has its own two tests in `crates/core/store/src/order_tests.rs`:
`pending_pages_filter_without_false_empty_or_replay`
and `sample_and_pending_queries_share_trust`.
The one new public read, pending_orders, shares the existing age predicate with
SampleId lookup and reservations, and shares the restore-aware lookup gate.
No schema or contract changed.

Config requires explicit `[law.work]` bindings plus shared `[orders]` translations.
Configuration permits explicit send-ahead; test: `crates/core/config/tests/law.rs::work_requires_bindings_and_allows_send_ahead`.
Results-only configuration still returns a query error without work.

Small pure-code corrections use the LAW syntax entry point in both LAB-28
builders (C.4's 50-character control ID) and send only C.11-supported EI.1 in
negative SAC fields. The complete QPD is still echoed in RSP and retained raw.
Tests: `builders_keep_50_character_control_ids`,
`negative_lab28_for_genuine_no_work`, and the extended
`rsp_separates_no_work_from_failed_lookup` for QAK AR.
Source: IHE PaLM TF-2b Rev 11, 3.27/3.28, B.7.7 and Appendix C.

The send-ahead and cancellation behavior below shares this query path.


## Send-ahead and cancellation

Evidence remains **documented**. These tests use synthetic peers, not Gazelle or real analyzers. Tests below
live in `crates/analyte/tests/law/work.rs`, compiled under `tests/delivery.rs`, and
run the program with temporary stores and the reference receiver.

- [x] Reserve one shared item per AWOS before unsolicited OML; match ORL by retained AWOS ID through the same finish_order_items handler as queries (TF-2b 3.28.4, C.8). Test: `send_ahead_accepted_rejected_and_mixed` (OK/UA/mixed, including reversed replies and restart without resend).
- [x] Use LAW CA control and original stored AWOS IDs for app-requested cancellation; CR confirms per item and UC reports refusal, preserving prior item state (TF-2b 3.28.4, C.8). Test: `cancel_confirmed_refused_and_mixed_per_awos` (CR/UC/mixed; each version admitted once, including after refusal and restart).
- [x] Query-triggered and unsolicited work reuse reservation, rendering, writing, exchange and ORL handling; queried work can be cancelled with send-ahead disabled (TF-2b 3.27/3.28). Test: `queried_work_cancels_without_send_ahead`.
- [x] Missing ORL or disconnect produces Uncertain for original work and cancellation, with no replay (TF-2b 3.28.4/B.7; ADR 0003 rule 8). Test: `no_reply_or_disconnect_is_never_replayed`.
- [x] Restart during Sending or admitted CancelRequested recovers retained items as Uncertain, never repeats work or cancel bytes (ADR 0003 rule 8). Test: `restart_mid_send_or_cancel_never_resends`. The shared startup recovery now covers both states.
- [x] An answered LAB-27 query and a correlated LAB-28 ORL (positive or negative followup) are recorded as handled, with no results, right after their answer or item outcomes commit. Startup recovery leaves them alone; a query stored but never answered is still recovered and parked with its MSH-21 reason (TF-2b 3.27/3.28; ADR 0003 rule 2; ADR 0016 recovery). Test: `restart_never_reparks_answered_query_or_orl`.
- [x] Shared restore pause blocks both new work and cancel admission (ADR 0003, TF-2b 3.28.4). Test: `restore_pause_blocks_send_ahead_and_cancel`. Cancellation still rechecks the fresh trusted feed inside the shared transaction.
- [x] An unmapped command is retained as Rejected without Sending or bytes (TF-2b 3.28.4; ADR 0003). Test: `unmapped_send_is_rejected_before_reservation`.
- [x] Independent directional connections and per-transaction outbound lifetime (TF-2b B.6, pp.79-80). Tests: `independent_query_followup_survives_inbound_close`, `independent_failed_link_never_replays_on_restart`, `independent_restore_pause_sends_no_work`, `independent_send_ahead_and_cancel_keep_awos`. Query follow-up ownership leaves the inbound session before a positive RSP; the outbound owner waits for RSP issuance, then dials once. Disconnect, failed connection, cancellation or expiration settles owned reservations without resend. Persistent mode remains selectable.
- [ ] Complete LAW actor qualification: remaining enhanced work groups/options and a recorded Gazelle run. Receive/send/query/ORL deadlines and both topologies have source-named local tests.

The runtime uses the shared order store (schema 9) and operator controls.


## Link topologies

Evidence remains **documented**. `law.connection_mode = "persistent"` keeps all three transactions on one configured TCP client/server link. `independent_per_transaction` requires `transport.kind = "tcp_server"` and an explicit `law.outbound_endpoint` for the analyzer's LAB-28 listener. The site endpoint is the LAB-27/LAB-29 listener. Neither direction needs its opposite connection to be established first. The host accepts at most four concurrent inbound sessions and owns one outbound transaction at a time, with at most eight queued follow-ups. All sessions, queued reservations and recording taps have an owner and are settled on shutdown. Peers may retain an inbound connection or close after each transaction; the host opens and closes its own outbound link per LAB-28 attempt. No failed or expired command is automatically repeated.

- [x] Both topology choices, required independent listener role/destination, and rejected unused/invalid destinations. Test: `crates/core/config/tests/law.rs::topologies_require_direction_and_outbound_endpoint`.
- [x] Both roles validate without startup I/O. Test: `crates/analyte/tests/receive.rs::law_roles_validate_without_startup_io`.
- [x] All three transactions share one persistent connection in both configured directions (B.5 and the declared B.6 subset). Test: `crates/analyte/tests/law/connections.rs::single_persistent_link_carries_every_transaction`.
- [x] Client disconnect/first-byte timeout retains evidence, reconnects and never replays reserved work. Test: `crates/analyte/tests/law/connections.rs::partial_deadline_keeps_exact_bytes_across_reconnect`.
- [x] Independent query/result and work links, peer closure, concurrent inbound handling, restart without replay, restore pause and original cancellation identities. Tests: `independent_query_followup_survives_inbound_close`, `independent_failed_link_never_replays_on_restart`, `independent_restore_pause_sends_no_work`, `independent_send_ahead_and_cancel_keep_awos`, `independent_crash_awaiting_orl_recovers_uncertain`.
- [ ] Gazelle qualification: record tool build, profile revision, actor/options, scenarios and validation results. Local synthetic tests are not simulator evidence.
- [ ] Full actor/workflow qualification: remaining enhanced content/groups, pooling/relationships and complete workflows remain as listed above. Isolate/parent needs app-supplied identities in a future contract; rack/tray forms are still explicitly rejected.
- [x] Shared stored-byte reprocessing (ADR 0003 rules 2-4; TF-2b 3.29.4). The shared reprocessing path uses LAB-29 calls `evidence::interpret(store, machine, id)`; `machine/interpretation.rs` selects the LAW parser/views/mapper. Test: `parked_result_reprocesses_after_mapping_fix`. It repairs a mapping, preserves original raw/config and interpretation version 1, appends audited versions, resolves parking, delivers once and creates no second delivery or machine traffic on repeated reprocessing. Reprocessing a retained non-C conflict adds one failed version/audit and no revisions or deliveries. A 50-character LAW control ID prevents generic-HL7 fallback.

### Review pass 4 regression evidence

Evidence remains documented; no open qualification row is promoted.

- L8: `patient_builder_and_receive_check_profile` now compares emitted PID/PV1 values and absent demographics. `work_builder_checks_container_details` compares each supplied SPM/SAC detail and omitted optional details, including unchanged volume units and dilution.
- L9: `orl_commit_precedes_each_item_outcome` holds raw ORL custody while every item remains Sending, then checks mixed indexed outcomes. `failed_orl_write_never_finishes_items` rejects the write and requires every item Uncertain. The preflight/reservation failure case now fails the second item; the failed query follow-up case requeries after restart without reissuing the original AWOS. `gates/lookup.rs::lookup_and_reservation_commit_precede_response` observes durable QBP at lookup entry and both committed Sending items before the first response byte. `cancel_rechecks_trust_after_candidate_selection` invalidates trust after selection, waits for transactional admission to return, and checks that no CancelRequested state or bytes appeared.
- L10: `pending_bound_never_returns_partial_work` exceeds the aggregate bound and requires AE, no positive work and no reservations. `gates/paging.rs::competing_feed_during_paging_is_refused` holds collection after the first of multiple pages, queues a generation-changing feed request on the same store owner, and requires AE with zero reservations rather than mixed or false-empty output. It also compares all 101 resulting cache sample IDs exactly.
- L11: negative follow-up tests compare exact retained ORL frames and audit actor, correlation, acceptance and both receipt targets. `parked_result_reprocesses_after_mapping_fix` checks audit action, actor, reason and receipt/version targets for success, repeated interpretation and conflict rollback.
- L12: the specimen/priority intersection test also enables peer repeat options and asserts empty TCD-6/7/8 and ORC-8 without app authorization.
- L13: independent successful query, negative work and send-ahead peers remain open until manager-owned EOF. Send-ahead is exercised before any inbound establishment. `gates/handover.rs::rsp_issuance_precedes_outbound_dial` holds transferred ownership before any RSP byte and again after RSP issuance, requiring no dial until release. `shutdown_during_handover_settles_without_reissue` and `queued_followup_expiry_settles_without_reissue` use a worker-received gate with injected queued deadline expiry, or shutdown while ownership is held, and check every reserved AWOS becomes Uncertain and retains its identity across restart/requery, which sends only negative work.
- L14: `partial_deadline_keeps_exact_bytes_across_reconnect` selects the exact partial frame, its receipt and incomplete-block parking reason with an unrelated receipt present, then checks the same receipt after restart and absence of replay. It waits for the exact frame and linked parking reason rather than inferring custody from EOF. `wire_deadline_does_not_cancel_raw_custody` holds custody across wire expiry and requires the original frame to survive without positive output.

### Program end-to-end evidence index

All tests below run with temporary protected stores and the repository reference
receiver. Tests name their TF-2b sections/ADR boundaries. Each index row concerns
only the stated behavior; unchecked actor and simulator gates stay open.

| Behavior | Named program test |
| --- | --- |
| Parked LAW result reprocessed after mapping repair, original retained, repeat produces no delivery | `parked_result_reprocesses_after_mapping_fix` |
| LAB-29 store before ACK, split input, AA only after interpretation | `raw_commit_precedes_interpretation_and_ack` |
| LAB-29 storage failure and near-full disk never AA | `storage_failure_never_sends_aa`, `nearly_full_disk_closes_without_aa` |
| Delivered once, retransmission no delivery, explicit C new revision | `results_deliver_once_and_c_corrects` |
| Uninterpretable retained with error ACK | `uninterpretable_result_is_parked_with_error_ack` |
| Unsolicited explicit identity, ambiguous data parked | `unsolicited_result_never_invents_awos` |
| Query found, one item/AWOS, reordered ORL outcomes | `found_query_reserves_and_finishes_each_awos` |
| Trusted empty, restricted negative work, correlated ORL without items | `trusted_empty_sends_negative_work_without_items` |
| Untrusted/read failure/restore pause never empty | `untrusted_or_paused_lookup_is_an_error` |
| Unreachable without trusted snapshot never empty | `unreachable_app_never_answers_no_work` |
| Late query includes first-byte and storage budget | `query_budget_counts_first_byte_and_storage` |
| Unsupported query forms are AR | `unsupported_query_forms_get_ar` |
| Send-ahead accepted/rejected/mixed AWOS; restart without resend | `send_ahead_accepted_rejected_and_mixed` |
| No reply/disconnect, including cancellation, becomes Uncertain | `no_reply_or_disconnect_is_never_replayed` |
| Crash during Sending or CancelRequested, no resend | `restart_mid_send_or_cancel_never_resends` |
| Answered query and ORL not re-parked on restart; unanswered query still parked | `restart_never_reparks_answered_query_or_orl` |
| Cancel confirmed/refused/mixed per AWOS, retained identities | `cancel_confirmed_refused_and_mixed_per_awos` |
| Restore pause blocks send-ahead and cancellation | `restore_pause_blocks_send_ahead_and_cancel` |
| Query work can be cancelled with send-ahead disabled | `queried_work_cancels_without_send_ahead` |
| Independent query/work/result sockets, closure, restart and restore pause | `independent_query_followup_survives_inbound_close`, `independent_failed_link_never_replays_on_restart`, `independent_restore_pause_sends_no_work`, `independent_send_ahead_and_cancel_keep_awos` |
| Both TCP roles, all transactions on one persistent link | `single_persistent_link_carries_every_transaction` |
| Client query and ORL deadlines use config | `single_client_uses_configured_query_and_orl_deadlines` |
| Client reconnect/deadline retains partial data, no replay | `partial_deadline_keeps_exact_bytes_across_reconnect` |

LAB-29 cases are in `crates/analyte/tests/law/results.rs`, query cases in
`queries.rs`, send/cancel cases in `work.rs`, and role cases in `connections.rs`.
All compile under `crates/analyte/tests/delivery.rs`. Store trust/paging tests,
config validation and pure profile tests remain listed in earlier rows.


The shared reprocessing path preserves original evidence and never sends machine work.

| State | Source | Restriction |
| --- | --- | --- |
| Open restriction | TF-2b section 3.29.4.1.2.1 | Status-only IP/SC/CA is retained and specifically parked; durable lifecycle registration awaits an ADR. |
