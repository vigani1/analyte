# HL7 conformance

## Checklist

### Later-edition truncation groundwork (documented)

- [x] 2.8.2 / 2.9.1 — Preserve the optional fifth encoding character and require
  distinct separators; earlier editions still refuse the declaration.
  Sources: 2.8.2 §2.14.9.2 p.59; 2.9.1 §2.13.9.2 p.69.
  Evidence: `hl7_2_8_2_s2_14_9_2_and_2_9_1_s2_13_9_2_encoding_characters`.
- [x] Preserve marked primitive bytes, expose marker flags and refuse clinical
  mapping. Sources: 2.8.2 §2.5.5.2 pp.12–13; 2.9.1 §2.4.5.2 pp.14–15.
  Evidence: `hl7_2_8_2_s2_5_5_2_and_2_9_1_s2_4_5_2_preserve_and_refuse_marked_values`
  and `hl7_2_9_1_s2_4_5_2_truncated_input_never_reaches_result_mapping`.
- [x] Decode and encode literal declared truncation characters; refuse size
  overflow without shortening; retain undeclared-escape rejection/preservation.
  Sources: 2.8.2 §§2.7.1–2 pp.20–21; 2.9.1 §§2.6.1–2 pp.22–23.
  Evidence: `hl7_2_9_1_s2_6_1_truncation_character_escape` and
  `hl7_2_8_2_s2_7_2_undeclared_p_escape_remains_unsupported`.
- [x] Register all eight exact editions with source-named tests and explicit
  qualification state. `version/tests.rs` covers every registry entry
  plus `adr0012_s3_no_unlisted_edition_or_prefix_match` (ADR 0012 §3).
- [x] Accept machine edition selections only for implemented typed profiles
  (all eight declared receiving editions), without defaults or patch upgrades.
  `config/tests/hl7_editions.rs` covers exact identity and canonical snapshots
  for all eight, withdrawn-query and unqualified-writer refusals, missing
  declarations and positioned unlisted-value errors.
- [x] Implement the declared 2.3/2.4/2.5 receiving compatibility prefix under
  2.5.1 §§2.8–2.8.2 and the 2002-04-08 errata, with exact edition replies and
  explicit raw/refusal behavior. Evidence and limitations: [versions](versions.md),
  protocol `compatibility.rs`, mapping `mapping/compatibility.rs`, dialog
  `hl7_compatibility.rs`, datatype sequencing and config `hl7_editions.rs` tests.
- [x] 2.6 — Complete the bounded receiving/query profile: DTM, CWE, XCN changes,
  199-byte MSH-10/MSA-2, expanded ERR, deprecated-field raw/refusal rules and
  protected raw retention. Sources and per-row tests: [versions](versions.md),
  protocol `receiving.rs`, mapping `mapping/compatibility.rs`, dialog
  `hl7_compatibility.rs` and config `hl7_editions.rs`. Evidence: documented.
- [x] Refuse withdrawn original queries in config for 2.8.2 (§5.10.2 p.104)
  and 2.9.1 (§5.9.2 p.80), with a specific edition/section diagnostic. Keep
  2.6's retained backward-compatible query path (§5.10.2 pp.95–96).
  Evidence: the three source-named config tests listed in [versions](versions.md).
  The newer editions' selected scope is result reception only; LAW QBP is separate.
- [x] 2.8.2 — Complete the bounded result profile: 22-component CWE, normative
  versus conformance lengths, withdrawal checks, literal-marker handling and
  explicit rejection of unselected groups. Sources and per-row tests: [versions](versions.md),
  protocol `modern.rs`, mapping `mapping/modern.rs`, config `hl7_editions.rs`
  and store-before-ACK dialog tests. Evidence: documented.
- [x] 2.9.1 — Complete that bounded result profile with its additional OBR
  withdrawals, paired ACK requests and raw-only UTC-without-civil-offset times.
  Same source/test matrix; no new common-model field or dependency.
- [ ] Step 5 — Implement order writers per edition in a separate PR.


**Applicability:** B = both 2.3.1 and 2.5.1; 231 or 251 = edition-specific; M = version-independent MLLP R1. References are chapter/section/PDF page as above. Each checkbox is an implementation requirement or an explicit support-declaration decision. Optional features are required only when declared/negotiated; unsupported cases must have a specific safe outcome. Engineering gates are identified separately from normative wire rules. A profile matrix must instantiate the field definitions and conditional predicates referenced here; this is not a claim that one generic application must implement all healthcare domains.

### A. Scope, profiles and boundaries

- [ ] B — Declare supported version/event/structure/role combinations and extensions; do not claim all HL7 from an ORU demonstration. 231 ch.2 §§2.1, 2.11 pp.37, 106–107; 251 ch.2 §§2.12–2.13 pp.40–60.
- [ ] B — Select version and message semantics from validated MSH fields, not from familiar segment content. 231 ch.2 §2.12.1 pp.107–112; 251 ch.2 §2.9 pp.24–28.
- [ ] 251 — Publish static and dynamic profile constraints, usage, cardinalities, predicates, tables and identifier, and validate profile claims. Ch.2 §2.12 pp.40–58; MSH-21 §2.15.9.21 p.82.
- [ ] B — Separate structural parse success, base conformance, profile conformance and successful bridge processing. 231 ch.2 §§2.10–2.12 pp.104–112; 251 ch.2 §§2.6, 2.9, 2.12 pp.12–17, 24–28, 40–58.
- [ ] B — Preserve unknown/local content with an explicit compatibility policy; do not invent semantics for Z segments or extra fields. 231 ch.2 §§2.5, 2.10.2 pp.41, 105–106; 251 ch.2 §§2.8, 2.11 pp.20–24, 37–40.
- [ ] B — Generate standard fixtures separately from named vendor deviations and record their source/evidence level. Same version/profile rules; AGENTS evidence policy (engineering gate).

### B. Encoding, hierarchy and escaping

- [ ] B — Discover field separator from MSH-1 and the ordered component/repetition/escape/subcomponent characters from MSH-2. 231 ch.2 §§2.7, 2.24.1.1–2 pp.43–44, 130; 251 ch.2 §§2.5.4, 2.15.9.1–2 pp.11–12, 75.
- [ ] B — Handle MSH's special field indexing and CR segment terminators. Same delimiter clauses.
- [ ] B — Parse all field, repetition, component and subcomponent positions, preserving empty/trailing positions and cardinality. 231 ch.2 §§2.6–2.7, 2.10 pp.41–44, 104–105; 251 ch.2 §§2.5.3–2.6 pp.7–17.
- [ ] B — Distinguish explicit null, empty/unvalued, and absent data according to the message's update semantics. 231 ch.2 §§2.6, 2.10 pp.41–42, 104–105; 251 ch.2 §§2.5.3, 2.6 pp.7–17.
- [ ] B — Encode/decode literal delimiter escapes using the declared escape character without creating spurious structure. 231 ch.2 §2.9 pp.101–104; 251 ch.2 §2.7 pp.17–20.
- [ ] B — Support or specifically flag formatting/highlighting, hex, local and character-switch escapes; preserve unsupported content. Same escape clauses; do not silently flatten FT instructions.
- [ ] B — Detect malformed escapes safely and retain original spelling; structure is parsed before value de-escaping. Same escape/construction clauses plus project retention rule.
- [ ] B — Preserve nested patient/order/specimen/observation groups and NTE ownership rather than flattening segment lists. 231 ch.7 §7.2.1 pp.487–488; 251 ch.2 §2.5.2 p.6, ch.7 §§7.3.1, 7.3.7–9 pp.12–13, 21–25.
- [ ] B — Serialize using the selected version's grammar, delimiters and escaping, and retain raw bytes independently of normalized rendering. 231 ch.2 §2.10 pp.104–105; 251 ch.2 §2.6 pp.12–17; project raw-retention rule.

### C. Field/datatype validation

- [ ] B — Apply every supported field's declared datatype, length, required/conditional usage, repetitions and table constraints at the owning boundary. 231 ch.2 §§2.6, 2.8 pp.41–101; 251 ch.2 §§2.5.3, 2.12 pp.7–11, 40–58; ch.2A datatype definitions.
- [ ] B — Preserve complete composite IDs and assigning authorities, including EI/HD/CX as used by the edition. 231 ch.2 §§2.8.17, 2.8.20 pp.67–71; 251 ch.2A §§2.A.14, .25–26, .33 PDF pp.19–28, 34–36, 40–43.
- [ ] B — Retain coded-entry identifiers, display text, coding systems and alternate codes without equating display labels with identity. 231 ch.2 CE definition §2.8.3 pp.50–51; 251 ch.2A §§2.A.6, .8, .13 PDF pp.8–12, 17–19.
- [ ] 251 — Distinguish CE, CWE and CNE constraints; unknown local codes are not automatically legal under closed coding. Ch.2A §§2.A.6, .8, .13 PDF pp.8–12, 17–19.
- [ ] B — Parse NM and SN according to their own syntax, keep comparators/ranges/ratios, and never silently apply locale conversion. 231 ch.2 §2.8.27 p.72 and ch.7 §7.3.2.2 pp.507–509; 251 ch.2A §§2.A.47, .70 PDF pp.53–54, 85–86.
- [ ] B — Preserve date/time precision, supplied offset and lexical form without inventing missing precision. 231 ch.2 §§2.8.15/.41/.44 pp.65, 87–89; 251 ch.2A §§2.A.21–22, .75, .77 PDF pp.31–33, 89–90, 95.
- [ ] B — Parse OBX-5 through OBX-2, including repetitions and composite values. 231 ch.7 §§7.3.2.2–5 pp.507–512; 251 ch.7 §§7.4.2.2–5 pp.43–49.
- [ ] B — Keep ST/TX/FT semantics and repeats distinct; text repetition is not always multiple independent numeric observations. Same OBX clauses; 251 ch.2A §§2.A.31, .74, .78 PDF pp.39–40, 89, 95–96.
- [ ] B — Handle ED/RP with type/subtype/encoding metadata or park as unsupported; bound decoded binary expansion. 231 ch.2 §2.8.16 pp.65–67; 251 ch.2A §§2.A.24, .65 PDF pp.33–34, 77–80; bound is project policy.
- [ ] 251 — Handle MSG/VID/PT and error-location datatypes according to 251 rather than a 231 composite layout. Ch.2A §§2.A.44, .81, .57, .27–28 PDF pp.50–51, 97–98, 69–70, 36–38.
- [ ] B — Support each additional datatype referenced by the declared message/profile, or reject that capability explicitly; a raw parser cannot claim datatype conformance. 231 ch.2 §2.8; 251 ch.2A §§2.A.1–89 PDF pp.3–117 (per-type component/usage rules are the schema source).

### D. Character encoding

- [ ] B — Apply edition-specific MSH-18 default and code-table rules; do not guess from local OS settings. 231 ch.2 §2.24.1.18 pp.144–145; 251 ch.2 §2.15.9.18 pp.79–81.

  **Partial evidence:** [control.rs](../../../crates/protocols/hl7/tests/control.rs): `hl7_231_2_24_1_18_and_251_2_15_9_18_charset_is_edition_specific` tests the supported ASCII, Latin-1 and UTF-8 subset by edition, including default rejection and explicit override for legacy UTF-8. [config.rs](../../../crates/core/config/tests/config.rs): `hl7_231_2_24_1_18_utf8_requires_named_vendor_departure` requires a named reason and limits the departure to 2.3.1 with UTF-8 configured. Other table encodings remain unsupported.

- [ ] B — Implement declared alternate-set switching and MSH-20 handling or explicitly reject the unsupported scheme. 231 ch.2 §§2.9.2, 2.24.1.20 pp.102, 145; 251 ch.2 §§2.7.2, 2.15.9.20 pp.17–18, 82.
- [ ] 251 — Enforce the charset table's UTF-8 restrictions; MLLP's UTF-8 capability does not independently authorize every HL7 header usage. Ch.2 §2.15.9.18 pp.79–81.
- [ ] M — Agree a connection encoding compatible with R1; exclude UTF-16/32 and embedded conflicting control bytes. MLLP §1.3 pp.2–3.
- [ ] B — On undecodable or contradictory charset input retain bytes and report failure without replacement-character corruption. Above clauses plus project retention policy (engineering gate).

- [x] B — Validate MSH-9 component presence by edition: all three in 2.5.1, optional trigger on 2.3.1 responses/acknowledgments. 251 section 2.A.44 pp.2-180–181; 231 section 2.24.1.9 p.2-95.

  **Evidence:** [control.rs](../../../crates/protocols/hl7/tests/control.rs): `hl7_251_2_a_44_and_231_2_24_1_9_message_type_components` checks required components, component lengths, optional legacy response triggers and unchanged source bytes.

- [x] B — Require a valid MSH-7 in 2.5.1; permit its omission in 2.3.1 and validate supplied timestamps. 251 ch.2 sections 2.15.9, 2.15.9.7 pp.74, 76; 231 ch.2 section 2.24.1.7.

  **Evidence:** [control.rs](../../../crates/protocols/hl7/tests/control.rs): `hl7_251_2_15_9_7_requires_valid_message_time_but_231_allows_absence` checks missing/null, invalid dates, repetition/composite misuse, valid reduced precision and unchanged source bytes.

Delimiter escapes are decoded after selecting fields/repetitions/components, never
before structural extraction. `original` retains its wire spelling, and decoded
plain text is separate. `hl7_251_2_7_4_decodes_selected_identifiers_and_values_keeps_wire_originals`
and `hl7_251_2_7_4_decoding_uses_message_delimiters_and_is_not_recursive`
(mapping tests) cover identifiers, coded/text values, repeats, alternate delimiters,
non-recursive decoding and byte-for-byte source retention. Source: 2.5.1 section
2.7.4, p.2-18. No hexadecimal/local/repertoire agreement is inferred.

Formatting/highlighting in result values is retained and parked, with no clean decoded
text. `hl7_251_2_7_3_2_7_6_formatting_is_unsupported_evidence_not_clean_text`
(mapping tests) checks ST/TX/FT, original spelling, absent decoded text and the
specific finding. `hl7_251_2_7_6_formatting_command_grammar` (control tests)
checks command names and argument grammar. Source: 2.5.1 sections 2.7.3, 2.7.6,
pp.2-18 through 2-20. Hex/local/repertoire escapes require an agreement; none is
inferred (`hl7_251_2_7_unsupported_escapes_never_become_clean_text`).

### E. Original/enhanced ACK and errors

- [ ] B — Both MSH-15/16 unvalued selects original mode; otherwise validate and apply the enhanced condition fields. 231 ch.2 §§2.12.1, 2.24.1.15–16 pp.107–112, 143–144; 251 ch.2 §§2.9, 2.15.9.15–16 pp.24–28, 78.
- [ ] B — Apply AL/NE/ER/SU independently to acceptance and application outcomes; test ACK suppression as well as positive/negative responses. Same clauses.
- [ ] B — Original mode reports AA only for successful agreed application processing, AE for content/application error and AR for rejection. 231 §2.12.1.2.1 pp.109–110; 251 §2.9.2 pp.26–27.
- [ ] B — Enhanced mode safely commits before positive acceptance and distinguishes CA from CE/CR; no CA may precede durable responsibility. 231 §2.12.1.2.2 pp.110–112; 251 §2.9.3 pp.27–28.
- [ ] B — Treat application response as its own exchange with correct correlation and no infinite application-ACK chain. Same enhanced clauses.
- [ ] B — Create response MSH-10/time anew, address recipient from sender identity and copy MSA-2 from the triggering message ID. 231 §2.12.1.2 pp.110–111; 251 §§2.9.2.2, 2.9.3.2 pp.26, 28.
- [ ] B — Validate MSA codes and keep application, acceptance, query and order status separate. 231 §§2.24.2.1, 2.24.22 pp.146, 168–169; 251 §§2.15.8, ch.5 §5.5.2 pp.73–74, 44–45.
- [ ] 231 — Implement structured MSA-6 and ERR-1 code/location with segment occurrence and field position. Ch.2 §§2.24.2.6–2.24.3.1 pp.147–148.
- [ ] 251 — Implement ERR location, HL7 code, severity, application code/parameters and diagnostic/user details where applicable; handle legacy fields under compatibility policy. Ch.2 §2.15.5 pp.66–70, §2.15.8 pp.73–74.
- [ ] B — A functional order/query reply must not be replaced by a generic positive ACK. 231 ch.4 §4.2 pp.288–289, ch.2 §2.18 pp.120–121; 251 ch.4 §4.4 pp.6–25, ch.5 §5.4 pp.36–42.
- [ ] B — Declare legacy deferred-processing/MCF support separately from enhanced acceptance and deferred queries. 231 ch.2 §§2.12.2–2.13 pp.112–114; 251 ch.2 §2.14 p.61 and ch.5 §5.6 pp.52–60.
- [ ] B — Test disk failure, response loss, late/mismatched ACK, duplicate message ID with changed content and restart after commit/before reply. Processing rules above plus project safety policy; timeout does not prove failure (engineering gate).

Sender ACK phases follow 2.5.1 sections 2.9.2-2.9.3 pp.2-26-28.
`hl7_251_2_9_2_2_9_3_sender_preserves_ack_phases` (dialog tests) checks early
application replies, nonterminal CA and duplicate CA in the application phase.
`hl7_251_2_15_8_all_codes_and_mismatched_ack` now treats CA/CE/CR in original
mode as specific wrong-phase errors rather than completion.
`hl7_251_2_9_3_sender_acceptance_and_application_condition_matrix` checks the
16 AL/NE/ER/SU combinations and condition violations.
`hl7_251_2_9_3_sender_application_wait_is_bounded_and_never_resends` checks
the separate application deadline, timeout/disconnect uncertainty and no resend.
Both phases disabled reports issuance with no requested response, never acceptance.

ERR locations use the actual source segment occurrence and field when known,
and remain unvalued when reconciliation cannot identify a source field. HL7 and
LAW use one host builder. Source: 2.5.1 section 2.15.5.2 pp.2-67-68.
`hl7_251_2_15_5_2_adr0003_ae_reports_actual_mapping_location` (receive tests)
covers the second OBX and an unlocated conflict; `hl7_251_2_15_5_2_unknown_location_stays_unvalued`
(control tests) checks wire rendering. LAW's
`law_tf2b_3_29_4_b3_b7_conflict_rejects_batch_and_complete_og_distinguishes_runs`
(delivery tests) checks the same second-OBX mapping failure.

### F. Laboratory results, status and identity

- [ ] B — Implement ORU^R01 and ACK with edition-specific grouping, requiredness and optional context/notes. 231 ch.7 §7.2.1 pp.487–488; 251 ch.7 §7.3.1 pp.12–13.
- [ ] B — Preserve order identifiers, specimen identifiers and observation IDs as distinct scoped data. 231 ch.7 §§7.3.1.2–3, 7.3.2.3–4 pp.490–491, 509–511; 251 ch.7 §§7.4.1–2 pp.26–49.
- [ ] B — Identify observations with code system/sub-ID/order context, not OBX-1, test name, value equality or receipt ID alone. Same observation clauses.
- [ ] B — Preserve multiple same-key OBXs as a replacement unit and handle repeated OBX-5 values without discarding members. 231 §§7.3.2.5/.11 pp.511–512, 527–528; 251 §§7.4.2.5/.11 pp.46–49, 54–55.
- [ ] B — Preserve and handle all OBX-11 states: pending/unverified/preliminary/partial/final/corrected/deleted/wrong/unobtainable/status-only and order-detail states. 231 §7.3.2.11 pp.527–528; 251 §7.4.2.11 pp.54–55.
- [ ] B — Keep OBR-25 report status separate from OBX-11 and ORC control/status, including their precedence rules. 231 §7.3.1.25 p.502; 251 §7.4.1.25 p.35; order-control definitions in ch.4.
- [ ] B — Carry units, ranges, abnormal flags, probability and abnormal-test context without substituting vendor semantics globally. 231 §§7.3.2.6–10 pp.512–527; 251 §§7.4.2.6–10 pp.48–54.
- [ ] B — Preserve observation/report times, producer, observer and method separately when supplied. 231 §§7.3.1.22, 7.3.2.14–17 pp.500, 528–529; 251 §§7.4.1.22, 7.4.2.14–19 pp.34, 55–57.
- [ ] B — Preserve parent/child relationships rather than flattening microbiology or reflex relationships into equal sample/test rows. 231 §§7.3.1.26/.29 pp.502–503; 251 §§7.4.1.26/.29 pp.35–37 and ch.2A §2.A.56 PDF pp.68–69.
- [ ] 251 — Implement OUL^R21, specimen-oriented R22, specimen-container R23 and order-oriented R24 distinctly when advertised, including SPM/SAC/TCD/SID participation. Ch.7 §§7.3.2, 7.3.7–9 pp.14–16, 21–25.
- [ ] 251 — Implement the selected SPM fields, specimen role, parent IDs, collection/receipt/rejection and availability semantics; do not infer specimen identity from an arbitrary order field. Ch.7 §7.4.3 pp.59–71.
- [ ] 251 — Preserve OBX-18 equipment hierarchy, OBX-19 analysis time and relevant performing-organization fields, where the profile uses them. Ch.7 §§7.4.2.18–25 pp.57–59.
- [ ] B — Do not treat OBX-21 as a standard persistent ID in either target edition. 231 OBX definition ch.7 §7.3.2 pp.506–529; 251 §7.4.2.21 p.57 reserves it.
- [ ] 251 — Declare whether point-of-care R30/R31/R32 transactions are supported, with their different order-correlation responsibilities; do not equate them to R01. Ch.7 §§7.3.4–6 pp.17–20.
- [ ] B — Declare result-query QRY/ORF R02/R04 support and its version-specific response structure. 231 ch.7 §7.2.2 p.488; 251 §7.3.3 p.16.
- [ ] B — Test U with no repeated value, D/W withdrawal, missing predecessor, out-of-order correction, same-value rerun, multiple orders and mixed result/status records. Status/identity clauses above plus safety policy (engineering gate).

### G. Orders and lifecycle

- [x] B — Never emit PID with only a patient ID; omit the optional patient group, or reject a machine requiring patient identification before reservation because contract v1 has no name. 231 section 3.3.2 p.3-36; optional-group structures: 231 section 4.2.1 and 251 section 4.4.6.

  **Evidence:** [order.rs](../../../crates/protocols/hl7/tests/order.rs): `hl7_231_ch4_explicit_group_position_and_identifier_do_not_invent_context` asserts no PID for identifier-only context. [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs): `hl7_231_3_3_2_required_pid_rejects_before_reservation_without_inventing_name` checks the specific rejection reason, no Sending history and no wire bytes in both editions. Configure `orders.hl7_send.require_patient_identification = true` only when the machine requires PID; such work is refused while contract v1 lacks names.

- [x] B — Encode ORC-4 as an EI with the exact ORC-2 authority components; reject complete EIs over 22 bytes before reservation. 231 section 4.3.1.4 p.4-17; 251 section 4.5.1.4 p.4-39; each edition's ORC attribute table.

  **Evidence:** [order.rs](../../../crates/protocols/hl7/tests/order.rs): `hl7_231_4_3_1_4_251_4_5_1_4_group_ei_authority_and_complete_field_limit` checks local/universal authorities and exact/overflow limits in both editions. [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs): `hl7_231_4_3_1_4_251_4_5_1_4_full_group_ei_overflow_precedes_reservation` checks a scalar that fits but whose complete EI overflows, specific refusal, no Sending history and no wire bytes.

Audit scope: ORM/O01–ORR/O02 in both editions; non-LAW O21/O22 in 2.5.1;
O33/O34 only under the LAW profile. Generic work supports new/cancel requests,
one specimen/test per order and a non-repeating timing window. A checked
conditional row covers only its declared subset, including explicit rejection
of excluded capabilities. Partial rows stay unchecked with the remaining work
named. All evidence below is documented-level synthetic testing, not machine
qualification or proof of host durability/recovery.

- [x] B — Implement declared ORM^O01/ORR^O02 grammars and repeat groups, not merely names. 231 ch.4 §§4.2.1–2 pp.288–289; 251 ch.4 §§4.4.1–2 pp.6–9.

  **Evidence:** [order.rs](../../../crates/protocols/hl7/tests/order.rs): `hl7_231_4_2_1_and_251_4_4_1_build_new_and_cancel_groups`, `hl7_231_4_3_1_b_and_251_4_5_1_b_obr_identity_fallback_and_response_order`, `hl7_231_4_2_2_and_251_4_4_2_multiple_details_are_not_flattened`. Declared scope: laboratory OBR detail, one patient context and one test/battery per order; other detail families are rejected.
- [x] B — Model each supported ORC request, response and status explicitly; distinguish requested cancel/change from acknowledgment that it happened. 231 ch.4 §4.3.1 pp.290–307; 251 ch.4 §4.5.1 pp.27–50.

  **Evidence:** [order.rs](../../../crates/protocols/hl7/tests/order.rs): `hl7_231_4_3_1_1_and_251_4_5_1_1_exhaustive_request_reply_pairs`, `hl7_231_4_3_1_5_and_251_4_5_1_5_snapshot_does_not_change_request_outcome`, `hl7_231_4_3_1_5_and_251_4_5_1_5_invalid_status`. Supported controls are NW/CA and OK/UA/CR/UC; status is retained as a peer snapshot. Change/discontinue controls are explicitly unsupported, not aliases for acceptance.
- [ ] B — Preserve placer/filler namespaces, parent/group relationships and identity on changes/retries. Same ORC clauses; OBR definitions in each ch.7.

  **Remaining:** parent/placer-group and change relationships in the generic ORM/O21 API, while host recovery is covered in [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs), `adr0003_hl7_restart_during_sending_recovers_items_without_resend`. Existing namespace and replay evidence: [order.rs](../../../crates/protocols/hl7/tests/order.rs), `hl7_231_4_3_1_2_3_and_251_4_5_1_2_3_universal_namespaces_preserved` and `hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities`. LAW-specific parents/groups have separate [law_order.rs](../../../crates/protocols/hl7/tests/law_order.rs) coverage; that does not establish generic support in both editions.
- [ ] B — Validate requested timing/priority/specimen and required order detail; retain unsupported order content with a specific rejection. 231 ch.4 §4.3 pp.290–338; 251 ch.4 §§4.5.1–4.5.5 pp.27–80.

  **Remaining:** complete optional order-detail/component predicates ; host rejection and operator behavior exist in [program orders](../../../crates/analyte/ORDERS.md); cross-field timing interpretation stays with the host. The current coded specimen, explicit priority/non-repeating window and rejection subset is covered by [order.rs](../../../crates/protocols/hl7/tests/order.rs), `hl7_231_2_8_and_251_2a_caller_data_rejects_injection_and_oversize`, and [laboratory_order.rs](../../../crates/protocols/hl7/tests/laboratory_order.rs), `hl7_251_4_4_6_builder_is_bounded_and_replay_is_pure`.
- [ ] 251 — Implement OML^O21/ORL^O22, multiple-orders-per-specimen O33/O34, and single-container O35/O36 separately where declared. Ch.4 §§4.4.6–11 pp.14–25.

  **Partial evidence:** [laboratory_order.rs](../../../crates/protocols/hl7/tests/laboratory_order.rs): `hl7_251_4_4_6_builds_order_owned_timing_specimen_and_cancel`, `hl7_251_4_4_7_correlates_typed_decisions_and_specimen_container_groups`, `hl7_251_4_4_6_7_rejects_wrong_version_and_transaction_family`. [law.rs](../../../crates/protocols/hl7/tests/law.rs): `law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options`, `law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions`. Declared families: non-LAW O21/O22 and LAW O33/O34. This row stays open: the cited tests do not exercise O35/O36. O35/O36 and unprofiled O33/O34 are not selected; they are not routed through another family.
- [ ] 251 — Handle order groups with SPM/SAC, associated OBX and specimen/container-specific instructions without moving fields between groups. Same OML/ORL definitions; ch.7 §7.4.3; ch.13 §13.4.3.

  **Remaining:** associated OBX groups and specimen/container-specific instructions in the generic O21/O22 API; it currently supports one specimen per new order and preserves returned SAC groups. Evidence for that subset: [laboratory_order.rs](../../../crates/protocols/hl7/tests/laboratory_order.rs), `hl7_251_4_4_6_7_specimens_stay_with_their_own_orders`, `hl7_251_4_4_7_orl_sac_requires_spm`. LAW enhanced-content evidence is separate: [law.rs](../../../crates/protocols/hl7/tests/law.rs), `law_tf2b_2_2_4_app_supplied_patient_specimen_container_and_order_content`.
- [ ] 251 — Handle TQ1/TQ2 timing/relationships where declared rather than assuming only the legacy TQ field. Ch.4 §§4.5.4–5 pp.71–80.

  **Partial evidence:** [laboratory_order.rs](../../../crates/protocols/hl7/tests/laboratory_order.rs): `hl7_251_4_4_6_builds_order_owned_timing_specimen_and_cancel`, `hl7_251_4_5_4_orl_repeated_window_scope`, `hl7_251_4_5_5_orl_unsupported_timing_relationship`. [law_order.rs](../../../crates/protocols/hl7/tests/law_order.rs): `law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option`. This row stays open because TQ2 relationships are not implemented. Declared scope is one TQ1 window/priority in O21 and profiled LAW priority; repeated windows and TQ2 are excluded and rejected, not silently read as legacy TQ.
- [x] B — Declare order-status-query and unsupported order-family behavior; all-HL7 claims otherwise include more than laboratory ordering. 231 ch.4 order chapter; 251 ch.4 §4.4.3 p.9 and §§4.4.4–5 pp.10–14.

  **Evidence:** [order.rs](../../../crates/protocols/hl7/tests/order.rs): `hl7_231_4_2_3_and_251_4_4_3_unsupported_order_families_are_located_errors`. Status-query replies, dietary/pharmacy/supply and other order families have no builder in this API; a typed order parser rejects an unexpected event/structure at MSH-9 and unsupported controls at ORC-1. This declaration does not claim a host-wide dispatch/rejection dialog.
- [ ] B — Test accept/reject/cancel/unable-to-cancel/change and uncertain delivery with stable identities, never issuing new machine orders as a side effect of replay. Same order rules plus project order-lifecycle policy.

  **Remaining:** change workflows and broader cancellation-race qualification. Program send/recovery tests exist in [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs): `hl7_251_no_reply_disconnect_and_restart_never_resend` and `hl7_231_251_cancel_accepted_or_refused_restores_prior_state_once`. Pure acceptance/cancellation and stable replay evidence: [order.rs](../../../crates/protocols/hl7/tests/order.rs), `hl7_231_4_3_1_and_251_4_5_1_reports_each_matching_outcome`, `hl7_231_4_2_2_and_251_4_4_2_message_ack_is_not_order_acceptance`, `hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities`. These tests neither send orders nor prove durable host recovery.

### H. Queries, responses and continuation

Declared query transactions are legacy Q02/Q03 in both editions and LAW
QBP/Q11–RSP/K11. Wire builders/views do not establish a connection lifecycle or
make an order cache trustworthy. The host owns filter interpretation, deadlines,
durable responsibility and recovery; unchecked integration rows remain open.

- [ ] 231 — Implement declared immediate QRY/DSR Q01 and deferred QRY/QCK Q02 followed by DSR/ACK Q03. Ch.2 §§2.17–2.18 pp.120–121.

  **Remaining:** immediate Q01. Deferred Q02/Q03 runtime evidence is in [hl7.rs](../../../crates/analyte/tests/orders/hl7.rs), `hl7_231_251_query_found_reserves_before_dsr_ack_aa_sends_once`. The declared pure Q02/Q03 subset has [query.rs](../../../crates/protocols/hl7/tests/query.rs) evidence: `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`. Q01 is not silently treated as Q02.
- [x] 251 — Treat those older query exchanges as backward-compatible legacy capabilities, not the modern query-by-parameter model. Ch.5 §5.10 pp.113–137.

  **Evidence:** [query.rs](../../../crates/protocols/hl7/tests/query.rs): `hl7_231_2_18_qck_answers_deferred_query` (both editions), `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`. Legacy QRD/QRF transactions remain separate from LAW QBP/QPD queries.
- [x] B — Validate QRD format/priority/query ID/quantity/filter and QRF fields according to the edition; DSP positions have no generic laboratory meaning. 231 ch.2 §§2.24.4–5/.9 pp.148–153, 156–157; 251 ch.5 §5.10.5 pp.123–137.

  **Evidence:** [query_validation.rs](../../../crates/protocols/hl7/tests/query_validation.rs): `hl7_231_2_24_4_5_and_251_5_10_5_3_4_field_usage_lengths_and_repetitions`, `hl7_231_2_24_4_5_and_251_5_10_5_3_4_preserve_local_filters`, plus its per-field named negative fixtures and boundary/property cases. [query.rs](../../../crates/protocols/hl7/tests/query.rs): `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr` demonstrates caller-supplied display text without a laboratory row interpretation. Valid field syntax does not establish that the host can honor every filter/unit.
- [ ] B — Keep query tag/QRD ID distinct from per-message MSH-10/MSA-2 correlation, especially across continuation. 231 ch.2 §§2.24.4.4, 2.24.22 p.149, 168–169; 251 ch.5 §§5.5.2–4 pp.44–47.

  **Partial evidence:** [query.rs](../../../crates/protocols/hl7/tests/query.rs): `hl7_231_2_24_4_query_id_is_preserved`, `hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr`, `hl7_231_2_18_dsr_requires_data_and_distinct_response_ids`. This row stays open: these tests check only the declared non-continuation exchange; continuation identity/lifetime support remains open below.
- [x] B — Preserve no-data versus application error/rejection; never answer NF on an unknown lookup. 231 ch.2 §2.24.22.2 p.169; 251 ch.5 §§5.5.2, 5.6.5 pp.44–45, 60–61; project rule 9.

  **Evidence:** [query.rs](../../../crates/protocols/hl7/tests/query.rs): `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf` (both editions). [law.rs](../../../crates/protocols/hl7/tests/law.rs): `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`. Unavailable lookup is an error, not a trusted empty result; malformed/unsupported requests cannot be constructed into a positive typed response.
- [ ] 251 — Define query parameters and exact response using a query conformance statement; QBP alone does not specify the lab's lookup contract. Ch.5 §§5.3, 5.4.1 pp.14–37.

  **Remaining:** any non-LAW QBP contract requires its own selected query conformance statement before implementation. The only current query-by-parameter contract is IHE LAB-27, documented in [ihe-law.md](../law/README.md) and tested by [law.rs](../../../crates/protocols/hl7/tests/law.rs), `law_tf2b_3_27_4_query_forms_and_conditional_fields`; this is not a general lab-app lookup contract.
- [ ] 251 — Implement selected QBP/RSP, QBP/RTB and QBP/RDY pairs with their own structures and event/structure IDs. Ch.5 §§5.4.1–3 pp.37–40.

  **Partial evidence:** [law.rs](../../../crates/protocols/hl7/tests/law.rs): `law_tf2b_3_27_4_query_forms_and_conditional_fields`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`. This row stays open: only LAW QBP/Q11–RSP/K11 is selected; RTB and RDY transactions are not declared or implemented. [query.rs](../../../crates/protocols/hl7/tests/query.rs), `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events`, prevents legacy fallback.
- [ ] 251 — Validate QPD query name/tag/parameters, RCP priority/quantity/mode, QAK status/counts and echoed correlation. Ch.5 §§5.5.2–6 pp.44–50.

  **Remaining:** generic QPD/RCP/QAK profiles, quantitative response/count handling and contracts beyond the LAW subset. Existing selected LAW scope/tag/mode and response-echo evidence: [law.rs](../../../crates/protocols/hl7/tests/law.rs), `law_tf2b_3_27_4_forbidden_missing_null_and_repeated_parameters_retain_bytes`, `law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup`. Those cases do not establish arbitrary query/profile conformance.
- [ ] 251 — Handle RDF/RDT row schemas and QRI instance/match information if those response forms are supported. Ch.5 §§5.5.5, .7–8 pp.47–52.

  **Remaining:** RDF/RDT/QRI schemas and validation if a tabular or match-list response is selected. These response forms are currently unselected and unimplemented; no typed API claims to consume them.
- [ ] B — Implement declared DSC continuation, cancellation and logical response boundaries without confusing them with TCP reads. 231 ch.2 §§2.15.4–5, 2.24.8 pp.118–119, 156; 251 ch.5 §§5.6.2–3 pp.55–59; ch.2 §2.15.4 pp.65–66.

  **Remaining:** DSC pointer lifetime, continuation/cancellation state and logical batch completion in the host dialog. The current query view rejects continuation and retains the original message: [query.rs](../../../crates/protocols/hl7/tests/query.rs), `hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located`.
- [ ] 251 — Distinguish fragmentation from interactive continuation and preserve pointer lifetime/correlation. Ch.2 §§2.10.2, 2.15.4 pp.30–34, 65–66; ch.5 §5.6.3 pp.55–59.

  **Remaining:** message fragmentation/reassembly and interactive continuation with persisted correlation and pointer expiry. Receiving a complete bounded Message or rejecting DSC is not evidence of this lifecycle.
- [ ] 251 — Declare subscription/replay and query/subscription cancellation support; implement QSB/QVR/QCN/QSX only with their lifecycle. Ch.5 §§5.4.4–7, 5.7 pp.40–42, 61–66.

  **Partial evidence:** [query.rs](../../../crates/protocols/hl7/tests/query.rs): `hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events` tests QSB/Q16, QVR/Q17, QCN/J01 and QSX/J02. This row stays open: the tests exercise rejection, not subscription/replay/cancellation lifecycles. These capabilities and subscription/replay lifecycles are explicitly unselected; no typed query handler treats them as legacy lookup or starts work.
- [x] 231 — Declare EQQ/VQQ/SPQ/ERQ and enhanced query response support separately; these are not QBP/RSP. Ch.2 §§2.19–2.22 pp.121–123.

  **Evidence:** [query.rs](../../../crates/protocols/hl7/tests/query.rs): `hl7_231_2_19_2_20_undeclared_enhanced_query_forms_are_rejected`, `hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located`. EQQ, VQQ, SPQ and event replay are explicitly unsupported. ERQ is an event-replay segment, not another QBP transaction; ERP/R09 and ERQ content are not accepted by the legacy handler.
- [ ] B — Test successful empty/multiple/partial responses, unavailable lookup, late response, cancellation and restart without unintended order retransmission. Query clauses above plus project deadlines/order rules.

  **Remaining:** logical multi-part/partial replies and broader cancellation qualification. Host tests in [hl7.rs](../../../crates/analyte/tests/orders/hl7.rs) cover `hl7_251_dsr_ack_ae_ar_lost_and_disconnect_are_specific_uncertain` and `hl7_251_adr0004_total_query_deadline_includes_split_block_receive_time`; [hl7_send.rs](../../../crates/analyte/tests/orders/hl7_send.rs) covers restart without resend. Current pure wire evidence is [query.rs](../../../crates/protocols/hl7/tests/query.rs), `hl7_231_2_18_qck_answers_deferred_query`, `hl7_231_2_18_dsr_requires_data_and_distinct_response_ids`, `hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf`; it does not exercise a connection state machine or durable recovery.

### I. Special control protocols

- [ ] B — If sequence protocol is negotiated, persist last accepted state and implement initialization, expected-number checks and resynchronization. 231 ch.2 §2.23.1 pp.123–125; 251 ch.2 §2.10.1 pp.29–30.
- [ ] B — If message/segment continuation is supported, reconstruct correct identity and boundaries rather than delivering partial content as complete. 231 ch.2 §2.23.2 pp.125–126; 251 ch.2 §2.10.2 pp.30–34.
- [ ] B — If file/batch support is advertised, implement FHS/BHS/BTS/FTS ordering, counts and acknowledgment semantics. 231 ch.2 §2.23.3 pp.126–128; 251 ch.2 §2.10.3 pp.34–36 and control segments §§2.15.2–3/.6–7 pp.63–65, 71–73.
- [ ] B — Apply snapshot/action-code repeating-segment update semantics where specified. 231 ch.2 §2.23.4 pp.128–129; 251 ch.2 §2.10.4 pp.36–37.
- [ ] B — Explicitly reject unsupported negotiated control modes rather than ignoring their fields and partially processing data. Processing/profile rules above; project fail-visible policy.

### J. Relevant 2.5.1 laboratory automation

These capabilities matter to a literal “complete” claim and to future equipment automation. They need not all be enabled on a result-only link. Add no command subsystem merely by implication: resolve F01/F16 scope with the owner.

- [ ] 251 — Declare roles and support for equipment status ESU/ESR U01/U02 and specimen status SSU/SSR U03/U04. Ch.13 §§13.3.1–4 pp.15–17.
- [ ] 251 — Declare inventory INU/INR U05/U06, commands EAC U07 and completion EAR U08, and notifications EAN U09. Ch.13 §§13.3.5–9 pp.17–20.
- [ ] 251 — Declare test-code settings TCU/TCR U10/U11 and equipment service/log LSU/LSR U12/U13. Ch.13 §§13.3.10–13 pp.20–22.
- [ ] 251 — Preserve equipment identity/state, interaction state and notifications using EQU/ISD/NDS/CNS where supported. Ch.13 §§13.4.1–2/.7–8 pp.23–25, 46–48.
- [ ] 251 — Preserve SAC container, parent, carrier, position, volume, status and related information with correct specimen association. Ch.13 §13.4.3 pp.25–38.
- [ ] 251 — Keep inventory/material quantities and lot identifiers and command/response correlation distinct from result acceptance. Ch.13 §§13.4.4–6 pp.38–46.
- [ ] 251 — Preserve TCC/TCD dilution/configuration, SID material identification and EQP service data where supported. Ch.13 §§13.4.9–12 pp.48–55.
- [ ] 251 — For QC carry control specimen role/source, container/material identity, equipment identity and analysis time; this transports evidence, not QC decisions. Ch.13 §13.5.3 p.55; ch.7 §§7.4.2.18–19 p.57.
- [ ] 251 — If claiming this automation profile scope, account for clock-synchronization receiver support rather than inventing a timer command. Ch.13 §13.5.1 p.55 (full transaction definition requires the applicable administrative message, beyond these lab chapters).
- [ ] 251 — Do not invent a universal dedicated lab-order query from ch.13; distinguish unsolicited/implicit triggers from separately profiled QBP/RSP. Ch.13 §13.5.4 p.55.
- [ ] 251 — Resolve stale ch.13 cross-references against actual OML/OUL definitions before encoding messages. Ch.13 §§13.3 introductory text, 13.5.2 pp.15, 55 versus ch.4 §4.4.6 p.14 and ch.7 §7.3.2 p.14.

### K. MLLP R1 and recovery engineering

- [ ] M — Read exactly VT, compatible payload, FS, CR; preserve segment-ending CR independently of block trailer. MLLP §1.2 p.2; HL7 delimiter rules.
- [ ] M — Validate permitted content/control bytes and encoding; arbitrary binary content is not a raw pass-through inside R1. MLLP §§1.2–1.3 pp.2–3.
- [ ] B — Decode split headers/trailers and coalesced blocks incrementally; TCP reads are not message boundaries. MLLP §1.2 plus stream-handling obligation.
- [ ] B — Bound pending bytes, complete/partial frame size, decoded size and every wait; reject visibly on overlimit. Project safety rule 7; HL7 does not give one universal maximum (231 ch.2 §2.3 p.40; 251 ch.2 §2.4 pp.5–6).
- [ ] B — Define recovery for partial EOF, invalid trailer, unexpected start/control byte and timeout; retain diagnostic raw evidence in the protected store. MLLP §1.2 plus project retention policy.
- [ ] M — Do not add ASTM handshake/checksum or assume an R1 byte ACK/NAK/retry algorithm. MLLP §§1.1–1.2 pp.1–2.
- [ ] B — Agree connection roles, reuse/reconnect, concurrency, ACK timeout and retry policy per interface; R1 supplies no business idempotency. MLLP §1.1 pp.1–2; 231 ch.2 §2.12.1.2.1 p.110; 251 ch.2 §2.9.2.2 p.26.
- [ ] B — Test disconnect/lost reply after peer commit as uncertain outcome, retain identities across retries, and recover after restart without duplicate clinical effects. Same processing rules plus project identities/durability policy.
- [ ] B — Verify durability latency against configured peer deadlines and failure behavior on disk-full/store failure; transport success is not durable/application success. Enhanced-processing clauses plus project safety rules.


## Acceptance criteria from the open-source comparison

1. **Independent parse, validation and dialog contracts.** Structural acceptance, edition/profile correctness and clinical mapping are different outputs. A permissive library may preserve invalid input usefully without making it conformant.
2. **A version/event/schema registry.** Select explicit grammars and named vendor overlays. A fallback generic tree must retain its “not validated” status; it must not silently acquire the current version's semantics.
3. **Group-relative mapping paths with cardinality.** Global “first OBR/OBX” selection is unsafe across multiple patients/orders/specimens, repeated IDs and contextual notes.
4. **A core-owned ACK policy.** Rendering helpers do not decide whether AA or CA is warranted. Persisted state and the declared responsibility decide the response before a library formats it.
5. **Lossless raw retention plus typed interpretation.** Re-rendered messages are not original bytes. Normalized escapes, trailing separators, charset conversions or regenerated IDs can change a message even when clinical values look equal.
6. **Independent differential tests.** Compare parse/render results with more than one tool, then resolve disagreements against the standard. Two tools agreeing, or a simulator accepting a message, is not exhaustive conformance proof.
7. **A negative corpus.** Include malformed MSH/escapes, wrong table values, NM locale errors, unknown datatype, invalid group ownership, huge encoded data and charset mismatch. Assert retained failures, specific errors and actual replies, not just parse success.
8. **An identity/event corpus.** Include equal keys in different orders, multi-OBX replacement units, U/D/W, absent predecessors, same-value reruns, duplicate control IDs with changed bytes, and response IDs distinct from requests. Generic libraries do not solve clinical reconciliation.
9. **A source/fixture manifest.** Pin standard edition/section, profile, synthetic/vendor origin, tool version and known deviations. Vendor teaching examples must never become the standard suite merely because a parser accepts them.
10. **Reproducible adoption evidence.** Before dependency selection, audit current releases, license, enabled features, transitive code, bounds and Windows behavior with the agreed corpus. This report does not approve adding any library.

