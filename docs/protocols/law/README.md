# IHE LAW (PaLM Technical Framework Rev 11.0)

Evidence level: documented. Phase 4. A recorded Gazelle simulator conversation
is the next qualification step.

Conformance checklist, written in our own words from the standard and checked
in an independent review on 2026-11-15. Field-profile implementation now covers
all five Appendix C groups. Evidence is still **documented**, not simulated.
Section/page references identify sources; no source tables or examples are copied.

Checked rows mean their stated behavior has named local tests at the specified
pure-crate or program boundary. An
unchecked row can have partial evidence; its remaining work is stated explicitly.
A planned test is not evidence and does not exist yet. Even a checked field row
does not establish simulator, host integration or overall actor conformance.
The crate README records exact fields and source discrepancies. Test names below
are in `crates/protocols/hl7/tests/` unless a different path is given.

## Agreed scope and qualification

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


## Notes

- [Design decisions](decisions.md)
- [Conformance checklist and retained evidence](checklist.md)
- [Versions](versions.md)
