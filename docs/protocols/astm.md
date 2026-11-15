# ASTM: LIS01-A2 and LIS02-A2

Evidence level: documented. Phase 2.

Draft conformance checklist, written in our own words from the standard and
checked in an independent review on 2026-11-15. Section and page numbers
point into the standard; nothing here reproduces its text or tables. The
phase starts by confirming this scope (plan principle 7); the checklist then
becomes the test list, one or more tests per row.

## Scope decisions to confirm at phase start

- **Edition:** LIS01-A2 (2008) and LIS02-A2 (2004, printed as LIS2-A2).
  Both are archived by CLSI but still the current editions.
- **Frames:** strict mode follows LIS01-A2: up to 64,000 characters per
  frame including 7 bytes of overhead (63,993 of text). Smaller vendor
  limits (for example 247) are machine config.
- **Units of data:** a link session (ENQ to EOT) carries one or more H…L
  messages; a message is made of records; a long record is split across
  frames. Strict mode never packs two records in one frame; tolerating that
  is a vendor compatibility option.
- **TCP:** strict LIS01 over TCP keeps the full handshake, checksums and
  timers, with the computer as server by default. Direct LIS02 records over
  TCP without LIS01 framing, and reversed client/server roles, are separate
  profiles chosen in config. TCP packet boundaries never frame ASTM.
- **Queries:** either side may query; Analyte supports the search criteria
  it lists here and declares the rest unsupported.
- **Content:** the record hierarchy, sequence numbers, all result status
  codes and the processing ID (production, training, debug, QC) are kept;
  training and debug messages never reach production delivery.

## Checklist

M = mandatory behavior; C = mandatory semantics when a field/feature is used (not every field has to be sent); O = optional/recommended. Hardware rows require installation evidence. Optional search services may be explicitly unsupported; mandatory parsing cannot silently discard them. Make each row positive, invalid-input, boundary and recovery tests as applicable.

### LIS01 physical and profile

- [ ] M — §1/§3.3 p.1: distinguish session, record/message and frame.
- [ ] C — §4.3 p.2: agree encoding out of band; preserve raw bytes before decoding.
- [ ] O — §4.4.1 p.2: restrict large frames preferentially to TCP.
- [ ] C — §4.5 pp.2–3: document physical adaptations such as DB9/USB separately.
- [ ] M/hardware — §§5.1–5.2.1 p.3: qualify point-to-point EIA-232 signaling.
- [ ] M — §5.2.2 pp.3–4: support/default 8N1; use serial API for prescribed bit order and marking idle.
- [ ] O — §5.2.2.5 p.3: alternative data/parity/stop-bit settings are optional.
- [ ] M — §5.2.3.1 p.4: host supports 1200/2400/4800/9600; instrument at least one; prefer/default 9600 when multiple rates.
- [ ] O — §5.2.3.2 p.4: other rates optional.
- [ ] C/hardware — §§5.2.4–5.3 pp.4–5: qualify DB25 roles, signal contacts, shield grounding, connectors, locks and cable installation; explicit vendor adaptations.
- [ ] C/hardware — §§7.1–7.3 pp.10–12: qualify IEEE 802.3 network/cabling; do not apply serial-like start/stop wording in this section as application TCP bits.
- [ ] M — §8.2.1.1 p.13: strict profile computer server/instrument client; configurable destination address/port.

### LIS01 state machine (serial / TCP paired references)

- [ ] M — §6.1 pp.5–6 / §8.1 pp.12–13: neutral/establish/transfer/terminate; simplex stop-and-wait with alternate supervision.
- [ ] M — §6.2.1–.5 p.6 / §8.2.1–.5 p.13: initiate ENQ from neutral with data; ACK readiness before data; ignore establishment responses except ACK/NAK/ENQ; idle receiver ignores irrelevant bytes.
- [ ] M — §6.2.6–.7 p.6 / §8.2.6–.7 p.13: busy NAK, ≥10 s retry ENQ delay; receive-incapable NAKs, send-incapable never initiates.
- [ ] M — §6.2.7.1 p.6 / §8.2.7.1 pp.13–14: instrument priority, computer yields until new ENQ, instrument repeats after ≥1 s.
- [ ] M — §6.3.1 p.7 / §8.3.1 p.14: ≤64,000 total, ≤63,993 text; split longer records, never combine separate records in strict frames.
- [ ] M — §6.3.1.2 p.7 / §8.3.1.2 p.14: STX/FN/text/ETB-or-ETX/checksum/CR/LF; intermediate versus final segmentation.
- [ ] M — §6.3.2 p.7 / §8.3.2 p.14: ASCII 0–7, first 1, increment modulo eight only for new frame; same number on retransmission.
- [ ] M — §6.3.3 pp.7–8 / §8.3.3 p.15: raw-byte sum modulo 256, frame digit through terminator; exclude STX/checksum/trailer; two uppercase hex digits, high first.
- [ ] M — §6.3.4 p.8 / §8.3.4 p.15: wait per frame; ACK advances, NAK repeats, EOT response accepts and requests interruption.
- [ ] M — §6.3.5 pp.8–9 / §8.3.5 pp.15–16: interpret interrupt; ignoring requires repeat request to remain active; honoring terminates and yields for ≥15 s or peer send-and-release.
- [ ] O — same: issuing/honoring an interrupt optional; positive acknowledgment meaning mandatory.
- [ ] M — §6.4 p.9 / §8.4 p.16: sender EOT returns both neutral; no EOT ACK exchange.
- [ ] M — §6.5.1 p.9 / §8.5.1 p.16: reject character/checksum/sequence defects; accept prior or next modulo-eight number; irrelevant outside-frame bytes cannot pollute checksum.
- [ ] M — §6.5.1.2 p.9 / §8.5.1.2 p.16: unexpected reply except ACK/EOT is NAK condition; abort at six unaccepted sends of one frame.
- [ ] M — §6.5.2.1 p.9 / §8.5.2.1 p.16: 15 s ENQ response timeout → termination.
- [ ] M — §6.5.2.2 p.9 / §8.5.2.2 p.16: computer contention 20 s without new ENQ → neutral.
- [ ] M — §6.5.2.3 p.9 / §8.5.2.3 p.17: 15 s after last frame character without response → terminate; remember failed content for repetition.
- [ ] M — §6.5.2.4 p.10 / §8.5.2.4 p.17: receive timer starts at transfer entry/each reply; 30 s without frame/EOT → discard incomplete assembly, neutral.
- [ ] M — §6.5.2.5 p.10 / §8.5.2.5 p.17: reply within 15 s including processing/busy delay.
- [ ] O — §6.5.2.6 p.10 / §8.5.2.6 p.17: more than normal one outgoing-message buffer optional.
- [ ] M — §6.6 p.10 / §8.6 p.17: exclude SOH/STX/ETX/EOT/ENQ/ACK/DLE/NAK/SYN/ETB/LF/DC1–DC4 from payload; distinguish record CR from trailer.
- [ ] M — Appendix A p.19: reconcile every state/arrow/timer visually before signoff; outstanding in this review.

Source caveats: serial §6.3.1 uses an inconsistent payload threshold; honor its overhead-inclusive ceiling. §6.3.3.1/§8.3.3.1 do not explicitly name ETB/ETX in the addition sentence; inspected implementations/vendor notes include the terminator. Resolve wording against mandatory diagram/errata before locking normative tests. The instruction to ignore bytes after the block character in §6.5.1.1/§8.5.1.1 cannot mean ignoring required checksum validation. LIS2 §5.4's decimal number for backslash is a source typo: actual character is 0x5C, not decimal 96.

### LIS2 logical structure and syntax

- [ ] M — §4.1 pp.3–4: H/L level zero, P/Q/S level one, O two, R three; O needs P, R needs O; same/higher level closes prior scope.
- [ ] M — §4.1 p.3, §5.5.5/.8 pp.8–9: C/M contextual attachment, variable level, never after L.
- [ ] M — §4.2.1–.2 pp.4–5: save on upward hierarchy transition; repeat ancestors needed to restart last unsaved logical record.
- [ ] M — §5.1 p.5: specified Latin-1 repertoire and allowed controls, excluded values; scalar data excludes active separators; case-sensitive content except record IDs.
- [ ] C — §§5.2–5.3 pp.5–6: no standard maximum field/record; document agreed implementation limits and never silently truncate.
- [ ] M — §§5.4.1–.4 p.6,5.4.6 p.7,6.2 p.11: CR termination, distinct header-selected field/repeat/component/escape, consistent until L.
- [ ] M — §5.4.5 pp.6–7: escape-aware parsing, no nesting.
- [ ] O/C — §5.4.5.1 p.7: preserve/support optional H/N, F/S/R/E, variable-length X hex and Z local operations without changing boundaries.
- [ ] M — §§5.4.7–.9 pp.7–8,5.7 p.11: positional internal empties; tolerate omitted/present trailing null delimiters; null versus explicit two-quote deletion.
- [ ] C — §5.4.8 p.7: semantically unused fields may be ignored, positions still preserved.
- [ ] M — §5.5 p.8: type first, record IDs case-insensitive, CR end.
- [ ] C — §5.6.1 pp.9–10: four positional test-ID components with documented local extensions; sufficient identity for battery/specimen association.
- [ ] C — §5.6.2 p.10: YYYYMMDD, HHMMSS, combined up to fourteen digits, optional signed timezone; absent zone is sender's, not UTC.
- [ ] C — §5.6.3 p.10: phones free text, repeats allowed, first primary/daytime.
- [ ] C — §5.6.4 p.10: field-specific fixed-observation default units; alternate units in components, distinct from absent result-unit guessing.
- [ ] C — §§5.6.5–.6 pp.10–11: address/provider/name positions and repeated identities retained.
- [ ] M — §5.6.7 p.11: record sequence required for repeatable types, resets on higher context/changed level; independent of frame number.

### LIS2 records and conditional semantics

- [ ] M — §6 pp.11–13: H first; L before another H/session end; bootstrap header delimiters specially.
- [ ] C — §6.3 p.11: control ID identifies transmission, not persistent observation.
- [ ] C — §6.4 p.12: agreed password failure aborts and notifies access violation.
- [ ] C — §§6.5–.11 p.12: sender/receiver, sender details, reserved position and instructions retained.
- [ ] C — §6.12 p.12: P/T/D/Q processing; training has no production effect.
- [ ] C — §§6.13–.14 p.13: declared LIS2-A2 version and transmission time, vendor substitutions explicit.
- [ ] C — §§7.1–.35 pp.13–17: distinct patient IDs and all defined demographics retained where used; never require every optional demographic.
- [ ] C — §§7.8–.10 pp.13–14: preserve date precision and specified sex/race coding without inference.
- [ ] C — §§7.17–.18 pp.14–15: height/weight defaults and explicit alternate units.
- [ ] C — §§7.24–.35 pp.15–17: admission/discharge and coded clinical/administrative context distinct from specimen overrides.
- [ ] C — §§8.1–.3 pp.17–18: multiple tests share O only if all other attributes same; repeated specimen IDs allowed; reporting uses separate O per battery and R per observation.
- [ ] C — §§8.2–.3 pp.17–18: not-done O with X and no R is meaningful, not empty noise.
- [ ] C — §§8.4.1–.5 pp.18–19: O type/sequence, specimen ID distinct from instrument specimen ID and complete test identification.
- [ ] C — §8.4.6 p.19: S/A/R/C/P priorities; multiple priorities repeat-separated.
- [ ] C — §§8.4.7–.11 p.19: distinguish ordered/future requested time, actual collection/start/end, volume and collector.
- [ ] C — §8.4.12 p.20: cancel/add/new/pending/in-process/QC distinct, L reserved.
- [ ] C — §§8.4.13–.18 p.20: hazards, clinical context, receipt time, type/source and ordering clinician.
- [ ] C — §§8.4.19–.22 p.21: requester user fields roundtrip; lab fields retained.
- [ ] C — §§8.4.23–.25 p.21: report-change time, charge reference and performing section distinct.
- [ ] C — §8.4.26 p.21: O/C/P/F/X/I/Y/Z/Q; distinguish no order/no patient/query response.
- [ ] C — §§8.4.27–.31 pp.21–22: reserved position, collection location, infection flag, specimen service/institution.
- [ ] C — §§9.1–.4 p.22: analytic determinations separately identified; raw numeric/text/code values preserved; no implied independent subvalues.
- [ ] C — §9.5 p.23: retain units, standard abbreviations where available; not necessarily UCUM.
- [ ] C — §9.6 p.23: repeated ranges with component descriptions retained.
- [ ] C — §§9.7–.8 p.23: flags including LL/HH, combined A/S/R population basis or generic N.
- [ ] C — §9.9 pp.23–24: C/P/F/X/I/S/M/R/N/Q/V/W status meanings distinct.
- [ ] C — §9.10 p.24: normative/units change date empty if irrelevant; relevant changes trigger review rather than silent historical comparability.
- [ ] C — §§9.11–.14 p.24: operator/verifier, started/completed and instrument identities distinct.
- [ ] C — §§10.1–.5 pp.24–25: comment sequence/source/type and coded/text components with correct parent.
- [ ] O/C — §§13.1–.21 pp.28–29: scientific methods/instrument/reagent/QC/specimen/result/patient context preserved, not clinically interpreted.
- [ ] O/C — §14 p.30: M plus sequence and documented manufacturer-defined remainder; preserve unknown structures.

### LIS2 queries and completion

- [ ] M — §11 p.25: either side queries; one outstanding request; L completion or cancellation before next.
- [ ] C — §§11.1–.4 pp.25–26: Q sequence, positional patient/specimen/vendor ID components, repeated IDs, range end and direction-sensitive ALL.
- [ ] O — §11.3 p.26: full historical search/retention not mandated; declare practical retrieval subset.
- [ ] C — §11.5 p.26: repeated test IDs/ALL within other selection constraints.
- [ ] C — §§11.6–.8 p.26: S collection/R result basis, default R; beginning/repeated dates, open past and latest bound.
- [ ] C — §§11.9–.12 pp.26–27: requesting physician/phone/user fields.
- [ ] C — §11.13 p.27: result-state filters, pending/partial/new-only, MIC/previously sent; A cancel, O orders/demographics, D demographics only.
- [ ] M — §§12.1–.2 p.27: L last, sequence normally 1, next H a new message.
- [ ] C — §12.3 pp.27–28: normal/sender-abort/receiver-abort/system-error/query-error/no-information/finished distinct; I/Q terminate request.

### Additional safety tests owned by Analyte, not attributed to ASTM

- [ ] Frame commit before data ACK, capacity reservation before ENQ ACK, disk-full/commit failure refusal.
- [ ] Crash after write/before ACK, repeated previous frame, same-number conflicting payload, incomplete ETB record and repeated hierarchy ancestors.
- [ ] Split/merge every byte boundary; high-bit checksum before character decode; rollover and separate frame/record/message/session boundaries.
- [ ] Fixed bounds for trickles, frames, records and sessions, while retaining raw evidence.
- [ ] Application deadline versus link timer, late reply/cancel/cache uncertainty and no unintended worklist resend.
- [ ] No payload/credential logs, even trace.

## Field coverage index

Use this index to split the grouped conditional rows above into individual field fixtures. It lists field names, not reproduced standard content. Manufacturer-selected fields remain conditional under §4.1; parser positions and type/sequence/delimiter requirements remain mandatory. A field being indexed is not evidence its full validation test has been designed.

| Section | Printed page | Field |
|---|---|---|
| 6.1 | 11 | Record Type ID |
| 6.2 | 11 | Delimiter Definition |
| 6.3 | 11 | Message Control ID |
| 6.4 | 12 | Access Password |
| 6.5 | 12 | Sender Name or ID |
| 6.6 | 12 | Sender Street Address |
| 6.7 | 12 | Reserved Field |
| 6.8 | 12 | Sender Telephone Number |
| 6.9 | 12 | Characteristics of Sender |
| 6.10 | 12 | Receiver ID |
| 6.11 | 12 | Comment or Special Instructions |
| 6.12 | 12 | Processing ID |
| 6.13 | 13 | Version Number |
| 6.14 | 13 | Date and Time of Message |
| 7.1 | 13 | Record Type |
| 7.2 | 13 | Sequence Number |
| 7.3 | 13 | Practice-Assigned Patient ID |
| 7.4 | 13 | Laboratory-Assigned Patient ID |
| 7.5 | 13 | Patient ID Number 3 |
| 7.6 | 13 | Patient Name |
| 7.7 | 13 | Mother's Maiden Name |
| 7.8 | 13 | Birthdate |
| 7.9 | 14 | Patient Sex |
| 7.10 | 14 | Patient Race-Ethnic Origin |
| 7.11 | 14 | Patient Address |
| 7.12 | 14 | Reserved Field |
| 7.13 | 14 | Patient Telephone Number |
| 7.14 | 14 | Attending Physician ID |
| 7.15 | 14 | Special Field 1 |
| 7.16 | 14 | Special Field 2 |
| 7.17 | 14 | Patient Height |
| 7.18 | 15 | Patient Weight |
| 7.19 | 15 | Patient's Known or Suspected Diagnosis |
| 7.20 | 15 | Patient Active Medications |
| 7.21 | 15 | Patient's Diet |
| 7.22 | 15 | Practice Field Number 1 |
| 7.23 | 15 | Practice Field Number 2 |
| 7.24 | 15 | Admission and Discharge Dates |
| 7.25 | 15 | Admission Status |
| 7.26 | 15 | Location |
| 7.27 | 15 | Nature of Alternative Diagnostic Code and Classifiers |
| 7.28 | 16 | Alternative Diagnostic Code and Classification |
| 7.29 | 16 | Patient Religion |
| 7.30 | 16 | Marital Status |
| 7.31 | 16 | Isolation Status |
| 7.32 | 16 | Language |
| 7.33 | 17 | Hospital Service |
| 7.34 | 17 | Hospital Institution |
| 7.35 | 17 | Dosage Category |
| 8.4.1 | 18 | Record Type ID |
| 8.4.2 | 18 | Sequence Number |
| 8.4.3 | 18 | Specimen ID |
| 8.4.4 | 19 | Instrument Specimen ID |
| 8.4.5 | 19 | Universal Test ID |
| 8.4.6 | 19 | Priority |
| 8.4.7 | 19 | Requested/Ordered Date and Time |
| 8.4.8 | 19 | Specimen Collection Date and Time |
| 8.4.9 | 19 | Collection End Time |
| 8.4.10 | 19 | Collection Volume |
| 8.4.11 | 19 | Collector ID |
| 8.4.12 | 20 | Action Code |
| 8.4.13 | 20 | Danger Code |
| 8.4.14 | 20 | Relevant Clinical Information |
| 8.4.15 | 20 | Date/Time Specimen Received |
| 8.4.16 | 20 | Specimen Descriptor |
| 8.4.16.1 | 20 | Specimen Type |
| 8.4.16.2 | 20 | Specimen Source |
| 8.4.17 | 20 | Ordering Physician |
| 8.4.18 | 20 | Physician's Telephone Number |
| 8.4.19 | 21 | User Field Number 1 |
| 8.4.20 | 21 | User Field Number 2 |
| 8.4.21 | 21 | Laboratory Field Number 1 |
| 8.4.22 | 21 | Laboratory Field Number 2 |
| 8.4.23 | 21 | Date/Time Results Reported or Last Modified |
| 8.4.24 | 21 | Instrument Charge to Information System |
| 8.4.25 | 21 | Instrument Section ID |
| 8.4.26 | 21 | Report Types |
| 8.4.27 | 21 | Reserved Field |
| 8.4.28 | 22 | Location of Specimen Collection |
| 8.4.29 | 22 | Nosocomial Infection Flag |
| 8.4.30 | 22 | Specimen Service |
| 8.4.31 | 22 | Specimen Institution |
| 9.1 | 22 | Record Type ID |
| 9.2 | 22 | Sequence Number |
| 9.3 | 22 | Universal Test ID |
| 9.4 | 22 | Data or Measurement Value |
| 9.5 | 23 | Units |
| 9.6 | 23 | Reference Ranges |
| 9.7 | 23 | Result Abnormal Flags |
| 9.8 | 23 | Nature of Abnormality Testing |
| 9.9 | 23 | Result Status |
| 9.10 | 24 | Date of Change in Instrument Normative Values or Units |
| 9.11 | 24 | Operator Identification |
| 9.12 | 24 | Date/Time Test Started |
| 9.13 | 24 | Date/Time Test Completed |
| 9.14 | 24 | Instrument Identification |
| 10.1 | 24 | Record Type ID |
| 10.2 | 25 | Sequence Number |
| 10.3 | 25 | Comment Source |
| 10.4 | 25 | Comment Text |
| 10.5 | 25 | Comment Type |
| 11.1 | 25 | Record Type ID |
| 11.2 | 25 | Sequence Number |
| 11.3 | 25 | Starting Range ID Number |
| 11.4 | 26 | Ending Range ID Number |
| 11.5 | 26 | Universal Test ID |
| 11.6 | 26 | Nature of Request Time Limits |
| 11.7 | 26 | Beginning Request Results Date and Time |
| 11.8 | 26 | Ending Request Results Date and Time |
| 11.9 | 26 | Requesting Physician Name |
| 11.10 | 27 | Requesting Physician Telephone Number |
| 11.11 | 27 | User Field Number 1 |
| 11.12 | 27 | User Field Number 2 |
| 11.13 | 27 | Request Information Status Codes |
| 12.1 | 27 | Record Type ID |
| 12.2 | 27 | Sequence Number |
| 12.3 | 27 | Termination Code |
| 13.1 | 28 | Record Type ID |
| 13.2 | 28 | Sequence Number |
| 13.3 | 28 | Analytical Method |
| 13.4 | 28 | Instrumentation |
| 13.5 | 28 | Reagents |
| 13.6 | 28 | Units of Measure |
| 13.7 | 28 | Quality Control |
| 13.8 | 28 | Specimen Descriptor |
| 13.9 | 28 | Reserved Field |
| 13.10 | 29 | Container |
| 13.11 | 29 | Specimen ID |
| 13.12 | 29 | Analyte |
| 13.13 | 29 | Result |
| 13.14 | 29 | Result Units |
| 13.15 | 29 | Collection Date and Time |
| 13.16 | 29 | Result Date and Time |
| 13.17 | 29 | Analytical Preprocessing Steps |
| 13.18 | 29 | Patient Diagnosis |
| 13.19 | 29 | Patient Birthdate |
| 13.20 | 29 | Patient Sex |
| 13.21 | 29 | Patient Race |

## Tests suggested by open-source comparisons

Recommended additions to Analyte's test plan: delayed contention ENQ, six NAK versus silence, lost ACK exact repeat, EOT interrupt honored/ignored, 64,000 total boundaries, long-record rollover, multiple H…L per session, changing header separators, explicit deletion, O without R, C/M hierarchy changes, Q range/ALL/repeat/cancel and L I/Q/F, storage failures at every acknowledgment window. No inspected library is an official CLSI oracle.
