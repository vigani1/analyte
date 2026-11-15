# IHE LAW (PaLM Technical Framework Rev 11.0)

Evidence level: documented, then simulated (IHE Gazelle). Phase 4.

Draft conformance checklist, written in our own words from the standard and
checked in an independent review on 2026-11-15. Section and page numbers
point into the standard; nothing here reproduces its text or tables. The
phase starts by confirming this scope (plan principle 7); the checklist then
becomes the test list, one or more tests per row.

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
- [ ] **R** Answer supported peer query forms, support broadcast and assume query mode unless configured otherwise (TF-1 §5.1.1.2, p.34).
- [ ] **R** Apply LAW M/R/RE/RE.AN/C/X usages, cardinality, length, data-type and empty/null rules (TF-2b §§2.2.1–2.2.3, pp.9–14; Appendix A, p.59).
- [ ] **R** Support enhanced content and implementation profiles; transaction ID first in MSH-21, correct option identifiers and conditional usages (TF-2b §2.2.4, pp.14–24; C.4, pp.90–95).
- [ ] **S** Send enhanced data only to supporting peers (TF-1 §5.1.1.2, p.34).
- [ ] **R** Use directional MLLP initiating connections; tolerate persistent/per-transaction connections and independent establishment (TF-2b B.6, pp.79–80; TF-2x §2.2.4 dependency).
- [ ] **R** Use NE/AL application acknowledgement mode and correct header reversal/MSA-2 correlation; do not reject incoming application/facility names merely for differing from configured values (TF-2b B.5, pp.78–79; HL7 §2.9, pp.2-24–2-28, §2.15.8.2).

### Query/work

- [ ] **R** Parse QBP and return correctly correlated RSP, distinguishing acceptance/error from actual work availability (TF-2b §3.27.4, pp.34–44).
- [ ] **R/C** Handle container, isolate, rack/position, tray/position, all-pending and informational location forms as applicable to connected peers (TF-1 §§5.2.1–5.2.5, pp.36–37; TF-2b QPD definition, pp.35–41).
- [ ] **R** Send work in OML and process ORL for every AWOS, including all applicable groups/fields (TF-2b §3.28.4.1.2, pp.47–51).
- [ ] **R** Distinguish new-work OK/UA and cancel CR/UC from message acceptance (TF-2b §3.28.4.1.3, pp.51–52).
- [ ] **R** Use restricted negative LAB-28 structure for genuine no-work; preserve query scope and analyzer-specific deadline (TF-2b §2.2.4.2, p.16; §§3.27.4–3.28.4, pp.34–52).
- [ ] **R** Preserve uncertain prior acceptance; duplicate-AWOS UA must not undo the original accepted request (TF-2b §3.28.4.1.3, p.51).
- [ ] **R** Use separate AWOS for separately requested replicates (TF-2b B.8, p.83).

### Results/schema/errors

- [ ] **R** Receive OUL and return ACK with required structures/fields; correlate known AWOS to originating work (TF-2b §3.29.4, pp.53–58).
- [ ] **R** Support unsolicited/unassigned work cases; do not reject all results merely because no downloaded AWOS exists (TF-1 §5.4.2; TF-2b §3.29.4, C.8, pp.53–58,113–118).
- [ ] **R** Preserve specimen/container/device identities (TF-2b B.1–B.2, pp.60–63; Appendix A, p.59; HL7 §§2.A.25–2.A.26, pp.2-164–2-165).
- [ ] **R** Use AWOS plus OBX-3/4 for observation/run identity, retain preliminary/final/correction and retransmission semantics (TF-2b §3.29.4.1.3, pp.57–58; B.3, pp.63–77; base HL7 §7.4.2).
- [ ] **R** Preserve supplemental/primary relationships, typed values, units, statuses and scoped notes; do not treat NTE prose as executable rules (TF-2b B.3; C.5/C.7, pp.95–113).
- [ ] **R** Implement every applicable manager usage for ERR, INV, MSA, MSH, NTE, OBR, OBX, ORC, PID, PV1, SAC, SPM, TCD and TQ1, including component usages and conditional predicates (TF-2b Appendix C, pp.84–141).
- [ ] **R** Apply coding-system identification, test/result vocabulary and units rules without inventing codes (TF-2b §2.3, pp.24–31).
- [ ] **R** Distinguish malformed-trigger AE, control-content AR, invalid ACK failure, inconsistent ORL failure and query processing rejection; report locations where required (TF-2b B.7.1–B.7.8, pp.80–82).
- [ ] **R** Reject inconsistent LAB-29 as a whole, including TCD-1/OBX-3 mismatch; retain bytes without applying a subset (TF-2b p.57; B.7.6, p.82).
- [ ] **R** Keep manager-sourced patient data authoritative; correct downloaded patient information through cancel/new-AWOS, not repeated work (TF-2b B.7.9, pp.82–83).
- [ ] **R** Handle unknown/non-HL7 input without compromising the application (TF-2b B.7.10, p.83).

### Enhanced content and manager options

These are optional Analyzer capabilities, but relevant manager enhanced-content support is required; outgoing use is conditional on the peer and available app-supplied information. They do not require Analyte to measure or make clinical rerun/reflex decisions.

- [ ] **R/C** Preserve contributing substance/disposable identifiers, types and available lot/container data (TF-1 §5.2.6, p.37; TF-2b §2.2.4.3, p.17; C.2).
- [ ] **R/C** Send explicit requested dilution and retain actual dilution/value meaning (TF-1 §5.2.7, pp.37–38; TF-2b §2.2.4.4, pp.17–18; C.13).
- [ ] **R/C** Handle profiled patient/encounter demographics without invented values (TF-1 §5.2.8, p.38; TF-2b §2.2.4.5, pp.18–19).
- [ ] **R/C** Preserve Analyzer reflex parent/child relationships (TF-1 §5.2.9, p.38; TF-2b §2.2.4.6, p.19; B.4, pp.77–78).
- [ ] **R/C** Preserve Analyzer-initiated reruns and run identifiers (TF-1 §5.2.10, p.38; TF-2b §2.2.4.7, p.19; B.3).
- [ ] **R/C** Encode app-authorized manager rerun/reflex as new AWOS with repeat status (TF-1 §5.2.11, pp.38–39; TF-2b §2.2.4.8, p.20).
- [ ] **R/C** Encode authorized automatic-repeat/reflex permissions for supporting peers (TF-1 §5.2.12, p.39; TF-2b §2.2.4.9, p.20).
- [ ] **R/C** Preserve requested and applied priority (TF-1 §5.2.13, p.39; TF-2b §2.2.4.10, pp.20–21).
- [ ] **R/C** Preserve specimen/parent identities and applicable collection/source/risk/time data (TF-1 §5.2.14, p.39; TF-2b §2.2.4.11, p.21).
- [ ] **R/C** Handle container carrier/volume/units/dilution data (TF-1 §5.2.15, pp.39–40; TF-2b §2.2.4.12, p.21).
- [ ] **R/C** Preserve mass-spectrometry probability (TF-1 §5.2.16, p.40; TF-2b §2.2.4.13, p.22).
- [ ] **R/C** Preserve external-result pointers and persistence classification (TF-1 §5.2.17, p.40; TF-2b §2.2.4.15, p.22; B.3.12, p.77).
- [ ] **R/C** Support enhanced elements outside named options: error detail, comments, grouping, providers, ranges, instance IDs, timing and consumption (TF-2b §2.2.4.18, pp.23–24).
- [ ] **O/C** Related observations: if selected, send supplied related observations to supporting peers (TF-1 §5.2.18, p.40; TF-2b §2.2.4.14, p.22).
- [ ] **O/C** Pooling on Analyzer: if selected, request authorized pool size and interpret actual size (TF-1 §5.2.19, p.40; TF-2b §2.2.4.16, pp.22–23).
- [ ] **O/C** Pooling outside Analyzer: if selected, send pooled-specimen characteristics with restricted patient/specimen structures and required echoes (TF-1 §5.2.20, p.41; TF-2b §2.2.4.17, p.23).
- [ ] **O** Declare any security/cross-profile groupings separately; LAW itself lists no required actor grouping (TF-1 §§5.3,5.5–5.6, pp.41,54).
- [ ] **R, project gate** Trace requirements/field predicates to tests and qualified simulator evidence; include lost ACK, restart, storage failure, cancellation race and selected-option cases.

## Extra acceptance cases

Add explicit acceptance cases for: result counts differing from message/topic counts; characters split across reads; socket EOF; remote test-tool access to a SUT; changing validation profiles; one saved downstream fragment versus custody of the complete acknowledged message; duplicate observations after restart; uncertain order acceptance; and external-result pointers whose data has not been retained. General safety rules already cover some risks, but phase gates need concrete protocol cases.
