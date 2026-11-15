# POCT1-A (CLSI POCT01-A2)

Evidence level: documented. Phase 5.

Draft conformance checklist, written in our own words from the standard and
checked in an independent review on 2026-11-15. Section and page numbers
point into the standard; nothing here reproduces its text or tables. The
phase starts by confirming this scope (plan principle 7); the checklist then
becomes the test list, one or more tests per row.

## Scope decisions to confirm at phase start

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

- [ ] **R** Receive HEL, DST, OBS, EOT, END, positive/error ACK and ESC; send observation REQ, END, positive/error ACK and ESC (Appendix B §4.4, p.90).
- [ ] **R** Accept/reject Hello correctly, acknowledge accepted Hello and Device Status in sequence, then drive the dialogue (Appendix B §4.1.2, p.78).
- [ ] **R** Read capabilities, tailor topics and support legal repeated-topic handling (Appendix B §4.1.3, p.78; §§5.6–5.7, p.95).
- [ ] **R** Request announced new observations; support patient/nonpatient results, zero/multiple messages and EOT completion (Appendix B §§4.1.4–4.1.5, pp.78–79).
- [ ] **R** Acknowledge dynamic transfers only after taking custody; prevent repeat observation recording independently of delivery identity (Appendix B §3.2, pp.71–72; project requires durable storage before ACK).
- [ ] **R** Distinguish application error ACK from protocol Escape; leave escaped topics in defined states and support normal/abnormal termination (Appendix B §§3.4,4.1.11–4.1.12, pp.73–74,81–86).
- [ ] **R** Respect device application timeout and do not replay the prior message solely on application timeout (Appendix B §3.6, p.75).
- [ ] **R** Implement required objects, attributes, containment/cardinalities, order, codes and control-ID correlation for supported messages (Appendix B §§5–6, pp.91–126).
- [ ] **R** Use POCT-defined coded/quantity/time/null/encapsulated types and XML representations, not later incompatible HL7 v3 encodings (Appendix B §8, pp.128–137).
- [ ] **R** Apply normative DTD structures to supported messages (Appendix B §12, pp.171–202); secure local schema handling is an additional project requirement.
- [ ] **R** Tolerate allowed asynchronous observations, preserve message/ACK order and apply flow control when unable to accept more data (Appendix B §9, pp.138–139).

### Optional extensions and conditional behavior

- [ ] **O/C** Device events: request/receive/acknowledge events and retain operator/event context and EOT (Appendix B §§4.1.6,5.8,6.3, pp.79–80,96,112).
- [ ] **O/C** Patient/operator lists: obey complete/incremental support, acknowledgements and EOT (Appendix B §§4.1.7,6.11–6.12, pp.80,123–125).
- [ ] **O/C** Directives: send supported authorized commands and distinguish receipt from eventual execution (Appendix B §§4.1.8,6.5, pp.80–81,113–117).
- [ ] **O/C** Keep-alive: obey allowed timing and ACK/unsupported-topic Escape behavior (Appendix B §§3.5,4.1.9, pp.74–75,81).
- [ ] **O/C** Continuous mode: drain announced observations first, enter through acknowledged START_CONTINUOUS, accept unsolicited observations/events and obey list/directive interleaving and termination (Appendix B §4.2, pp.86–89).
- [ ] **O/C** Set time: supply UTC offset, claim accuracy only with synchronization evidence and handle applicable timezone transition data (Appendix B §6.5.2.1, pp.115–117; §10, pp.139–143).
- [ ] **O/C** Vendor extensions: preserve legal headers, bounded topics, standard startup and unsupported-extension responses (Appendix B §7, pp.126–128).

### Additional interfaces, if claimed

- [ ] **C** Physical Access Point: meet selected physical/link discovery, IAS and networked-AP requirements with referenced lower-layer specifications (Appendix A §§7–8, pp.38–46). TCP DML alone does not satisfy this.
- [ ] **C** Device actor: implement Appendix B's Device-side obligations in the peer; physical compatible Device claims additionally require Appendix A §6, pp.36–38.
- [ ] **C** ORI: implement Observation Reviewer-side HL7 reporting/acknowledgement/order-result association flows (Appendix C §§2–4, pp.203–222), including referenced base definitions. HTTPS is separate.
- [ ] **R, project gate** Test a minimum-capability Basic peer, every advertised extension, malformed input, async backpressure, timeout, lost ACK/reconnection, duplicate observation, restart and disk failure. Examples supplement these tests.
