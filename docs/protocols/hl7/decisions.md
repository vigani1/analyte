# HL7 decisions

Scope choices and conformance requirements are catalogued together. Status
remains in [checklist.md](checklist.md); unchecked requirements are not claims
of implementation. Original checklist references are retained there. Source:
not found means the original section has not been independently verified for
this catalogue. Existing named tests may cover only part of a requirement.

## 1. Scope choice

- **what**: **Not all of HL7.** Analyte supports the encoding and control rules of both editions completely, plus a declared list of laboratory transactions. The first list: ORU^R01 with ACK (both editions); ORM^O01 and ORR^O02 (both editions); non-LAW OML^O21 and ORL^O22 (2.5.1); OML^O33 and ORL^O34, OUL^R22 and ACK^R22, QBP^Q11 and RSP^K11 (2.5.1, also used by IHE LAW); the legacy QRY^Q02, QCK^Q02 and DSR^Q03 exchange (both editions). Everything else is rejected with a specific, safe answer, never silently dropped.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 2. Scope choice

- **what**: **Who is the receiving application.** Each connection declares that Analyte takes on store-and-forward responsibility. It answers AA only for messages it durably stored *and* could interpret. Input it stored but could not interpret is answered AE or AR as the standard prescribes and parked for an operator. Enhanced mode (CA/CE/CR) follows MSH-15/16 exactly.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 3. Scope choice

- **what**: **Identity.** An observation is identified by its code and code system, its sub-ID and its order context, never by OBX-1, test name or value. Several OBX segments with the same key can form one replacement unit. OBX-11 is kept as sent; Analyte assigns its own revision numbers, because a correction can arrive without the original. OBX-21 is not an identifier in either edition (absent in 2.3.1, reserved in 2.5.1).
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 4. Scope choice

- **what**: **Groups.** Parsing keeps the patient/order/observation/specimen group each segment belongs to; field paths are relative to their group.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 5. Scope choice

- **what**: **Character sets.** Each connection declares its encoding (ASCII, ISO 8859-1 or UTF-8); MSH-18 is checked against it. The `UNICODE UTF-8` code is standard in 2.5.1 only; accepting it in 2.3.1 requires `syntax.allow_utf8_in_v231 = true` and a named deviation with the same key and a reason. Undecodable bytes are kept and reported, never replaced.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 6. Scope choice

- **what**: **Vendor departures** (repurposed fields, reused control IDs in responses, QC arrays in OBR) live in machine configs as named deviations, never in the standard code or the standard test suite.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 7. Conformance choice

- **what**: B — Declare supported version/event/structure/role combinations and extensions; do not claim all HL7 from an ORU demonstration. 231 ch.2 §§2.1, 2.11 pp.37, 106–107; 251 ch.2 §§2.12–2.13 pp.40–60.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 8. Conformance choice

- **what**: B — Select version and message semantics from validated MSH fields, not from familiar segment content. 231 ch.2 §2.12.1 pp.107–112; 251 ch.2 §2.9 pp.24–28.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 9. Conformance choice

- **what**: 251 — Publish static and dynamic profile constraints, usage, cardinalities, predicates, tables and identifier, and validate profile claims. Ch.2 §2.12 pp.40–58; MSH-21 §2.15.9.21 p.82.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 10. Conformance choice

- **what**: B — Separate structural parse success, base conformance, profile conformance and successful bridge processing. 231 ch.2 §§2.10–2.12 pp.104–112; 251 ch.2 §§2.6, 2.9, 2.12 pp.12–17, 24–28, 40–58.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 11. Conformance choice

- **what**: B — Preserve unknown/local content with an explicit compatibility policy; do not invent semantics for Z segments or extra fields. 231 ch.2 §§2.5, 2.10.2 pp.41, 105–106; 251 ch.2 §§2.8, 2.11 pp.20–24, 37–40.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 12. Conformance choice

- **what**: B — Generate standard fixtures separately from named vendor deviations and record their source/evidence level. Same version/profile rules; AGENTS evidence policy (engineering gate).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 13. Conformance choice

- **what**: B — Discover field separator from MSH-1 and the ordered component/repetition/escape/subcomponent characters from MSH-2. 231 ch.2 §§2.7, 2.24.1.1–2 pp.43–44, 130; 251 ch.2 §§2.5.4, 2.15.9.1–2 pp.11–12, 75.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 14. Conformance choice

- **what**: B — Handle MSH's special field indexing and CR segment terminators. Same delimiter clauses.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 15. Conformance choice

- **what**: B — Parse all field, repetition, component and subcomponent positions, preserving empty/trailing positions and cardinality. 231 ch.2 §§2.6–2.7, 2.10 pp.41–44, 104–105; 251 ch.2 §§2.5.3–2.6 pp.7–17.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 16. Conformance choice

- **what**: B — Distinguish explicit null, empty/unvalued, and absent data according to the message's update semantics. 231 ch.2 §§2.6, 2.10 pp.41–42, 104–105; 251 ch.2 §§2.5.3, 2.6 pp.7–17.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 17. Conformance choice

- **what**: B — Encode/decode literal delimiter escapes using the declared escape character without creating spurious structure. 231 ch.2 §2.9 pp.101–104; 251 ch.2 §2.7 pp.17–20.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 18. Conformance choice

- **what**: B — Support or specifically flag formatting/highlighting, hex, local and character-switch escapes; preserve unsupported content. Same escape clauses; do not silently flatten FT instructions.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 19. Conformance choice

- **what**: B — Detect malformed escapes safely and retain original spelling; structure is parsed before value de-escaping. Same escape/construction clauses plus project retention rule.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 20. Conformance choice

- **what**: B — Preserve nested patient/order/specimen/observation groups and NTE ownership rather than flattening segment lists. 231 ch.7 §7.2.1 pp.487–488; 251 ch.2 §2.5.2 p.6, ch.7 §§7.3.1, 7.3.7–9 pp.12–13, 21–25.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 21. Conformance choice

- **what**: B — Serialize using the selected version's grammar, delimiters and escaping, and retain raw bytes independently of normalized rendering. 231 ch.2 §2.10 pp.104–105; 251 ch.2 §2.6 pp.12–17; project raw-retention rule.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 22. Conformance choice

- **what**: B — Apply every supported field's declared datatype, length, required/conditional usage, repetitions and table constraints at the owning boundary. 231 ch.2 §§2.6, 2.8 pp.41–101; 251 ch.2 §§2.5.3, 2.12 pp.7–11, 40–58; ch.2A datatype definitions.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 23. Conformance choice

- **what**: B — Preserve complete composite IDs and assigning authorities, including EI/HD/CX as used by the edition. 231 ch.2 §§2.8.17, 2.8.20 pp.67–71; 251 ch.2A §§2.A.14, .25–26, .33 PDF pp.19–28, 34–36, 40–43.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 24. Conformance choice

- **what**: B — Retain coded-entry identifiers, display text, coding systems and alternate codes without equating display labels with identity. 231 ch.2 CE definition §2.8.3 pp.50–51; 251 ch.2A §§2.A.6, .8, .13 PDF pp.8–12, 17–19.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 25. Conformance choice

- **what**: 251 — Distinguish CE, CWE and CNE constraints; unknown local codes are not automatically legal under closed coding. Ch.2A §§2.A.6, .8, .13 PDF pp.8–12, 17–19.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 26. Conformance choice

- **what**: B — Parse NM and SN according to their own syntax, keep comparators/ranges/ratios, and never silently apply locale conversion. 231 ch.2 §2.8.27 p.72 and ch.7 §7.3.2.2 pp.507–509; 251 ch.2A §§2.A.47, .70 PDF pp.53–54, 85–86.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 27. Conformance choice

- **what**: B — Preserve date/time precision, supplied offset and lexical form without inventing missing precision. 231 ch.2 §§2.8.15/.41/.44 pp.65, 87–89; 251 ch.2A §§2.A.21–22, .75, .77 PDF pp.31–33, 89–90, 95.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 28. Conformance choice

- **what**: B — Parse OBX-5 through OBX-2, including repetitions and composite values. 231 ch.7 §§7.3.2.2–5 pp.507–512; 251 ch.7 §§7.4.2.2–5 pp.43–49.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 29. Conformance choice

- **what**: B — Keep ST/TX/FT semantics and repeats distinct; text repetition is not always multiple independent numeric observations. Same OBX clauses; 251 ch.2A §§2.A.31, .74, .78 PDF pp.39–40, 89, 95–96.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 30. Conformance choice

- **what**: B — Handle ED/RP with type/subtype/encoding metadata or park as unsupported; bound decoded binary expansion. 231 ch.2 §2.8.16 pp.65–67; 251 ch.2A §§2.A.24, .65 PDF pp.33–34, 77–80; bound is project policy.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 31. Conformance choice

- **what**: 251 — Handle MSG/VID/PT and error-location datatypes according to 251 rather than a 231 composite layout. Ch.2A §§2.A.44, .81, .57, .27–28 PDF pp.50–51, 97–98, 69–70, 36–38.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 32. Conformance choice

- **what**: B — Support each additional datatype referenced by the declared message/profile, or reject that capability explicitly; a raw parser cannot claim datatype conformance. 231 ch.2 §2.8; 251 ch.2A §§2.A.1–89 PDF pp.3–117 (per-type component/usage rules are the schema source).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 33. Conformance choice

- **what**: B — Apply edition-specific MSH-18 default and code-table rules; do not guess from local OS settings. 231 ch.2 §2.24.1.18 pp.144–145; 251 ch.2 §2.15.9.18 pp.79–81. **Partial evidence:** [control.rs](../../../crates/hl7/tests/control.rs): `hl7_231_2_24_1_18_and_251_2_15_9_18_charset_is_edition_specific` tests the supported ASCII, Latin-1 and UTF-8 subset by edition, including default rejection and explicit override for legacy UTF-8. [config.rs](../../../crates/config/tests/config.rs): `hl7_231_2_24_1_18_utf8_requires_named_vendor_departure` requires a named reason and limits the departure to 2.3.1 with UTF-8 configured. Other table encodings remain unsupported.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `hl7_231_2_24_1_18_and_251_2_15_9_18_charset_is_edition_specific`, `hl7_231_2_24_1_18_utf8_requires_named_vendor_departure`.

## 34. Conformance choice

- **what**: B — Implement declared alternate-set switching and MSH-20 handling or explicitly reject the unsupported scheme. 231 ch.2 §§2.9.2, 2.24.1.20 pp.102, 145; 251 ch.2 §§2.7.2, 2.15.9.20 pp.17–18, 82.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 35. Conformance choice

- **what**: 251 — Enforce the charset table's UTF-8 restrictions; MLLP's UTF-8 capability does not independently authorize every HL7 header usage. Ch.2 §2.15.9.18 pp.79–81.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 36. Conformance choice

- **what**: M — Agree a connection encoding compatible with R1; exclude UTF-16/32 and embedded conflicting control bytes. MLLP §1.3 pp.2–3.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 37. Conformance choice

- **what**: B — On undecodable or contradictory charset input retain bytes and report failure without replacement-character corruption. Above clauses plus project retention policy (engineering gate).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 38. Conformance choice

- **what**: B — Validate MSH-9 component presence by edition: all three in 2.5.1, optional trigger on 2.3.1 responses/acknowledgments. 251 section 2.A.44 pp.2-180–181; 231 section 2.24.1.9 p.2-95. **Evidence:** [control.rs](../../../crates/hl7/tests/control.rs): `hl7_251_2_a_44_and_231_2_24_1_9_message_type_components` checks required components, component lengths, optional legacy response triggers and unchanged source bytes.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_251_2_a_44_and_231_2_24_1_9_message_type_components`.

## 39. Conformance choice

- **what**: B — Require a valid MSH-7 in 2.5.1; permit its omission in 2.3.1 and validate supplied timestamps. 251 ch.2 sections 2.15.9, 2.15.9.7 pp.74, 76; 231 ch.2 section 2.24.1.7. **Evidence:** [control.rs](../../../crates/hl7/tests/control.rs): `hl7_251_2_15_9_7_requires_valid_message_time_but_231_allows_absence` checks missing/null, invalid dates, repetition/composite misuse, valid reduced precision and unchanged source bytes. Delimiter escapes are decoded after selecting fields/repetitions/components, never before structural extraction. `original` retains its wire spelling, and decoded plain text is separate. `hl7_251_2_7_4_decodes_selected_identifiers_and_values_keeps_wire_originals` and `hl7_251_2_7_4_decoding_uses_message_delimiters_and_is_not_recursive` (mapping tests) cover identifiers, coded/text values, repeats, alternate delimiters, non-recursive decoding and byte-for-byte source retention. Source: 2.5.1 section 2.7.4, p.2-18. No hexadecimal/local/repertoire agreement is inferred. Formatting/highlighting in result values is retained and parked, with no clean decoded text. `hl7_251_2_7_3_2_7_6_formatting_is_unsupported_evidence_not_clean_text` (mapping tests) checks ST/TX/FT, original spelling, absent decoded text and the specific finding. `hl7_251_2_7_6_formatting_command_grammar` (control tests) checks command names and argument grammar. Source: 2.5.1 sections 2.7.3, 2.7.6, pp.2-18 through 2-20. Hex/local/repertoire escapes require an agreement; none is inferred (`hl7_251_2_7_unsupported_escapes_never_become_clean_text`).
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `hl7_251_2_15_9_7_requires_valid_message_time_but_231_allows_absence`, `hl7_251_2_7_4_decodes_selected_identifiers_and_values_keeps_wire_originals`, `hl7_251_2_7_4_decoding_uses_message_delimiters_and_is_not_recursive`, `hl7_251_2_7_3_2_7_6_formatting_is_unsupported_evidence_not_clean_text`, `hl7_251_2_7_6_formatting_command_grammar`, `hl7_251_2_7_unsupported_escapes_never_become_clean_text`.

## 40. Conformance choice

- **what**: B — Both MSH-15/16 unvalued selects original mode; otherwise validate and apply the enhanced condition fields. 231 ch.2 §§2.12.1, 2.24.1.15–16 pp.107–112, 143–144; 251 ch.2 §§2.9, 2.15.9.15–16 pp.24–28, 78.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 41. Conformance choice

- **what**: B — Apply AL/NE/ER/SU independently to acceptance and application outcomes; test ACK suppression as well as positive/negative responses. Same clauses.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 42. Conformance choice

- **what**: B — Original mode reports AA only for successful agreed application processing, AE for content/application error and AR for rejection. 231 §2.12.1.2.1 pp.109–110; 251 §2.9.2 pp.26–27.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 43. Conformance choice

- **what**: B — Enhanced mode safely commits before positive acceptance and distinguishes CA from CE/CR; no CA may precede durable responsibility. 231 §2.12.1.2.2 pp.110–112; 251 §2.9.3 pp.27–28.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 44. Conformance choice

- **what**: B — Treat application response as its own exchange with correct correlation and no infinite application-ACK chain. Same enhanced clauses.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 45. Conformance choice

- **what**: B — Create response MSH-10/time anew, address recipient from sender identity and copy MSA-2 from the triggering message ID. 231 §2.12.1.2 pp.110–111; 251 §§2.9.2.2, 2.9.3.2 pp.26, 28.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 46. Conformance choice

- **what**: B — Validate MSA codes and keep application, acceptance, query and order status separate. 231 §§2.24.2.1, 2.24.22 pp.146, 168–169; 251 §§2.15.8, ch.5 §5.5.2 pp.73–74, 44–45.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 47. Conformance choice

- **what**: 231 — Implement structured MSA-6 and ERR-1 code/location with segment occurrence and field position. Ch.2 §§2.24.2.6–2.24.3.1 pp.147–148.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 48. Conformance choice

- **what**: 251 — Implement ERR location, HL7 code, severity, application code/parameters and diagnostic/user details where applicable; handle legacy fields under compatibility policy. Ch.2 §2.15.5 pp.66–70, §2.15.8 pp.73–74.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 49. Conformance choice

- **what**: B — A functional order/query reply must not be replaced by a generic positive ACK. 231 ch.4 §4.2 pp.288–289, ch.2 §2.18 pp.120–121; 251 ch.4 §4.4 pp.6–25, ch.5 §5.4 pp.36–42.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 50. Conformance choice

- **what**: B — Declare legacy deferred-processing/MCF support separately from enhanced acceptance and deferred queries. 231 ch.2 §§2.12.2–2.13 pp.112–114; 251 ch.2 §2.14 p.61 and ch.5 §5.6 pp.52–60.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 51. Conformance choice

- **what**: B — Test disk failure, response loss, late/mismatched ACK, duplicate message ID with changed content and restart after commit/before reply. Processing rules above plus project safety policy; timeout does not prove failure (engineering gate). Sender ACK phases follow 2.5.1 sections 2.9.2-2.9.3 pp.2-26-28. `hl7_251_2_9_2_2_9_3_sender_preserves_ack_phases` (dialog tests) checks early application replies, nonterminal CA and duplicate CA in the application phase. `hl7_251_2_15_8_all_codes_and_mismatched_ack` now treats CA/CE/CR in original mode as specific wrong-phase errors rather than completion. `hl7_251_2_9_3_sender_acceptance_and_application_condition_matrix` checks the 16 AL/NE/ER/SU combinations and condition violations. `hl7_251_2_9_3_sender_application_wait_is_bounded_and_never_resends` checks the separate application deadline, timeout/disconnect uncertainty and no resend. Both phases disabled reports issuance with no requested response, never acceptance. ERR locations use the actual source segment occurrence and field when known, and remain unvalued when reconciliation cannot identify a source field. HL7 and LAW use one host builder. Source: 2.5.1 section 2.15.5.2 pp.2-67-68. `hl7_251_2_15_5_2_adr0003_ae_reports_actual_mapping_location` (receive tests) covers the second OBX and an unlocated conflict; `hl7_251_2_15_5_2_unknown_location_stays_unvalued` (control tests) checks wire rendering. LAW's `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs` (delivery tests) checks the same second-OBX mapping failure.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_251_2_9_2_2_9_3_sender_preserves_ack_phases`, `hl7_251_2_15_8_all_codes_and_mismatched_ack`, `hl7_251_2_9_3_sender_acceptance_and_application_condition_matrix`, `hl7_251_2_9_3_sender_application_wait_is_bounded_and_never_resends`, `hl7_251_2_15_5_2_adr0003_ae_reports_actual_mapping_location`, `hl7_251_2_15_5_2_unknown_location_stays_unvalued`, `law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`.

## 52. Conformance choice

- **what**: B — Implement ORU^R01 and ACK with edition-specific grouping, requiredness and optional context/notes. 231 ch.7 §7.2.1 pp.487–488; 251 ch.7 §7.3.1 pp.12–13.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 53. Conformance choice

- **what**: B — Preserve order identifiers, specimen identifiers and observation IDs as distinct scoped data. 231 ch.7 §§7.3.1.2–3, 7.3.2.3–4 pp.490–491, 509–511; 251 ch.7 §§7.4.1–2 pp.26–49.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 54. Conformance choice

- **what**: B — Identify observations with code system/sub-ID/order context, not OBX-1, test name, value equality or receipt ID alone. Same observation clauses.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 55. Conformance choice

- **what**: B — Preserve multiple same-key OBXs as a replacement unit and handle repeated OBX-5 values without discarding members. 231 §§7.3.2.5/.11 pp.511–512, 527–528; 251 §§7.4.2.5/.11 pp.46–49, 54–55.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 56. Conformance choice

- **what**: B — Preserve and handle all OBX-11 states: pending/unverified/preliminary/partial/final/corrected/deleted/wrong/unobtainable/status-only and order-detail states. 231 §7.3.2.11 pp.527–528; 251 §7.4.2.11 pp.54–55.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 57. Conformance choice

- **what**: B — Keep OBR-25 report status separate from OBX-11 and ORC control/status, including their precedence rules. 231 §7.3.1.25 p.502; 251 §7.4.1.25 p.35; order-control definitions in ch.4.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 58. Conformance choice

- **what**: B — Carry units, ranges, abnormal flags, probability and abnormal-test context without substituting vendor semantics globally. 231 §§7.3.2.6–10 pp.512–527; 251 §§7.4.2.6–10 pp.48–54.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 59. Conformance choice

- **what**: B — Preserve observation/report times, producer, observer and method separately when supplied. 231 §§7.3.1.22, 7.3.2.14–17 pp.500, 528–529; 251 §§7.4.1.22, 7.4.2.14–19 pp.34, 55–57.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 60. Conformance choice

- **what**: B — Preserve parent/child relationships rather than flattening microbiology or reflex relationships into equal sample/test rows. 231 §§7.3.1.26/.29 pp.502–503; 251 §§7.4.1.26/.29 pp.35–37 and ch.2A §2.A.56 PDF pp.68–69.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 61. Conformance choice

- **what**: 251 — Implement OUL^R21, specimen-oriented R22, specimen-container R23 and order-oriented R24 distinctly when advertised, including SPM/SAC/TCD/SID participation. Ch.7 §§7.3.2, 7.3.7–9 pp.14–16, 21–25.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 62. Conformance choice

- **what**: 251 — Implement the selected SPM fields, specimen role, parent IDs, collection/receipt/rejection and availability semantics; do not infer specimen identity from an arbitrary order field. Ch.7 §7.4.3 pp.59–71.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 63. Conformance choice

- **what**: 251 — Preserve OBX-18 equipment hierarchy, OBX-19 analysis time and relevant performing-organization fields, where the profile uses them. Ch.7 §§7.4.2.18–25 pp.57–59.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 64. Conformance choice

- **what**: B — Do not treat OBX-21 as a standard persistent ID in either target edition. 231 OBX definition ch.7 §7.3.2 pp.506–529; 251 §7.4.2.21 p.57 reserves it.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 65. Conformance choice

- **what**: 251 — Declare whether point-of-care R30/R31/R32 transactions are supported, with their different order-correlation responsibilities; do not equate them to R01. Ch.7 §§7.3.4–6 pp.17–20.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 66. Conformance choice

- **what**: B — Declare result-query QRY/ORF R02/R04 support and its version-specific response structure. 231 ch.7 §7.2.2 p.488; 251 §7.3.3 p.16.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 67. Conformance choice

- **what**: B — Test U with no repeated value, D/W withdrawal, missing predecessor, out-of-order correction, same-value rerun, multiple orders and mixed result/status records. Status/identity clauses above plus safety policy (engineering gate).
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 68. Conformance choice

- **what**: B — Never emit PID with only a patient ID; omit the optional patient group, or reject a machine requiring patient identification before reservation because contract v1 has no name. 231 section 3.3.2 p.3-36; optional-group structures: 231 section 4.2.1 and 251 section 4.4.6. **Evidence:** [order.rs](../../../crates/hl7/tests/order.rs): `hl7_231_ch4_explicit_group_position_and_identifier_do_not_invent_context` asserts no PID for identifier-only context. [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs): `hl7_231_3_3_2_required_pid_rejects_before_reservation_without_inventing_name` checks the specific rejection reason, no Sending history and no wire bytes in both editions. Configure `orders.hl7_send.require_patient_identification = true` only when the machine requires PID; such work is refused while contract v1 lacks names.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_231_ch4_explicit_group_position_and_identifier_do_not_invent_context`, `hl7_231_3_3_2_required_pid_rejects_before_reservation_without_inventing_name`.

## 69. Conformance choice

- **what**: B — Encode ORC-4 as an EI with the exact ORC-2 authority components; reject complete EIs over 22 bytes before reservation. 231 section 4.3.1.4 p.4-17; 251 section 4.5.1.4 p.4-39; each edition's ORC attribute table. **Evidence:** [order.rs](../../../crates/hl7/tests/order.rs): `hl7_231_4_3_1_4_251_4_5_1_4_group_ei_authority_and_complete_field_limit` checks local/universal authorities and exact/overflow limits in both editions. [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs): `hl7_231_4_3_1_4_251_4_5_1_4_full_group_ei_overflow_precedes_reservation` checks a scalar that fits but whose complete EI overflows, specific refusal, no Sending history and no wire bytes. Audit scope: ORM/O01–ORR/O02 in both editions; non-LAW O21/O22 in 2.5.1; O33/O34 only under the LAW profile. Generic work supports new/cancel requests, one specimen/test per order and a non-repeating timing window. A checked conditional row covers only its declared subset, including explicit rejection of excluded capabilities. Partial rows stay unchecked with the remaining work named. All evidence below is documented-level synthetic testing, not machine qualification or proof of host durability/recovery.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `hl7_231_4_3_1_4_251_4_5_1_4_group_ei_authority_and_complete_field_limit`, `hl7_231_4_3_1_4_251_4_5_1_4_full_group_ei_overflow_precedes_reservation`.

## 70. Conformance choice

- **what**: B — Implement declared ORM^O01/ORR^O02 grammars and repeat groups, not merely names. 231 ch.4 §§4.2.1–2 pp.288–289; 251 ch.4 §§4.4.1–2 pp.6–9. **Evidence:** [order.rs](../../../crates/hl7/tests/order.rs): `hl7_231_4_2_1_and_251_4_4_1_build_new_and_cancel_groups`, `hl7_231_4_3_1_b_and_251_4_5_1_b_obr_identity_fallback_and_response_order`, `hl7_231_4_2_2_and_251_4_4_2_multiple_details_are_not_flattened`. Declared scope: laboratory OBR detail, one patient context and one test/battery per order; other detail families are rejected.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: `hl7_231_4_2_1_and_251_4_4_1_build_new_and_cancel_groups`, `hl7_231_4_3_1_b_and_251_4_5_1_b_obr_identity_fallback_and_response_order`, `hl7_231_4_2_2_and_251_4_4_2_multiple_details_are_not_flattened`.

## 71. Conformance choice

- **what**: B — Model each supported ORC request, response and status explicitly; distinguish requested cancel/change from acknowledgment that it happened. 231 ch.4 §4.3.1 pp.290–307; 251 ch.4 §4.5.1 pp.27–50. **Evidence:** [order.rs](../../../crates/hl7/tests/order.rs): `hl7_231_4_3_1_1_and_251_4_5_1_1_exhaustive_request_reply_pairs`, `hl7_231_4_3_1_5_and_251_4_5_1_5_snapshot_does_not_change_request_outcome`, `hl7_231_4_3_1_5_and_251_4_5_1_5_invalid_status`. Supported controls are NW/CA and OK/UA/CR/UC; status is retained as a peer snapshot. Change/discontinue controls are explicitly unsupported, not aliases for acceptance.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_231_4_3_1_1_and_251_4_5_1_1_exhaustive_request_reply_pairs`, `hl7_231_4_3_1_5_and_251_4_5_1_5_snapshot_does_not_change_request_outcome`, `hl7_231_4_3_1_5_and_251_4_5_1_5_invalid_status`.

## 72. Conformance choice

- **what**: B — Preserve placer/filler namespaces, parent/group relationships and identity on changes/retries. Same ORC clauses; OBR definitions in each ch.7. **Remaining:** parent/placer-group and change relationships in the generic ORM/O21 API, while host recovery is covered in [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs), `adr0003_hl7_restart_during_sending_recovers_items_without_resend`. Existing namespace and replay evidence: [order.rs](../../../crates/hl7/tests/order.rs), `hl7_231_4_3_1_2_3_and_251_4_5_1_2_3_universal_namespaces_preserved` and `hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities`. LAW-specific parents/groups have separate [law_order.rs](../../../crates/hl7/tests/law_order.rs) coverage; that does not establish generic support in both editions.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: `adr0003_hl7_restart_during_sending_recovers_items_without_resend`, `hl7_231_4_3_1_2_3_and_251_4_5_1_2_3_universal_namespaces_preserved`, `hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities`.

## 73. Conformance choice

- **what**: B — Validate requested timing/priority/specimen and required order detail; retain unsupported order content with a specific rejection. 231 ch.4 §4.3 pp.290–338; 251 ch.4 §§4.5.1–4.5.5 pp.27–80. **Remaining:** complete optional order-detail/component predicates ; host rejection and operator behavior exist in [program orders](../../../crates/analyte/ORDERS.md); cross-field timing interpretation stays with the host. The current coded specimen, explicit priority/non-repeating window and rejection subset is covered by [order.rs](../../../crates/hl7/tests/order.rs), `hl7_231_2_8_and_251_2a_caller_data_rejects_injection_and_oversize`, and [laboratory_order.rs](../../../crates/hl7/tests/laboratory_order.rs), `hl7_251_4_4_6_builder_is_bounded_and_replay_is_pure`.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `hl7_231_2_8_and_251_2a_caller_data_rejects_injection_and_oversize`, `hl7_251_4_4_6_builder_is_bounded_and_replay_is_pure`.

## 74. Conformance choice

- **what**: 251 — Implement OML^O21/ORL^O22, multiple-orders-per-specimen O33/O34, and single-container O35/O36 separately where declared. Ch.4 §§4.4.6–11 pp.14–25. **Partial evidence:** [laboratory_order.rs](../../../crates/hl7/tests/laboratory_order.rs): `hl7_251_4_4_6_builds_order_owned_timing_specimen_and_cancel`, `hl7_251_4_4_7_correlates_typed_decisions_and_specimen_container_groups`, `hl7_251_4_4_6_7_rejects_wrong_version_and_transaction_family`. [law.rs](../../../crates/hl7/tests/law.rs): `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`. Declared families: non-LAW O21/O22 and LAW O33/O34. This row stays open: the cited tests do not exercise O35/O36. O35/O36 and unprofiled O33/O34 are not selected; they are not routed through another family.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_251_4_4_6_builds_order_owned_timing_specimen_and_cancel`, `hl7_251_4_4_7_correlates_typed_decisions_and_specimen_container_groups`, `hl7_251_4_4_6_7_rejects_wrong_version_and_transaction_family`, `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`.

## 75. Conformance choice

- **what**: 251 — Handle order groups with SPM/SAC, associated OBX and specimen/container-specific instructions without moving fields between groups. Same OML/ORL definitions; ch.7 §7.4.3; ch.13 §13.4.3. **Remaining:** associated OBX groups and specimen/container-specific instructions in the generic O21/O22 API; it currently supports one specimen per new order and preserves returned SAC groups. Evidence for that subset: [laboratory_order.rs](../../../crates/hl7/tests/laboratory_order.rs), `hl7_251_4_4_6_7_specimens_stay_with_their_own_orders`, `hl7_251_4_4_7_orl_sac_requires_spm`. LAW enhanced-content evidence is separate: [law.rs](../../../crates/hl7/tests/law.rs), `law_tf2b_2_2_4_app_supplied_patient_specimen_container_and_order_content`.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_251_4_4_6_7_specimens_stay_with_their_own_orders`, `hl7_251_4_4_7_orl_sac_requires_spm`, `law_tf2b_2_2_4_app_supplied_patient_specimen_container_and_order_content`.

## 76. Conformance choice

- **what**: 251 — Handle TQ1/TQ2 timing/relationships where declared rather than assuming only the legacy TQ field. Ch.4 §§4.5.4–5 pp.71–80. **Partial evidence:** [laboratory_order.rs](../../../crates/hl7/tests/laboratory_order.rs): `hl7_251_4_4_6_builds_order_owned_timing_specimen_and_cancel`, `hl7_251_4_5_4_orl_repeated_window_scope`, `hl7_251_4_5_5_orl_unsupported_timing_relationship`. [law_order.rs](../../../crates/hl7/tests/law_order.rs): `law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option`. This row stays open because TQ2 relationships are not implemented. Declared scope is one TQ1 window/priority in O21 and profiled LAW priority; repeated windows and TQ2 are excluded and rejected, not silently read as legacy TQ.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_251_4_4_6_builds_order_owned_timing_specimen_and_cancel`, `hl7_251_4_5_4_orl_repeated_window_scope`, `hl7_251_4_5_5_orl_unsupported_timing_relationship`, `law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option`.

## 77. Conformance choice

- **what**: B — Declare order-status-query and unsupported order-family behavior; all-HL7 claims otherwise include more than laboratory ordering. 231 ch.4 order chapter; 251 ch.4 §4.4.3 p.9 and §§4.4.4–5 pp.10–14. **Evidence:** [order.rs](../../../crates/hl7/tests/order.rs): `hl7_231_4_2_3_and_251_4_4_3_unsupported_order_families_are_located_errors`. Status-query replies, dietary/pharmacy/supply and other order families have no builder in this API; a typed order parser rejects an unexpected event/structure at MSH-9 and unsupported controls at ORC-1. This declaration does not claim a host-wide dispatch/rejection dialog.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_231_4_2_3_and_251_4_4_3_unsupported_order_families_are_located_errors`.

## 78. Conformance choice

- **what**: B — Test accept/reject/cancel/unable-to-cancel/change and uncertain delivery with stable identities, never issuing new machine orders as a side effect of replay. Same order rules plus project order-lifecycle policy. **Remaining:** change workflows and broader cancellation-race qualification. Program send/recovery tests exist in [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs): `hl7_251_no_reply_disconnect_and_restart_never_resend` and `hl7_231_251_cancel_accepted_or_refused_restores_prior_state_once`. Pure acceptance/cancellation and stable replay evidence: [order.rs](../../../crates/hl7/tests/order.rs), `hl7_231_4_3_1_and_251_4_5_1_reports_each_matching_outcome`, `hl7_231_4_2_2_and_251_4_4_2_message_ack_is_not_order_acceptance`, `hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities`. These tests neither send orders nor prove durable host recovery.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_251_no_reply_disconnect_and_restart_never_resend`, `hl7_231_251_cancel_accepted_or_refused_restores_prior_state_once`, `hl7_231_4_3_1_and_251_4_5_1_reports_each_matching_outcome`, `hl7_231_4_2_2_and_251_4_4_2_message_ack_is_not_order_acceptance`, `hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities`.

## 79. Conformance choice

- **what**: 231 — Implement declared immediate QRY/DSR Q01 and deferred QRY/QCK Q02 followed by DSR/ACK Q03. Ch.2 §§2.17–2.18 pp.120–121. **Remaining:** immediate Q01. Deferred Q02/Q03 runtime evidence is in [hl7.rs](../../../crates/analyte/tests/orders/hl7.rs), `hl7_231_251_query_found_reserves_before_dsr_ack_aa_sends_once`. The declared pure Q02/Q03 subset has [query.rs](../../../crates/hl7/tests/query.rs) evidence: `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`. Q01 is not silently treated as Q02.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_231_251_query_found_reserves_before_dsr_ack_aa_sends_once`, `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`.

## 80. Conformance choice

- **what**: 251 — Treat those older query exchanges as backward-compatible legacy capabilities, not the modern query-by-parameter model. Ch.5 §5.10 pp.113–137. **Evidence:** [query.rs](../../../crates/hl7/tests/query.rs): `hl7_231_2_18_qck_answers_deferred_query` (both editions), `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`. Legacy QRD/QRF transactions remain separate from LAW QBP/QPD queries.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_231_2_18_qck_answers_deferred_query`, `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`.

## 81. Conformance choice

- **what**: B — Validate QRD format/priority/query ID/quantity/filter and QRF fields according to the edition; DSP positions have no generic laboratory meaning. 231 ch.2 §§2.24.4–5/.9 pp.148–153, 156–157; 251 ch.5 §5.10.5 pp.123–137. **Evidence:** [query_validation.rs](../../../crates/hl7/tests/query_validation.rs): `hl7_231_2_24_4_5_and_251_5_10_5_3_4_field_usage_lengths_and_repetitions`, `hl7_231_2_24_4_5_and_251_5_10_5_3_4_preserve_local_filters`, plus its per-field named negative fixtures and boundary/property cases. [query.rs](../../../crates/hl7/tests/query.rs): `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr` demonstrates caller-supplied display text without a laboratory row interpretation. Valid field syntax does not establish that the host can honor every filter/unit.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_231_2_24_4_5_and_251_5_10_5_3_4_field_usage_lengths_and_repetitions`, `hl7_231_2_24_4_5_and_251_5_10_5_3_4_preserve_local_filters`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`.

## 82. Conformance choice

- **what**: B — Keep query tag/QRD ID distinct from per-message MSH-10/MSA-2 correlation, especially across continuation. 231 ch.2 §§2.24.4.4, 2.24.22 p.149, 168–169; 251 ch.5 §§5.5.2–4 pp.44–47. **Partial evidence:** [query.rs](../../../crates/hl7/tests/query.rs): `hl7_231_2_24_4_query_id_is_preserved`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`, `hl7_231_2_18_dsr_requires_data_and_distinct_response_ids`. This row stays open: these tests check only the declared non-continuation exchange; continuation identity/lifetime support remains open below.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: `hl7_231_2_24_4_query_id_is_preserved`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`, `hl7_231_2_18_dsr_requires_data_and_distinct_response_ids`.

## 83. Conformance choice

- **what**: B — Preserve no-data versus application error/rejection; never answer NF on an unknown lookup. 231 ch.2 §2.24.22.2 p.169; 251 ch.5 §§5.5.2, 5.6.5 pp.44–45, 60–61; project rule 9. **Evidence:** [query.rs](../../../crates/hl7/tests/query.rs): `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf` (both editions). [law.rs](../../../crates/hl7/tests/law.rs): `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`. Unavailable lookup is an error, not a trusted empty result; malformed/unsupported requests cannot be constructed into a positive typed response.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`.

## 84. Conformance choice

- **what**: 251 — Define query parameters and exact response using a query conformance statement; QBP alone does not specify the lab's lookup contract. Ch.5 §§5.3, 5.4.1 pp.14–37. **Remaining:** any non-LAW QBP contract requires its own selected query conformance statement before implementation. The only current query-by-parameter contract is IHE LAB-27, documented in [ihe-law.md](../law/README.md) and tested by [law.rs](../../../crates/hl7/tests/law.rs), `law_tf2b_3_27_4_query_forms_and_conditional_fields`; this is not a general lab-app lookup contract.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_query_forms_and_conditional_fields`.

## 85. Conformance choice

- **what**: 251 — Implement selected QBP/RSP, QBP/RTB and QBP/RDY pairs with their own structures and event/structure IDs. Ch.5 §§5.4.1–3 pp.37–40. **Partial evidence:** [law.rs](../../../crates/hl7/tests/law.rs): `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`. This row stays open: only LAW QBP/Q11–RSP/K11 is selected; RTB and RDY transactions are not declared or implemented. [query.rs](../../../crates/hl7/tests/query.rs), `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`, prevents legacy fallback.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`, `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`.

## 86. Conformance choice

- **what**: 251 — Validate QPD query name/tag/parameters, RCP priority/quantity/mode, QAK status/counts and echoed correlation. Ch.5 §§5.5.2–6 pp.44–50. **Remaining:** generic QPD/RCP/QAK profiles, quantitative response/count handling and contracts beyond the LAW subset. Existing selected LAW scope/tag/mode and response-echo evidence: [law.rs](../../../crates/hl7/tests/law.rs), `law_tf2b_3_27_4_forbidden_missing_null_and_repeated_parameters_retain_bytes`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`. Those cases do not establish arbitrary query/profile conformance.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `law_tf2b_3_27_4_forbidden_missing_null_and_repeated_parameters_retain_bytes`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`.

## 87. Conformance choice

- **what**: 251 — Handle RDF/RDT row schemas and QRI instance/match information if those response forms are supported. Ch.5 §§5.5.5, .7–8 pp.47–52. **Remaining:** RDF/RDT/QRI schemas and validation if a tabular or match-list response is selected. These response forms are currently unselected and unimplemented; no typed API claims to consume them.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 88. Conformance choice

- **what**: B — Implement declared DSC continuation, cancellation and logical response boundaries without confusing them with TCP reads. 231 ch.2 §§2.15.4–5, 2.24.8 pp.118–119, 156; 251 ch.5 §§5.6.2–3 pp.55–59; ch.2 §2.15.4 pp.65–66. **Remaining:** DSC pointer lifetime, continuation/cancellation state and logical batch completion in the host dialog. The current query view rejects continuation and retains the original message: [query.rs](../../../crates/hl7/tests/query.rs), `hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located`.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: `hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located`.

## 89. Conformance choice

- **what**: 251 — Distinguish fragmentation from interactive continuation and preserve pointer lifetime/correlation. Ch.2 §§2.10.2, 2.15.4 pp.30–34, 65–66; ch.5 §5.6.3 pp.55–59. **Remaining:** message fragmentation/reassembly and interactive continuation with persisted correlation and pointer expiry. Receiving a complete bounded Message or rejecting DSC is not evidence of this lifecycle.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 90. Conformance choice

- **what**: 251 — Declare subscription/replay and query/subscription cancellation support; implement QSB/QVR/QCN/QSX only with their lifecycle. Ch.5 §§5.4.4–7, 5.7 pp.40–42, 61–66. **Partial evidence:** [query.rs](../../../crates/hl7/tests/query.rs): `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events` tests QSB/Q16, QVR/Q17, QCN/J01 and QSX/J02. This row stays open: the tests exercise rejection, not subscription/replay/cancellation lifecycles. These capabilities and subscription/replay lifecycles are explicitly unselected; no typed query handler treats them as legacy lookup or starts work.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`.

## 91. Conformance choice

- **what**: 231 — Declare EQQ/VQQ/SPQ/ERQ and enhanced query response support separately; these are not QBP/RSP. Ch.2 §§2.19–2.22 pp.121–123. **Evidence:** [query.rs](../../../crates/hl7/tests/query.rs): `hl7_231_2_19_2_20_undeclared_enhanced_query_forms_are_rejected`, `hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located`. EQQ, VQQ, SPQ and event replay are explicitly unsupported. ERQ is an event-replay segment, not another QBP transaction; ERP/R09 and ERQ content are not accepted by the legacy handler.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: `hl7_231_2_19_2_20_undeclared_enhanced_query_forms_are_rejected`, `hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located`.

## 92. Conformance choice

- **what**: B — Test successful empty/multiple/partial responses, unavailable lookup, late response, cancellation and restart without unintended order retransmission. Query clauses above plus project deadlines/order rules. **Remaining:** logical multi-part/partial replies and broader cancellation qualification. Host tests in [hl7.rs](../../../crates/analyte/tests/orders/hl7.rs) cover `hl7_251_dsr_ack_ae_ar_lost_and_disconnect_are_specific_uncertain` and `hl7_251_adr0004_total_query_deadline_includes_split_block_receive_time`; [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs) covers restart without resend. Current pure wire evidence is [query.rs](../../../crates/hl7/tests/query.rs), `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_18_dsr_requires_data_and_distinct_response_ids`, `hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf`; it does not exercise a connection state machine or durable recovery.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: `hl7_251_dsr_ack_ae_ar_lost_and_disconnect_are_specific_uncertain`, `hl7_251_adr0004_total_query_deadline_includes_split_block_receive_time`, `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_18_dsr_requires_data_and_distinct_response_ids`, `hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf`.

## 93. Conformance choice

- **what**: B — If sequence protocol is negotiated, persist last accepted state and implement initialization, expected-number checks and resynchronization. 231 ch.2 §2.23.1 pp.123–125; 251 ch.2 §2.10.1 pp.29–30.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 94. Conformance choice

- **what**: B — If message/segment continuation is supported, reconstruct correct identity and boundaries rather than delivering partial content as complete. 231 ch.2 §2.23.2 pp.125–126; 251 ch.2 §2.10.2 pp.30–34.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 95. Conformance choice

- **what**: B — If file/batch support is advertised, implement FHS/BHS/BTS/FTS ordering, counts and acknowledgment semantics. 231 ch.2 §2.23.3 pp.126–128; 251 ch.2 §2.10.3 pp.34–36 and control segments §§2.15.2–3/.6–7 pp.63–65, 71–73.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 96. Conformance choice

- **what**: B — Apply snapshot/action-code repeating-segment update semantics where specified. 231 ch.2 §2.23.4 pp.128–129; 251 ch.2 §2.10.4 pp.36–37.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 97. Conformance choice

- **what**: B — Explicitly reject unsupported negotiated control modes rather than ignoring their fields and partially processing data. Processing/profile rules above; project fail-visible policy.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 98. Conformance choice

- **what**: 251 — Declare roles and support for equipment status ESU/ESR U01/U02 and specimen status SSU/SSR U03/U04. Ch.13 §§13.3.1–4 pp.15–17.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 99. Conformance choice

- **what**: 251 — Declare inventory INU/INR U05/U06, commands EAC U07 and completion EAR U08, and notifications EAN U09. Ch.13 §§13.3.5–9 pp.17–20.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 100. Conformance choice

- **what**: 251 — Declare test-code settings TCU/TCR U10/U11 and equipment service/log LSU/LSR U12/U13. Ch.13 §§13.3.10–13 pp.20–22.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 101. Conformance choice

- **what**: 251 — Preserve equipment identity/state, interaction state and notifications using EQU/ISD/NDS/CNS where supported. Ch.13 §§13.4.1–2/.7–8 pp.23–25, 46–48.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 102. Conformance choice

- **what**: 251 — Preserve SAC container, parent, carrier, position, volume, status and related information with correct specimen association. Ch.13 §13.4.3 pp.25–38.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 103. Conformance choice

- **what**: 251 — Keep inventory/material quantities and lot identifiers and command/response correlation distinct from result acceptance. Ch.13 §§13.4.4–6 pp.38–46.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 104. Conformance choice

- **what**: 251 — Preserve TCC/TCD dilution/configuration, SID material identification and EQP service data where supported. Ch.13 §§13.4.9–12 pp.48–55.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 105. Conformance choice

- **what**: 251 — For QC carry control specimen role/source, container/material identity, equipment identity and analysis time; this transports evidence, not QC decisions. Ch.13 §13.5.3 p.55; ch.7 §§7.4.2.18–19 p.57.
- **why**: Distinguish retransmission, correction and another run using source evidence rather than guessing.
- **source**: not found.
- **test**: none yet.

## 106. Conformance choice

- **what**: 251 — If claiming this automation profile scope, account for clock-synchronization receiver support rather than inventing a timer command. Ch.13 §13.5.1 p.55 (full transaction definition requires the applicable administrative message, beyond these lab chapters).
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 107. Conformance choice

- **what**: 251 — Do not invent a universal dedicated lab-order query from ch.13; distinguish unsolicited/implicit triggers from separately profiled QBP/RSP. Ch.13 §13.5.4 p.55.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 108. Conformance choice

- **what**: 251 — Resolve stale ch.13 cross-references against actual OML/OUL definitions before encoding messages. Ch.13 §§13.3 introductory text, 13.5.2 pp.15, 55 versus ch.4 §4.4.6 p.14 and ch.7 §7.3.2 p.14.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 109. Conformance choice

- **what**: M — Read exactly VT, compatible payload, FS, CR; preserve segment-ending CR independently of block trailer. MLLP §1.2 p.2; HL7 delimiter rules.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 110. Conformance choice

- **what**: M — Validate permitted content/control bytes and encoding; arbitrary binary content is not a raw pass-through inside R1. MLLP §§1.2–1.3 pp.2–3.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 111. Conformance choice

- **what**: B — Decode split headers/trailers and coalesced blocks incrementally; TCP reads are not message boundaries. MLLP §1.2 plus stream-handling obligation.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 112. Conformance choice

- **what**: B — Bound pending bytes, complete/partial frame size, decoded size and every wait; reject visibly on overlimit. Project safety rule 7; HL7 does not give one universal maximum (231 ch.2 §2.3 p.40; 251 ch.2 §2.4 pp.5–6).
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 113. Conformance choice

- **what**: B — Define recovery for partial EOF, invalid trailer, unexpected start/control byte and timeout; retain diagnostic raw evidence in the protected store. MLLP §1.2 plus project retention policy.
- **why**: Keep memory and waits bounded while retaining evidence for recovery.
- **source**: not found.
- **test**: none yet.

## 114. Conformance choice

- **what**: M — Do not add ASTM handshake/checksum or assume an R1 byte ACK/NAK/retry algorithm. MLLP §§1.1–1.2 pp.1–2.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 115. Conformance choice

- **what**: B — Agree connection roles, reuse/reconnect, concurrency, ACK timeout and retry policy per interface; R1 supplies no business idempotency. MLLP §1.1 pp.1–2; 231 ch.2 §2.12.1.2.1 p.110; 251 ch.2 §2.9.2.2 p.26.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 116. Conformance choice

- **what**: B — Test disconnect/lost reply after peer commit as uncertain outcome, retain identities across retries, and recover after restart without duplicate clinical effects. Same processing rules plus project identities/durability policy.
- **why**: Keep the promised evidence before transferring responsibility or advancing external state.
- **source**: not found.
- **test**: none yet.

## 117. Conformance choice

- **what**: B — Verify durability latency against configured peer deadlines and failure behavior on disk-full/store failure; transport success is not durable/application success. Enhanced-processing clauses plus project safety rules.
- **why**: Keep the supported boundary explicit; broader conformance still needs the evidence listed in the checklist.
- **source**: not found.
- **test**: none yet.

## 118. Host custody policy

- **what**: Retain original raw evidence before a positive acknowledgment; park uninterpretable input.
- **why**: A positive response must not promise custody of evidence the host can lose.
- **source**: [ADR 0010](../../adr/0010-acknowledgment-boundaries.md), Decision, HL7 row; [ADR 0003](../../adr/0003-clinical-safety-rules.md), Decision.
- **test**: `hl7_251_2_9_2_aa_only_after_store_and_interpret`.

## 119. Result history policy

- **what**: Assign revisions through the store using trusted identity and syntax-owned status progression; park conflicting changes.
- **why**: A preliminary-to-final transition differs from an unmarked correction after final.
- **source**: [ADR 0011](../../adr/0011-result-status-progression.md), Decision.
- **test**: `adr0011_caller_status_table_is_generic_and_correction_cannot_bypass_it`.
