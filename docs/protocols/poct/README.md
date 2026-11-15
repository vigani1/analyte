# POCT1-A (CLSI POCT01-A2)

Evidence level: documented. Phase 5.

Current conformance checklist, written in our own words. Checked rows name
executable synthetic tests; unchecked rows state remaining work. These tests
support documented implementation, not hardware or simulator qualification.

The final review audited the cited test bodies and their call paths. Parser tests
prove parsing, builders prove output syntax, and dialog tests prove ordered
actions with host-reported custody; only tests labelled program/reference-receiver
exercise transport, protected storage and delivery together. A row stays unchecked
where its full claim exceeds that evidence, even if some component tests pass.

## Supported scope

- **Edition:** POCT01-A2 (2006), archived by CLSI but still current.
- **Role:** Analyte implements the **Device Messaging Layer (DML)
  Observation Reviewer** over TCP. Device behavior lives only in the test
  peer. Policies and authorized lists stay in the lab app.
- **Out of scope unless claimed later:** the physical Access Point (Appendix
  A, which needs further lower-layer standards) and the HL7 Observation
  Reporting Interface (Appendix C). Analyte's HTTPS contract is not ORI.
- **Conformance gate:** incomplete. Basic Profile components and the program paths
  named below are implemented and tested; unchecked rows retain the remaining
  evidence gaps. Examples and passing component tests alone do not close them.


## Notes

- [Design decisions](decisions.md)
- [Conformance checklist and retained evidence](checklist.md)
- [Versions](versions.md)
