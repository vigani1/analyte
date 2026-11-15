# POCT design decisions

These entries retain the declared scope and conformance choices. An unchecked
checklist item is a requirement or open decision, not a claim of implementation.
Source references retained in the checklist have not been independently reverified
for this migration; those entries say **source**: not found here.

## 1. Scope

- **what**: **Edition:** POCT01-A2 (2006), archived by CLSI but still current.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 2. Scope

- **what**: **Role:** Analyte implements the **Device Messaging Layer (DML)
  Observation Reviewer** over TCP. Device behavior lives only in the test
  peer. Policies and authorized lists stay in the lab app.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 3. Scope

- **what**: **Out of scope unless claimed later:** the physical Access Point (Appendix
  A, which needs further lower-layer standards) and the HL7 Observation
  Reporting Interface (Appendix C). Analyte's HTTPS contract is not ORI.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 4. Scope

- **what**: **Conformance gate:** incomplete. Basic Profile components and the program paths
  named below are implemented and tested; unchecked rows retain the remaining
  evidence gaps. Examples and passing component tests alone do not close them.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 5. Conformance choice

- **what**: **R** Parse the mandatory HEL, DST, OBS, EOT, END, positive/error ACK and ESC bodies and build observation REQ, END, positive/error ACK and ESC (Appendix B section 4.4, p.90). Parser tests: `poct01_b_6_mandatory_message_bodies_and_control_ids`, `poct01_b_6_10_patient_and_nonpatient_values_are_explicit`. Builder test: `poct01_b_6_reviewer_builders_round_trip_and_correlate`. Conversation coverage is listed separately below.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_6_mandatory_message_bodies_and_control_ids`, `poct01_b_6_10_patient_and_nonpatient_values_are_explicit`, `poct01_b_6_reviewer_builders_round_trip_and_correlate`.

## 6. Conformance choice

- **what**: **R** Accept or reject Hello and acknowledge accepted Hello/Status in sequence; park parse/interpretation failures, send a classified error ACK then abnormal Terminate when the header reference is unambiguous (Appendix B section 4.1.2, p.78). Dialog test: `poct01_b_4_1_2_startup_and_rejection_in_sequence`. Program tests: `poct01_b4_1_2_program_rejected_hello_ack_then_terminate_keeps_raw`, `poct01_b4_1_2_program_ambiguous_hello_header_is_parked_without_guessed_ack`. Termination deadline test: `poct01_b4_1_2_invalid_capabilities_are_retained_and_termination_has_deadline`.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_4_1_2_startup_and_rejection_in_sequence`, `poct01_b4_1_2_program_rejected_hello_ack_then_terminate_keeps_raw`, `poct01_b4_1_2_program_ambiguous_hello_header_is_parked_without_guessed_ack`, `poct01_b4_1_2_invalid_capabilities_are_retained_and_termination_has_deadline`.

## 7. Conformance choice

- **what**: **R** Read capabilities, tailor topics and support legal repeated-topic handling (Appendix B sections 4.1.3, 5.6-5.7, pp.78,95-96). Partial evidence: `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination` exercises repeated mandatory OBS topics; the host capacity test below covers the size limit.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination`.

## 8. Conformance choice

- **what**: **R** Request announced new observations; support patient/nonpatient results, zero/multiple messages and EOT completion (Appendix B sections 4.1.4-4.1.5, pp.78-79). Partial evidence: `poct01_b_6_10_patient_and_nonpatient_values_are_explicit` is a parser test; `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks` and `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination` exercise parts of the dialog.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_6_10_patient_and_nonpatient_values_are_explicit`, `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`, `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination`.

## 9. Conformance choice

- **what**: **R** Acknowledge dynamic transfers after durable custody; configured device/time/analyte identity (plus sequence when declared) uses the store's revision history to prevent duplicate deliveries across lost ACK, reconnect and restart, with edited corrections and ambiguous/conflicting receipts parked (Appendix B section 3.2, pp.71-72; section 5.5, p.95; section 5.21 Tables 46/49, p.105; ADR 0003). Program/reference-receiver test: `poct01_b3_2_program_lost_ack_reconnect_restart_and_correction`. Custody gate test: `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b3_2_program_lost_ack_reconnect_restart_and_correction`, `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`.

## 10. Conformance choice

- **what**: **R** In the dialog, application error ACK leaves OBS open, storage failure issues Escape/abnormal Terminate, a correlated peer Escape completes the topic, and normal termination requires the matching final ACK (Appendix B sections 3.4, 4.1.11-4.1.12, pp.73-74,81-86). Dialog tests: `poct01_b_3_4_error_ack_keeps_topic_but_storage_failure_escapes`, `poct01_b_4_1_12_escape_correlation_and_final_ack_correlation`, `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination`.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_3_4_error_ack_keeps_topic_but_storage_failure_escapes`, `poct01_b_4_1_12_escape_correlation_and_final_ack_correlation`, `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination`.

## 11. Conformance choice

- **what**: **R** Respect device application timeout and do not replay the prior message solely on application timeout (Appendix B §3.6, p.75).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_3_6_no_resend_on_application_timeout`.

## 12. Conformance choice

- **what**: **R** Implement all required objects, attributes, containment/cardinalities, order and codes for supported messages (Appendix B sections 5-6, pp.91-126). Partial evidence: `poct01_b_6_12_required_structure_and_order_are_enforced` checks selected HEL/OBS failures, not every supported object or cardinality.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_6_12_required_structure_and_order_are_enforced`.

## 13. Conformance choice

- **what**: **R** Use POCT-defined coded/quantity/time/null/encapsulated types and XML representations (Appendix B section 8, pp.128-137). Partial evidence: `poct01_b_8_nulls_quantities_qualitative_codes_and_translation`, `poct01_b_8_11_time_keeps_offset_precision_and_null_distinct`, `poct01_b_12_intervals_and_value_choice_are_not_later_hl7_encodings`, and the mapping tests cover selected types and invalid encodings.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_8_nulls_quantities_qualitative_codes_and_translation`, `poct01_b_8_11_time_keeps_offset_precision_and_null_distinct`, `poct01_b_12_intervals_and_value_choice_are_not_later_hl7_encodings`.

## 14. Conformance choice

- **what**: **R** Apply all normative DTD structures for supported messages (Appendix B section 12, pp.171-202); secure local schema handling is a separate project rule. Partial evidence: `poct01_b_12_auxiliary_metadata_and_encapsulation_are_local_only` checks auxiliary fields and fixed attributes; `poct01_project_security_rejects_dtd_without_resolution` proves that the XML reader refuses external/internal DTDs.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_12_auxiliary_metadata_and_encapsulation_are_local_only`, `poct01_project_security_rejects_dtd_without_resolution`.

## 15. Conformance choice

- **what**: **R** Tolerate allowed asynchronous observations, preserve message/ACK order and apply flow control (Appendix B section 9, pp.138-139). Partial dialog evidence: `poct01_b_9_bounded_backpressure_returns_unconsumed_input` returns excess input as Deferred, and `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks` orders simulated custody completions.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_9_bounded_backpressure_returns_unconsumed_input`, `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`.

## 16. Conformance choice

- **what**: **R** Bound every host reply by the smaller of the configured limit and device receive capacity; reject an unfit message without truncation (Appendix B section 5.7, p.96). Host test: `poct01_b5_7_host_never_sends_beyond_device_capacity`.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b5_7_host_never_sends_beyond_device_capacity`.

## 17. Conformance choice

- **what**: **R** Detect reused incoming control IDs across message types and custody states, park their evidence before Escape/abnormal Terminate, and stop without eviction when the conversation ID budget fills (Appendix B section 5.13, printed p.99; section 3.4.2, p.74). Program test: `poct01_b5_13_program_reused_control_id_parks_evidence_and_escapes`. Dialog bounds/edge tests: `poct01_b5_13_reused_id_before_commit_is_retained_not_accepted`, `poct01_b5_13_control_id_budget_ends_conversation_without_forgetting_ids`, `poct01_b5_13_rejected_hello_reference_cannot_be_reused_by_terminal_ack`. The final ACK remains usable at capacity; reconnect starts a fresh ID namespace.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b5_13_program_reused_control_id_parks_evidence_and_escapes`, `poct01_b5_13_reused_id_before_commit_is_retained_not_accepted`, `poct01_b5_13_control_id_budget_ends_conversation_without_forgetting_ids`, `poct01_b5_13_rejected_hello_reference_cannot_be_reused_by_terminal_ack`.

## 18. Conformance choice

- **what**: **O/C** Device events: request/receive/acknowledge events and retain operator/event context and EOT (Appendix B §§4.1.6,5.8,6.3, pp.79–80,96,112).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 19. Conformance choice

- **what**: **O/C** Patient/operator lists: obey complete/incremental support, acknowledgements and EOT (Appendix B §§4.1.7,6.11–6.12, pp.80,123–125).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 20. Conformance choice

- **what**: **O/C** Directives: send supported authorized commands and distinguish receipt from eventual execution (Appendix B §§4.1.8,6.5, pp.80–81,113–117).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 21. Conformance choice

- **what**: **O/C** Keep-alive: obey allowed timing and ACK/unsupported-topic Escape behavior (Appendix B §§3.5,4.1.9, pp.74–75,81).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 22. Conformance choice

- **what**: **O/C** Continuous mode: drain announced observations first, enter through acknowledged START_CONTINUOUS, accept unsolicited observations/events and obey list/directive interleaving and termination (Appendix B §4.2, pp.86–89).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 23. Conformance choice

- **what**: **O/C** Set time: supply UTC offset, claim accuracy only with synchronization evidence and handle applicable timezone transition data (Appendix B §6.5.2.1, pp.115–117; §10, pp.139–143).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 24. Conformance choice

- **what**: **O/C** Vendor extensions: preserve legal headers, bounded topics, standard startup and unsupported-extension responses (Appendix B §7, pp.126–128).
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 25. Conformance choice

- **what**: **C** Physical Access Point: meet selected physical/link discovery, IAS and networked-AP requirements with referenced lower-layer specifications (Appendix A §§7–8, pp.38–46). TCP DML alone does not satisfy this.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 26. Conformance choice

- **what**: **C** Device actor: implement Appendix B's Device-side obligations in the peer; physical compatible Device claims additionally require Appendix A §6, pp.36–38.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 27. Conformance choice

- **what**: **C** ORI: implement Observation Reviewer-side HL7 reporting/acknowledgement/order-result association flows (Appendix C §§2–4, pp.203–222), including referenced base definitions. HTTPS is separate.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: none yet.

## 28. Conformance choice

- **what**: **R, project gate** Test a minimum-capability Basic peer, every advertised extension, malformed input, async backpressure, timeout, lost ACK/reconnection, duplicate observation, restart and disk failure. Examples supplement these tests.
- **why**: Keep the declared protocol contract explicit; the checklist records whether the full behavior is evidenced.
- **source**: not found.
- **test**: `poct01_b_4_1_2_minimum_capability_peer_and_hello_deadline`, `poct01_project_disconnect_wrong_session_and_reversed_time_do_not_ack`, `poct01_b3_2_program_lost_ack_reconnect_restart_and_correction`.

