# POCT conformance

## Checklist

### Mandatory Basic Profile

- [x] **R** Parse the mandatory HEL, DST, OBS, EOT, END, positive/error ACK and ESC bodies and build observation REQ, END, positive/error ACK and ESC (Appendix B section 4.4, p.90). Parser tests: `each_mandatory_body_has_exact_meaning`, `patient_and_nonpatient_values_are_explicit`. Builder test: `reviewer_builders_round_trip_and_correlate`. Conversation coverage is listed separately below.
- [x] **R** Accept or reject Hello and acknowledge accepted Hello/Status in sequence; park parse/interpretation failures, send a classified error ACK then abnormal Terminate when the header reference is unambiguous (Appendix B section 4.1.2, p.78). Dialog test: `startup_and_rejection_in_sequence`. Program tests: `rejected_hello_acks_terminates_and_keeps_raw`, `ambiguous_hello_header_parked_without_guessed_ack`. Termination deadline test: `invalid_capabilities_kept_and_termination_bounded`.
- [ ] **R** Read capabilities, tailor topics and support legal repeated-topic handling (Appendix B sections 4.1.3, 5.6-5.7, pp.78,95-96). Partial evidence: `zero_observations_repeat_topic_and_terminate` exercises repeated mandatory OBS topics; the host capacity test below covers the size limit. Remaining: the cited repeated-topic test does not exercise capability-dependent topic selection; optional topic extensions remain unimplemented.
- [ ] **R** Request announced new observations; support patient/nonpatient results, zero/multiple messages and EOT completion (Appendix B sections 4.1.4-4.1.5, pp.78-79). Partial evidence: `patient_and_nonpatient_values_are_explicit` is a parser test; `commit_gate_orders_asynchronous_acks` and `zero_observations_repeat_topic_and_terminate` exercise parts of the dialog. Remaining: a program-level conversation matrix covering patient/nonpatient and zero/multiple transfers through EOT, rather than inferring full receive behavior from syntax samples.
- [ ] **R** Acknowledge dynamic transfers after durable custody; configured device/time/analyte identity (plus sequence when declared) uses the store's revision history to prevent duplicate deliveries across lost ACK, reconnect and restart, with edited corrections and ambiguous/conflicting receipts parked (Appendix B section 3.2, pp.71-72; section 5.5, p.95; section 5.21 Tables 46/49, p.105; ADR 0003). Program/reference-receiver test: `lost_ack_restart_and_correction_deliver_once`. Custody gate test: `commit_gate_orders_asynchronous_acks`.
- [x] **R** In the dialog, application error ACK leaves OBS open, storage failure issues Escape/abnormal Terminate, a correlated peer Escape completes the topic, and normal termination requires the matching final ACK (Appendix B sections 3.4, 4.1.11-4.1.12, pp.73-74,81-86). Dialog tests: `error_ack_keeps_topic_but_store_failure_escapes`, `escape_and_final_ack_correlate`, `zero_observations_repeat_topic_and_terminate`.
- [x] **R** Respect device application timeout and do not replay the prior message solely on application timeout (Appendix B §3.6, p.75). Test: `application_timeout_never_resends`.
- [ ] **R** Implement all required objects, attributes, containment/cardinalities, order and codes for supported messages (Appendix B sections 5-6, pp.91-126). Partial evidence: `required_structure_and_order_are_enforced` checks selected HEL/OBS failures, not every supported object or cardinality. Remaining: the full supported-message validation matrix. Header uniqueness has its own program-tested row below; ACK correlation is covered by the dialog/builder rows.
- [ ] **R** Use POCT-defined coded/quantity/time/null/encapsulated types and XML representations (Appendix B section 8, pp.128-137). Partial evidence: `null_quantity_and_coded_values_are_validated`, `time_keeps_offset_and_precision`, `intervals_and_value_choice_use_poct_forms`, and the mapping tests cover selected types and invalid encodings. Remaining: a complete supported-type/encoding matrix; the original single cited parser test does not prove this whole claim.
- [ ] **R** Apply all normative DTD structures for supported messages (Appendix B section 12, pp.171-202); secure local schema handling is a separate project rule. Partial evidence: `auxiliary_metadata_and_encapsulation_stay_local` checks auxiliary fields and fixed attributes; `rejects_dtd_without_resolving_it` proves that the XML reader refuses external/internal DTDs. Remaining: complete normative structure/cardinality evidence; rejecting DTD declarations or parsing metadata samples does not prove DTD conformance.
- [ ] **R** Tolerate allowed asynchronous observations, preserve message/ACK order and apply flow control (Appendix B section 9, pp.138-139). Partial dialog evidence: `backpressure_returns_unconsumed_input` returns excess input as Deferred, and `commit_gate_orders_asynchronous_acks` orders simulated custody completions. Remaining: program-path asynchronous peer/backpressure coverage; these pure action tests do not exercise the host transport/store loop.

- [x] **R** Bound every host reply by the smaller of the configured limit and device receive capacity; reject an unfit message without truncation (Appendix B section 5.7, p.96). Program tests: `unsupported_keep_alive_gets_correlated_escape` checks ACK/error ACK/REQ/ESC/END with each ordering of the limits; `later_error_ack_cannot_exceed_device_capacity` checks later refusal; `host_never_sends_beyond_device_capacity` retains the one-byte refusal case.

- [x] **R** Detect reused incoming control IDs across message types and custody states, park their evidence before Escape/abnormal Terminate, and stop without eviction when the conversation ID budget fills (Appendix B section 5.13, printed p.99; section 3.4.2, p.74). Program test: `reused_control_id_parks_evidence_and_escapes`. Dialog bounds/edge tests: `reused_id_before_commit_is_retained_not_accepted`, `control_id_budget_ends_conversation_keeping_ids`, `rejected_hello_reference_never_reused`. The final ACK remains usable at capacity; reconnect starts a fresh ID namespace.

### Optional extensions and conditional behavior

- [ ] **O/C** Device events: request/receive/acknowledge events and retain operator/event context and EOT (Appendix B §§4.1.6,5.8,6.3, pp.79–80,96,112). Remaining: extension not implemented or advertised.
- [ ] **O/C** Patient/operator lists: obey complete/incremental support, acknowledgements and EOT (Appendix B §§4.1.7,6.11–6.12, pp.80,123–125). Remaining: extension not implemented or advertised.
- [ ] **O/C** Directives: send supported authorized commands and distinguish receipt from eventual execution (Appendix B §§4.1.8,6.5, pp.80–81,113–117). Remaining: extension not implemented or advertised.
- [ ] **O/C** Keep-alive: obey allowed timing and ACK/unsupported-topic Escape behavior (Appendix B §§3.5,4.1.9, pp.74–75,81). Done: the reviewer is not prepared for Keep Alive and answers each one with a correlated OTH Escape only, then continues the conversation; nothing is parked. Program test: `unsupported_keep_alive_gets_correlated_escape`. Remaining: acknowledging Keep Alive and the timing rule (only when no reply is outstanding) are not implemented.
- [ ] **O/C** Continuous mode: drain announced observations first, enter through acknowledged START_CONTINUOUS, accept unsolicited observations/events and obey list/directive interleaving and termination (Appendix B §4.2, pp.86–89). Remaining: extension not implemented or advertised.
- [ ] **O/C** Set time: supply UTC offset, claim accuracy only with synchronization evidence and handle applicable timezone transition data (Appendix B §6.5.2.1, pp.115–117; §10, pp.139–143). Remaining: extension not implemented or advertised.
- [ ] **O/C** Vendor extensions: preserve legal headers, bounded topics, standard startup and unsupported-extension responses (Appendix B §7, pp.126–128). Remaining: extension not implemented or advertised.

### Additional interfaces, if claimed

- [ ] **C** Physical Access Point: meet selected physical/link discovery, IAS and networked-AP requirements with referenced lower-layer specifications (Appendix A §§7–8, pp.38–46). TCP DML alone does not satisfy this. Remaining: outside the supported Observation Reviewer TCP profile; no conformance claimed.
- [ ] **C** Device actor: implement Appendix B's Device-side obligations in the peer; physical compatible Device claims additionally require Appendix A §6, pp.36–38. Remaining: outside the supported Observation Reviewer TCP profile; no conformance claimed.
- [ ] **C** ORI: implement Observation Reviewer-side HL7 reporting/acknowledgement/order-result association flows (Appendix C §§2–4, pp.203–222), including referenced base definitions. HTTPS is separate. Remaining: outside the supported Observation Reviewer TCP profile; no conformance claimed.
- [ ] **R, project gate** Test a minimum-capability Basic peer, every advertised extension, malformed input, async backpressure, timeout, lost ACK/reconnection, duplicate observation, restart and disk failure. Examples supplement these tests. Remaining: every advertised extension must be tested when added. Basic failure evidence: `minimum_capability_peer_and_hello_deadline`, `disconnect_wrong_session_and_reversed_time_never_ack`, `lost_ack_restart_and_correction_deliver_once` (program/reference receiver, replacing the standalone index test as host evidence), the Hello/control-ID program tests above, and the host storage-failure tests in [chunk_receive/custody.rs](../../../crates/analyte/tests/chunk_receive/custody.rs).

The correction receiver test explicitly compares stable observation identities,
revisions 0/1, original/corrected values and distinct receipt/delivery IDs.
Its ambiguous time/sequence and conflict probes each assert receipt-specific
parking evidence and unchanged outbox counts (B 3.2, 5.21).

`unexpected_ack_escape_references_incoming_header` checks the
incoming ACK header reference independently of its body reference (B 5.12).
`unsupported_keep_alive_gets_correlated_escape` covers a legal
post-ACK Keep Alive: retained bytes, an OTH Escape alone, no parked record, a
later accepted observation and normal termination after EOT. It also
checks every emitted ACK/error ACK/REQ/ESC/END length with each ordering of
host/device limits; the one-byte refusal test remains. This is selected
conversation evidence, not an exhaustive capacity/latency matrix.

## Identity safety follow-up

`repeated_analyte_without_identity_is_parked`
checks equal/different same-analyte siblings under NEW, RES and EDT through
the program and reference receiver. Mapping examines the full message before
reconciliation and marks every repeated derived identity unresolved. Both
observations and specific reasons are retained; neither revisions nor outbox
rows are created. XML positions remain evidence locations, never result keys.

P12 now uses conservative parking for EDT with unchanged latest common
content rather than silently suppressing it. Patient context is not delivered
downstream today. A precise protocol-context comparison still requires a
future ADR; no stored format or common model was changed.
## Source-backed type and edit regressions

`crates/protocols/poct/tests/type_rules.rs` covers numeric fixed metadata
(B 12.3 p.189), DML device/AP identifiers, nonnegative counts, the four-character
encoding declaration, CS lot fields (including the p.285 clarification),
default CV tables and qualified extensions, unique advertised sets, CD
translations/modifiers, restricted auxiliary child models, thumbnail property
inheritance, initial ED text and integral trailing-point REAL/PQ literals.
These selected regressions do not close the full vocabulary/type matrix.
The object definitions and published lot clarification take precedence over
the older element DTD's CV lot spelling.

Mapping tests cover UTC Z versus signed/absent offsets with original spelling
and precision, and retain trailing-point quantity text. The program test
`patient_and_metadata_edits_are_not_retransmissions`
checks conservative receipt-specific parking for patient, method, critical
range, normal-range unit, coded-version and specimen-namespace edits. Changed
common content still corrects; NEW/RES reuploads still deduplicate. Repeated
EDTs after a lost ACK park independently without duplicate delivery. See the
explicit patient-context limitation and policy in [decisions.md](decisions.md).
Evidence remains documented and the broader conformance gate remains open.
