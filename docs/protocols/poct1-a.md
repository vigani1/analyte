# POCT1-A (CLSI POCT01-A2)

Evidence level: documented. Phase 5.

Current conformance checklist, written in our own words. Checked rows name
executable synthetic tests; unchecked rows state remaining work. These tests
support documented implementation, not hardware or simulator qualification.

## Supported scope

- **Edition:** POCT01-A2 (2006), archived by CLSI but still current.
- **Role:** Analyte implements the **Device Messaging Layer (DML)
  Observation Reviewer** over TCP. Device behavior lives only in the test
  peer. Policies and authorized lists stay in the lab app.
- **Out of scope unless claimed later:** the physical Access Point (Appendix
  A, which needs further lower-layer standards) and the HL7 Observation
  Reporting Interface (Appendix C). Analyte's HTTPS contract is not ORI.
- **Done:** normative behavior, objects, data types and DTDs are tested;
  the standard's examples are fixtures, not the whole gate.

## Checklist

### Mandatory Basic Profile

- [x] **R** Receive HEL, DST, OBS, EOT, END, positive/error ACK and ESC; send observation REQ, END, positive/error ACK and ESC (Appendix B §4.4, p.90). Test: `poct01_b_6_mandatory_message_bodies_and_control_ids`.
- [x] **R** Accept/reject Hello correctly, acknowledge accepted Hello and Device Status in sequence, then drive the dialogue (Appendix B §4.1.2, p.78). Test: `poct01_b_4_1_2_startup_and_rejection_in_sequence`.
- [x] **R** Read capabilities, tailor topics and support legal repeated-topic handling (Appendix B §4.1.3, p.78; §§5.6–5.7, p.95). Test: `poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination`.
- [x] **R** Request announced new observations; support patient/nonpatient results, zero/multiple messages and EOT completion (Appendix B §§4.1.4–4.1.5, pp.78–79). Test: `poct01_b_6_10_patient_and_nonpatient_values_are_explicit`.
- [x] **R** Acknowledge dynamic transfers only after taking custody; prevent repeat observation recording independently of delivery identity (Appendix B §3.2, pp.71–72; project requires durable storage before ACK). Test: `poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks`.
- [x] **R** Distinguish application error ACK from protocol Escape; leave escaped topics in defined states and support normal/abnormal termination (Appendix B §§3.4,4.1.11–4.1.12, pp.73–74,81–86). Test: `poct01_b_3_4_error_ack_keeps_topic_but_storage_failure_escapes`.
- [x] **R** Respect device application timeout and do not replay the prior message solely on application timeout (Appendix B §3.6, p.75). Test: `poct01_b_3_6_no_resend_on_application_timeout`.
- [x] **R** Implement required objects, attributes, containment/cardinalities, order, codes and control-ID correlation for supported messages (Appendix B §§5–6, pp.91–126). Test: `poct01_b_6_12_required_structure_and_order_are_enforced`.
- [x] **R** Use POCT-defined coded/quantity/time/null/encapsulated types and XML representations, not later incompatible HL7 v3 encodings (Appendix B §8, pp.128–137). Test: `poct01_b_8_nulls_quantities_qualitative_codes_and_translation`.
- [x] **R** Apply normative DTD structures to supported messages (Appendix B §12, pp.171–202); secure local schema handling is an additional project requirement. Test: `poct01_b_12_auxiliary_metadata_and_encapsulation_are_local_only`.
- [x] **R** Tolerate allowed asynchronous observations, preserve message/ACK order and apply flow control when unable to accept more data (Appendix B §9, pp.138–139). Test: `poct01_b_9_bounded_backpressure_returns_unconsumed_input`.

- [x] **R** Bound every host reply by the smaller of the configured limit and device receive capacity; reject an unfit message without truncation (Appendix B section 5.7, p.96). Host test: `poct01_b5_7_host_never_sends_beyond_device_capacity`.

### Optional extensions and conditional behavior

- [ ] **O/C** Device events: request/receive/acknowledge events and retain operator/event context and EOT (Appendix B §§4.1.6,5.8,6.3, pp.79–80,96,112). Remaining: extension not implemented or advertised.
- [ ] **O/C** Patient/operator lists: obey complete/incremental support, acknowledgements and EOT (Appendix B §§4.1.7,6.11–6.12, pp.80,123–125). Remaining: extension not implemented or advertised.
- [ ] **O/C** Directives: send supported authorized commands and distinguish receipt from eventual execution (Appendix B §§4.1.8,6.5, pp.80–81,113–117). Remaining: extension not implemented or advertised.
- [ ] **O/C** Keep-alive: obey allowed timing and ACK/unsupported-topic Escape behavior (Appendix B §§3.5,4.1.9, pp.74–75,81). Remaining: extension not implemented or advertised.
- [ ] **O/C** Continuous mode: drain announced observations first, enter through acknowledged START_CONTINUOUS, accept unsolicited observations/events and obey list/directive interleaving and termination (Appendix B §4.2, pp.86–89). Remaining: extension not implemented or advertised.
- [ ] **O/C** Set time: supply UTC offset, claim accuracy only with synchronization evidence and handle applicable timezone transition data (Appendix B §6.5.2.1, pp.115–117; §10, pp.139–143). Remaining: extension not implemented or advertised.
- [ ] **O/C** Vendor extensions: preserve legal headers, bounded topics, standard startup and unsupported-extension responses (Appendix B §7, pp.126–128). Remaining: extension not implemented or advertised.

### Additional interfaces, if claimed

- [ ] **C** Physical Access Point: meet selected physical/link discovery, IAS and networked-AP requirements with referenced lower-layer specifications (Appendix A §§7–8, pp.38–46). TCP DML alone does not satisfy this. Remaining: outside the supported Observation Reviewer TCP profile; no conformance claimed.
- [ ] **C** Device actor: implement Appendix B's Device-side obligations in the peer; physical compatible Device claims additionally require Appendix A §6, pp.36–38. Remaining: outside the supported Observation Reviewer TCP profile; no conformance claimed.
- [ ] **C** ORI: implement Observation Reviewer-side HL7 reporting/acknowledgement/order-result association flows (Appendix C §§2–4, pp.203–222), including referenced base definitions. HTTPS is separate. Remaining: outside the supported Observation Reviewer TCP profile; no conformance claimed.
- [ ] **R, project gate** Test a minimum-capability Basic peer, every advertised extension, malformed input, async backpressure, timeout, lost ACK/reconnection, duplicate observation, restart and disk failure. Examples supplement these tests. Remaining: every advertised extension must be tested when added. Basic failure evidence: `poct01_b_4_1_2_minimum_capability_peer_and_hello_deadline`, `poct01_project_disconnect_wrong_session_and_reversed_time_do_not_ack`, `poct01_b_3_2_lost_ack_reconnect_and_restart_restore_same_identity`, and the host storage-failure tests in [vendor_receive.rs](../../crates/analyte/tests/vendor_receive.rs).
