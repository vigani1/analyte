# ASTM conformance

## Checklist

M = mandatory behavior; C = mandatory semantics when a field/feature is used (not every field has to be sent); O = optional/recommended. Hardware rows require installation evidence. Optional search services may be explicitly unsupported; mandatory parsing cannot silently discard them. Make each row positive, invalid-input, boundary and recovery tests as applicable.

### LIS01 physical and profile

- [ ] M — §1/§3.3 p.1: distinguish session, record/message and frame. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §4.3 p.2: agree encoding out of band; preserve raw bytes before decoding. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] O — §4.4.1 p.2: restrict large frames preferentially to TCP. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §4.5 pp.2–3: document physical adaptations such as DB9/USB separately. Remaining: installation documentation and qualification of the selected adapter.
- [ ] M/hardware — §§5.1–5.2.1 p.3: qualify point-to-point EIA-232 signaling. Remaining: installation qualification.
- [ ] M — §5.2.2 pp.3–4: support/default 8N1; use serial API for prescribed bit order and marking idle. Remaining: qualification of the selected serial settings on installed hardware.
- [ ] O — §5.2.2.5 p.3: alternative data/parity/stop-bit settings are optional. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] M — §5.2.3.1 p.4: host supports 1200/2400/4800/9600; instrument at least one; prefer/default 9600 when multiple rates. Remaining: qualification of the selected serial settings on installed hardware.
- [ ] O — §5.2.3.2 p.4: other rates optional. Remaining: qualification of the selected serial settings on installed hardware.
- [ ] C/hardware — §§5.2.4–5.3 pp.4–5: qualify DB25 roles, signal contacts, shield grounding, connectors, locks and cable installation; explicit vendor adaptations. Remaining: installation qualification.
- [ ] C/hardware — §§7.1–7.3 pp.10–12: qualify IEEE 802.3 network/cabling; do not apply serial-like start/stop wording in this section as application TCP bits. Remaining: installation qualification.
- [ ] M — §8.2.1.1 p.13: strict profile computer server/instrument client; configurable destination address/port. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.

### LIS01 state machine (serial / TCP paired references)

- [ ] M — §6.1 pp.5–6 / §8.1 pp.12–13: neutral/establish/transfer/terminate; simplex stop-and-wait with alternate supervision. Test: `neutral_readiness_and_busy_replies`. Remaining: the cited test covers readiness only, not the complete phase cycle.
- [ ] M — §6.2.1–.5 p.6 / §8.2.1–.5 p.13: initiate ENQ from neutral with data; ACK readiness before data; ignore establishment responses except ACK/NAK/ENQ; idle receiver ignores irrelevant bytes. Test: `neutral_readiness_and_busy_replies`. Remaining: ignored establishment replies and data initiation need dedicated assertions.
- [ ] M — §6.2.6–.7 p.6 / §8.2.6–.7 p.13: busy NAK, ≥10 s retry ENQ delay; receive-incapable NAKs, send-incapable never initiates. Test: `neutral_readiness_and_busy_replies`. Remaining: the cited test does not exercise the sender retry delay or send-incapable role.
- [ ] M — §6.2.7.1 p.6 / §8.2.7.1 pp.13–14: instrument priority, computer yields until new ENQ, instrument repeats after ≥1 s. Test: `contention_in_both_roles`. Remaining: computer yielding until a new ENQ needs an assertion.
- [x] M — §6.3.1 p.7 / §8.3.1 p.14: ≤64,000 total, ≤63,993 text; split longer records, never combine separate records in strict frames. Tests: `long_records_split_and_numbers_roll_over`, `separate_records_never_share_strict_frame`.
- [ ] M — §6.3.1.2 p.7 / §8.3.1.2 p.14: STX/FN/text/ETB-or-ETX/checksum/CR/LF; intermediate versus final segmentation. Test: `long_records_split_and_numbers_roll_over`. Remaining: independently assert every frame field and ETB/ETX distinction; the cited test reuses the encoder as its oracle.
- [ ] M — §6.3.2 p.7 / §8.3.2 p.14: ASCII 0–7, first 1, increment modulo eight only for new frame; same number on retransmission. Test: `committed_duplicate_acks_and_conflict_naks`. Remaining: the cited test does not exercise modulo-eight receive rollover.
- [ ] M — §6.3.3 pp.7–8 / §8.3.3 p.15: raw-byte sum modulo 256, frame digit through terminator; exclude STX/checksum/trailer; two uppercase hex digits, high first. Test: `rejects_bad_checksum_sequence_and_controls`. Remaining: independent checksum byte arithmetic, high-bit data and uppercase policy assertions.
- [ ] M — §6.3.4 p.8 / §8.3.4 p.15: wait per frame; ACK advances, NAK repeats, EOT response accepts and requests interruption. Test: `sender_aborts_after_six_failures`. Remaining: the cited test does not exercise EOT interruption acceptance.
- [ ] M — §6.3.5 pp.8–9 / §8.3.5 pp.15–16: interpret interrupt; ignoring requires repeat request to remain active; honoring terminates and yields for ≥15 s or peer send-and-release. Test: `interrupt_honored_ignored_and_issued`. Remaining: repeated interrupt request and peer send-and-release assertions.
- [x] O — same: issuing/honoring an interrupt optional; positive acknowledgment meaning mandatory. Test: `interrupt_honored_ignored_and_issued`.
- [ ] M — §6.4 p.9 / §8.4 p.16: sender EOT returns both neutral; no EOT ACK exchange. Test: `sender_aborts_after_six_failures`. Remaining: assert receiver neutral state and absence of EOT reply.
- [ ] M — §6.5.1 p.9 / §8.5.1 p.16: reject character/checksum/sequence defects; accept prior or next modulo-eight number; irrelevant outside-frame bytes cannot pollute checksum. Test: `rejects_bad_checksum_sequence_and_controls`. Remaining: outside-frame noise and all defect classes need assertions.
- [x] M — §6.5.1.2 p.9 / §8.5.1.2 p.16: unexpected reply except ACK/EOT is NAK condition; abort at six unaccepted sends of one frame. Test: `sender_aborts_after_six_failures`.
- [x] M — §6.5.2.1 p.9 / §8.5.2.1 p.16: 15 s ENQ response timeout → termination. Test: `busy_and_reply_timeouts`.
- [ ] M — §6.5.2.2 p.9 / §8.5.2.2 p.16: computer contention 20 s without new ENQ → neutral. Test: `contention_in_both_roles`. Remaining: assert successful neutral-state behavior after contention timeout.
- [ ] M — §6.5.2.3 p.9 / §8.5.2.3 p.17: 15 s after last frame character without response → terminate; remember failed content for repetition. Test: `busy_and_reply_timeouts`. Remaining: retained failed content needs an assertion.
- [ ] M — §6.5.2.4 p.10 / §8.5.2.4 p.17: receive timer starts at transfer entry/each reply; 30 s without frame/EOT → discard incomplete assembly, neutral. Test: `etb_timeout_and_split_merged_reads`. Also `nak_at_29s_expires_at_59s` covers NAK renewal. Remaining: assert discarded incomplete assembly and neutral-state behavior after timeout; timeout and NAK renewal are tested.
- [x] M — §6.5.2.5 p.10 / §8.5.2.5 p.17: reply within 15 s including processing/busy delay. Test: `failed_or_unanswered_commit_never_acks`.
- [x] O — §6.5.2.6 p.10 / §8.5.2.6 p.17: more than normal one outgoing-message buffer optional. Test: `one_message_buffer_and_validated_settings`.
- [ ] M — §6.6 p.10 / §8.6 p.17: exclude SOH/STX/ETX/EOT/ENQ/ACK/DLE/NAK/SYN/ETB/LF/DC1–DC4 from payload; distinguish record CR from trailer. Test: `rejects_bad_checksum_sequence_and_controls`. Remaining: the cited test exercises LF only, not the complete restricted character set.
- [ ] M — Appendix A p.19: reconcile every state/arrow/timer visually before signoff; visually inspected, with transition coverage. Test: `contention_in_both_roles`. Remaining: the cited contention test does not cover every diagram transition.

Source caveats: serial §6.3.1 uses an inconsistent payload threshold; honor its overhead-inclusive ceiling. §6.3.3.1/§8.3.3.1 do not explicitly name ETB/ETX in the addition sentence; inspected implementations/vendor notes include the terminator. The visually inspected Appendix A and the transition tests in [dialog evidence](../../../crates/steps/dialog/DETAILS.md#checklist-evidence) document the implemented interpretation. The instruction to ignore bytes after the block character in §6.5.1.1/§8.5.1.1 cannot mean ignoring required checksum validation. LIS2 §5.4's decimal number for backslash is a source typo: actual character is 0x5C, not decimal 96.

### LIS2 logical structure and syntax

- [ ] M — §4.1 pp.3–4: H/L level zero, P/Q/S level one, O two, R three; O needs P, R needs O; same/higher level closes prior scope. Test: `record_parents_and_context_closure`. Remaining: explicit parent assertions for every listed level and closure.
- [ ] M — §4.1 p.3, §5.5.5/.8 pp.8–9: C/M contextual attachment, variable level, never after L. Test: `record_parents_and_context_closure`. Tests: `consecutive_comments_share_their_owner`, `comment_and_manufacturer_level_changes_reset_sequence`. Remaining: full contextual permutations.
- [ ] M — §4.2.1–.2 pp.4–5: save on upward hierarchy transition; repeat ancestors needed to restart last unsaved logical record. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] M — §5.1 p.5: specified Latin-1 repertoire and allowed controls, excluded values; scalar data excludes active separators; case-sensitive content except record IDs. Test: `latin1_is_not_utf8_or_lossy`. Literal text controls: `text_controls_kept_byte_for_byte` (deliberate tolerance below). Remaining: separator exclusion and case sensitivity are not fully asserted.
- [x] **Deliberate tolerance:** §5.1 p.5: a literal BEL, HT, VT or FF inside a text or comment field is accepted instead of refusing the record. The value is kept byte for byte and each field gets a visible astm_control_character_in_text note in the receipt's departure evidence. Every other control, the record CR, the frame controls and the header-selected delimiters are still refused or stay syntax; identifiers still refuse controls. Reason: §5.1 allows these four bytes in a record, they change no boundary and no value, and refusing a whole result over a TAB in a comment loses data. Tests: `text_controls_kept_byte_for_byte`, `astm_comment_tab_kept_with_note`.
- [x] C — §§5.2–5.3 pp.5–6: no standard maximum field/record; document agreed implementation limits and never silently truncate. Test: `limits_reject_instead_of_truncating`.
- [ ] M — §§5.4.1–.4 p.6,5.4.6 p.7,6.2 p.11: CR termination, distinct header-selected field/repeat/component/escape, consistent until L. Test: `header_bootstraps_message_delimiters`. Remaining: duplicate separator rejection and complete delimiter consistency assertions.
- [ ] M — §5.4.5 pp.6–7: escape-aware parsing, no nesting. Test: `escapes_never_change_boundaries`. Remaining: nested escapes need an explicit rejection assertion.
- [ ] O/C — §5.4.5.1 p.7: preserve/support optional H/N, F/S/R/E, variable-length X hex and Z local operations without changing boundaries. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] M — §§5.4.7–.9 pp.7–8,5.7 p.11: positional internal empties; tolerate omitted/present trailing null delimiters; null versus explicit two-quote deletion. Test: `null_positions_and_explicit_deletion`. Remaining: equivalent omitted and present trailing-null forms need an assertion.
- [ ] C — §5.4.8 p.7: semantically unused fields may be ignored, positions still preserved. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] M — §5.5 p.8: type first, record IDs case-insensitive, CR end. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §5.6.1 pp.9–10: four positional test-ID components with documented local extensions; sufficient identity for battery/specimen association. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §5.6.2 p.10: YYYYMMDD, HHMMSS, combined up to fourteen digits, optional signed timezone; absent zone is sender's, not UTC. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §5.6.3 p.10: phones free text, repeats allowed, first primary/daytime. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §5.6.4 p.10: field-specific fixed-observation default units; alternate units in components, distinct from absent result-unit guessing. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§5.6.5–.6 pp.10–11: address/provider/name positions and repeated identities retained. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] M — §5.6.7 p.11: record sequence required for repeatable types, resets on higher context/changed level; independent of frame number. Test: `sequence_numbers_reset_with_context`. Changed-level reset and invalid continuation: `comment_and_manufacturer_level_changes_reset_sequence`. Remaining: full reset matrix.

### LIS2 records and conditional semantics

- [ ] M — §6 pp.11–13: H first; L before another H/session end; bootstrap header delimiters specially. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §6.3 p.11: control ID identifies transmission, not persistent observation. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §6.4 p.12: agreed password failure aborts and notifies access violation. Test: `sender_aborts_after_six_failures`. Remaining: no password configuration or access-violation notification; section 6.4 does not define a wire notification format.
- [ ] C — §§6.5–.11 p.12: sender/receiver, sender details, reserved position and instructions retained. Generated receiver test: `generated_header_identifies_receiver` (configured or absent; see decisions). Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §6.12 p.12: P/T/D/Q processing; training has no production effect. Test: `processing_and_termination_codes`. Additional exact-field guard: `composite_processing_id_is_refused`. Remaining: result delivery qualification for all processing contexts; the query regression now covers T/D/Q refusing real orders.
- [ ] C — §§6.13–.14 p.13: declared LIS2-A2 version and transmission time, vendor substitutions explicit. Test: `neutral_readiness_and_busy_replies`. Remaining: vendor edition substitutions are not supported. Standard generated headers now have LIS2-A2 and UTC time; tests: `cancel_and_unsupported_filters_are_not_empty_orders`, `generated_header_has_utc_time`.
- [ ] C — §§7.1–.35 pp.13–17: distinct patient IDs and all defined demographics retained where used; never require every optional demographic. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§7.8–.10 pp.13–14: preserve date precision and specified sex/race coding without inference. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§7.17–.18 pp.14–15: height/weight defaults and explicit alternate units. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§7.24–.35 pp.15–17: admission/discharge and coded clinical/administrative context distinct from specimen overrides. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.1–.3 pp.17–18: multiple tests share O only if all other attributes same; repeated specimen IDs allowed; reporting uses separate O per battery and R per observation. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.2–.3 pp.17–18: not-done O with X and no R is meaningful, not empty noise. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.4.1–.5 pp.18–19: O type/sequence, specimen ID distinct from instrument specimen ID and complete test identification. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §8.4.6 p.19: S/A/R/C/P priorities; multiple priorities repeat-separated. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.4.7–.11 p.19: distinguish ordered/future requested time, actual collection/start/end, volume and collector. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §8.4.12 p.20: cancel/add/new/pending/in-process/QC distinct, L reserved. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.4.13–.18 p.20: hazards, clinical context, receipt time, type/source and ordering clinician. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.4.19–.22 p.21: requester user fields roundtrip; lab fields retained. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.4.23–.25 p.21: report-change time, charge reference and performing section distinct. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §8.4.26 p.21: O/C/P/F/X/I/Y/Z/Q; distinguish no order/no patient/query response. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§8.4.27–.31 pp.21–22: reserved position, collection location, infection flag, specimen service/institution. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§9.1–.4 p.22: analytic determinations separately identified; raw numeric/text/code values preserved; no implied independent subvalues. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §9.5 p.23: retain units, standard abbreviations where available; not necessarily UCUM. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §9.6 p.23: repeated ranges with component descriptions retained. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§9.7–.8 p.23: flags including LL/HH, combined A/S/R population basis or generic N. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [x] C — §9.9 pp.23–24: C/P/F/X/I/S/M/R/N/Q/V/W status meanings distinct. Tests: `each_status_code_has_specified_meaning`, `result_status_codes_remain_distinct`.
- [ ] C — §9.10 p.24: normative/units change date empty if irrelevant; relevant changes trigger review rather than silent historical comparability. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§9.11–.14 p.24: operator/verifier, started/completed and instrument identities distinct. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §§10.1–.5 pp.24–25: comment sequence/source/type and coded/text components with correct parent. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] O/C — §§13.1–.21 pp.28–29: scientific methods/instrument/reagent/QC/specimen/result/patient context preserved, not clinically interpreted. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] O/C — §14 p.30: M plus sequence and documented manufacturer-defined remainder; preserve unknown structures. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.

### LIS2 queries and completion

- [ ] M — §11 p.25: either side queries; one outstanding request; L completion or cancellation before next. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] C — §§11.1–.4 pp.25–26: Q sequence, positional patient/specimen/vendor ID components, repeated IDs, range end and direction-sensitive ALL. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] O — §11.3 p.26: full historical search/retention not mandated; declare practical retrieval subset. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] C — §11.5 p.26: repeated test IDs/ALL within other selection constraints. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] C — §§11.6–.8 p.26: S collection/R result basis, default R; beginning/repeated dates, open past and latest bound. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] C — §§11.9–.12 pp.26–27: requesting physician/phone/user fields. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] C — §11.13 p.27: result-state filters, pending/partial/new-only, MIC/previously sent; A cancel, O orders/demographics, D demographics only. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] M — §§12.1–.2 p.27: L last, sequence normally 1, next H a new message. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] C — §12.3 pp.27–28: normal/sender-abort/receiver-abort/system-error/query-error/no-information/finished distinct; I/Q terminate request. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.

### Additional safety tests owned by Analyte, not attributed to ASTM

- [x] Frame commit before data ACK, capacity reservation before ENQ ACK, disk-full/commit failure refusal. Temporal proof: `restart_after_store_before_ack_delivers_once` in [paused custody test](../../../crates/analyte/tests/delivery/credentials.rs). Tests: `failed_or_unanswered_commit_never_acks`, `astm_journal_precedes_ack_and_is_interpreted`, `full_disk_refuses_enq_with_bounded_shutdown` in [host tests](../../../crates/analyte/tests/receive/connections.rs).
- [x] A store failure mid-transfer or at EOT parks the transfer with its acknowledged frames; operator reprocessing rebuilds them once into a receipt with the frames' exact bytes (ADR 0003 rules 1 and 2). Tests: `disk_full_mid_transfer_rebuilds_receipt`, `disk_full_at_eot_rebuilds_and_delivers` in [host custody tests](../../../crates/analyte/tests/receive/custody.rs).
- [ ] Crash after write/before ACK, repeated previous frame, same-number conflicting payload, incomplete ETB record and repeated hierarchy ancestors. Crash after write/before ACK: `hard_kill_before_ack_parks_stored_frame` hard-kills the real program there and checks the frame is parked, kept and never acknowledged, in [recovery tests](../../../crates/analyte/tests/service/recovery.rs). Remaining: a named test covering the rest of the row, beyond parser retention and the link tests listed above.
- [ ] Split/merge every byte boundary; high-bit checksum before character decode; rollover and separate frame/record/message/session boundaries. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] Fixed bounds for trickles, frames, records and sessions, while retaining raw evidence. Remaining: a named test covering the complete row, beyond parser retention and the link tests listed above.
- [ ] Application deadline versus link timer, late reply/cancel/cache uncertainty and no unintended worklist resend. Remaining: full search/filter qualification beyond exact-sample order queries; see [program order tests](../../../crates/analyte/tests/orders/cases.rs).
- [ ] No payload/credential logs, even trace. Remaining: a dedicated trace-level log-scrubbing assertion for every error path.

## Record coverage summary

The parser retains positional fields without requiring every optional field.
Counts include type/sequence/delimiter positions. They describe retained field
positions, not complete semantic validation. All rows use
[syntax field-position tests](../../../crates/protocols/astm/tests/syntax/grammar.rs),
`read_only_field_positions`.

| Record role | Positions handled | Coverage |
| --- | --- | --- |
| Header | 14 | Message envelope and declared processing context |
| Patient | 35 | Supplied identity and administrative context |
| Order | 31 | Specimen, requested work and source context |
| Result | 14 | Original observation and status metadata |
| Comment | 5 | Context-attached annotations |
| Query | 13 | Selection criteria retained for host interpretation |
| Terminator | 3 | Completion reason |
| Scientific context | 21 | Source methods/material metadata retained |
| Manufacturer | Variable | All 8 fixture positions, sequence and exact round-trip asserted by `all_manufacturer_positions_are_retained` |

## Comparison-derived test scope

Comparison-derived cases (several now covered above; broader query searches and hardware evidence remain): delayed contention ENQ, six NAK versus silence, lost ACK exact repeat, EOT interrupt honored/ignored, 64,000 total boundaries, long-record rollover, multiple H…L per session, changing header separators, explicit deletion, O without R, C/M hierarchy changes, Q range/ALL/repeat/cancel and L I/Q/F, storage failures at every acknowledgment window. No inspected library is an official CLSI oracle.

## Configured result upload rules

All cases use synthetic uploads in the [mapping suite](../../../crates/steps/mapping/tests/astm_results.rs).

| Source rule | Test |
| --- | --- |
| Roche HIM v2.3 Table 18, qualitative code and cutoff | `qualitative_code_and_cutoff_keep_distinct_meanings` |
| Roche HIM v2.3 Table 18, quantitative comparator and message code, rerun status, unknown kind | `quantitative_comparator_and_message_code_stay_separate` |
| Roche HIM v2.3 Table 18, seven-space absence | `seven_spaces_are_absent_instead_of_a_numeric_value` |
| Roche HIM v2.3 Tables 17–18, ratio and predilution | `dilution_preserves_both_indicators_without_multiplying` |
| Roche HIM v2.3 Tables 19–20, result alarm | `result_alarms_keep_the_dialects_own_number_and_comment` |
| Roche HIM v2.3 Tables 3 and 16, patient and generated IDs, QC names and ambiguous lot mode | `generated_and_qc_labels_never_become_patient_ids` |
| Biolyte Appendix VI sections 9.4.16, 9.4.26, 10.1.3–4 and p. 86, method, padding, descriptor, report and observation status | `method_specimen_and_report_status_are_independent` |
| Biolyte Appendix VI section 10.1.4, over-width text retention | `over_width_notation_is_kept_for_review` |
| Biolyte Appendix VI section 11.1.3, sender abort and normal termination | `sender_abort_holds_retained_measurements` |
