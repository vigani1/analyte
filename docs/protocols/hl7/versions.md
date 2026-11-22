# HL7 versions

## 2.8.2 and 2.9.1 bounded result reception

Part A qualifies the existing bounded ORU/R01 profile, without selecting the
optional participation, device, access-restriction or specimen groups in
§7.3.1 of either edition (2.8.2 pp.13–16; 2.9.1 pp.9–13).
CWE adds a third equivalent coding tuple and OID/value-set metadata:
2.8.2 §2.A.13 pp.24–32; 2.9.1 §2A.2.13 pp.25–33. Conformance lengths are
minimum receiver capacities rather than universal maxima (2.8.2 §2.5.5.3–4
pp.13–14; 2.9.1 §2.4.5.3–4 pp.15–16).
OBR-14 and other old fields are withdrawn (§7.4.1 in both editions), so their
earlier typed meanings cannot be inherited. DTM remains scalar; 2.9.1's
negative-zero zone distinguishes UTC from a known civil zero offset
(§2A.2.22 pp.39–40). These differences have explicit receiving rules and source-named tests. Both
editions are enabled for the bounded result profile. Legacy query stays refused.

## Later-edition receiving profiles

The 2.6 receiving/query implementation uses scalar DTM for MSH-7 and the
selected QRD/QRF time fields (2.6 §2.14.9.7 p.57; §5.10.4 pp.97–102).
CE and TS are withdrawn (chapter 2A §§2.A.6, 2.A.77); CWE and DTM have their
own constraints (§§2.A.13, 2.A.22). MSH-10 and MSA-2 allow 199 characters
(chapter 2 §2.14.8/9 attribute tables). XCN remains 23 components, but its
name-context, check-digit and time-component limits differ (§2.A.86 pp.2-190–193).
Deprecated nested DR and TQ filters are refused visibly; see the bounded 2.6 decision.

The 2.8.2 §5.10.2 (p.104) and 2.9.1 §5.9.2 (p.80) clauses mark the original
query forms withdrawn as of 2.7. The owner selected **result reception only** for
these editions. Any config enabling the legacy query profile fails with
`WithdrawnHl7Query`, naming the edition and withdrawing section, before runtime
or typed-result qualification. No replacement query contract is selected;
LAW's QBP path remains separate. Config enablement for results follows tested
typed support; order qualification is listed separately below. Evidence stays documented.

In 2.6, §5.10.2 (pp.95–96) retains original queries for backward compatibility;
it does not withdraw them. Analyte keeps its bounded deferred display exchange.

| Original-query capability row | Source | Config test |
| --- | --- | --- |
| 2.6 retained; bounded query config accepted | 2.6 §5.10.2 pp.95–96 | `v26_keeps_original_queries` |
| 2.8.2 withdrawn; legacy query config refused with its section | 2.8.2 §5.10.2 p.104 | `v282_withdrawn_original_query_is_refused` |
| 2.9.1 withdrawn; legacy query config refused with its section | 2.9.1 §5.9.2 p.80 | `v291_withdrawn_original_query_is_refused` |

**2.3.1 and 2.5.1** have native typed profiles; **2.3, 2.4 and 2.5** have
bounded receiving compatibility profiles described below. MLLP Release 1 is the framing
profile. **2.6** has the bounded ORU/R01 receiving and deferred display-query
profile described below. The bounded 2.8.2 and 2.9.1 profiles receive results
only. Evidence remains documented.

The lossless syntax layer retains the declared truncation character and marker
flags. All eight editions have typed receiving rules; none is silently upgraded.
Truncated input is refused before result mapping, while its exact bytes remain
available. Unselected extension fields stay raw or are explicitly refused.

| Grammar rule | Source | Test |
| --- | --- | --- |
| Optional fifth MSH-2 character; distinct separators; earlier editions refuse it | 2.8.2 §2.14.9.2, p.59; 2.9.1 §2.13.9.2, p.69 | `fifth_encoding_character_is_edition_specific` |
| Preserve sender-marked primitives and refuse clinical mapping | 2.8.2 §2.5.5.2, pp.12–13; 2.9.1 §2.4.5.2, pp.14–15 | `truncated_values_are_kept_and_refused`; mapping regression `truncated_input_never_reaches_mapping` |
| Literal truncation-character escape; bounded encoding without shortening | 2.8.2 §§2.7.1–2, pp.20–21; 2.9.1 §§2.6.1–2, pp.22–23 | `truncation_character_escape_round_trips` |
| Undeclared `\P\` retains the existing reject/preserve policy | Same escape sections | `undeclared_truncation_escape_stays_unsupported` |

### Edition registry and implementation stages

The single registry in `version/mod.rs` selects exact wire IDs, required MSH-7
usage and truncation-character availability. Each entry explicitly identifies
whether its typed field profile exists. Recognition never aliases a new edition
to 2.5.1: absent field profiles refuse interpretation and writing while syntax
retains the original bytes. Configuration parsing recognizes `v23`, `v231`,
`v24`, `v25`, `v251`, `v26`, `v282` and `v291`, which map to the exact wire IDs
below. Validation accepts all eight for their declared receiving profiles.
Eligibility is read from the same rule registry, and a recognized edition without
a qualified profile still fails validation before activation.
Unlisted values fail at the positioned TOML schema boundary with the bad value
and the recognized choices. A missing edition is not defaulted.
Order writing remains separately qualified from receiving. Enabling another edition in config
must accompany its implemented typed support and the corresponding tests.

The registry tests retain exact wire identity for all eight editions. Config
tests in `hl7_editions.rs` cover the eight supported selections and canonical
snapshots (`supported_editions_keep_exact_wire_identity`),
withdrawn-query refusal and the source-blocked 2.9.1 writer,
unlisted values and their diagnostics
(`unlisted_edition_names_value_and_allowed_ones`) and
missing declarations (`missing_edition_is_not_defaulted`).

| Registry entry | MSH-7 | Fifth MSH-2 character | Source | Source-named test in `version/tests.rs` |
| --- | --- | --- | --- | --- |
| 2.3 | Optional | Refused | 2.5.1 §§2.8, 2.15.9.7, pp.2-20–22, 2-76 | `recognizes_v23_without_upgrading` |
| 2.3.1 | Optional | Refused | 2.3.1 §§2.24.1.2/.7/.12, pp.2-94–95, 2-107 | `recognizes_v231` |
| 2.4 | Required | Refused | 2.5.1 §§2.8, 2.15.9.7, pp.2-20–22, 2-76 | `recognizes_v24` |
| 2.5 | Required | Refused | Same compatibility and MSH-7 clauses | `recognizes_v25_without_patch_upgrade` |
| 2.5.1 | Required | Refused | 2.5.1 §§2.15.9.2/.7/.12, pp.2-74–77 | `recognizes_v251` |
| 2.6 | Required | Refused | 2.6 §§2.14.9.2/.7/.12, pp.56–58 | `recognizes_v26_without_typed_fallback` |
| 2.8.2 | Required | Accepted | 2.8.2 §§2.14.9.2/.7/.12, pp.59–61 | `recognizes_v282_without_typed_fallback` |
| 2.9.1 | Required | Accepted | 2.9.1 §§2.13.9.2/.7/.12, pp.69–71 | `recognizes_v291_without_typed_fallback` |

`unlisted_edition_or_prefix_is_refused` checks unlisted versions,
patch-prefix lookalikes and whitespace. LAW's separately agreed 2.5-compatible
VID policy remains confined to LAW and never widens the base registry.

### Standard cross-reference discrepancy

HL7 2.9.1 chapter 2A refers to chapter 2 §2.5.5.2 for the truncation pattern
(for example in §2A.2.77). That reference is inconsistent with chapter 2's own
numbering: the actual 2.9.1 truncation-pattern clause is **§2.4.5.2**, pp.14–15.
Analyte cites the actual clause. The older number does match the 2.8.2 control
chapter; this does not justify applying the older edition's wording to 2.9.1.

| Edition | Difference that matters | Source |
| --- | --- | --- |
| 2.3.1 | Legacy laboratory transactions; no OBX-21 observation identifier | Existing scope and field evidence in [checklist](checklist.md) |
| 2.5.1 | Adds the selected OML/OUL/QBP transaction profiles; OBX-21 is reserved, not a trusted result identity | Existing scope and field evidence in [checklist](checklist.md) |
| 2.3, 2.4, 2.5 | Implemented receiving compatibility profiles; exact edition retained, unresolved extensions remain raw or are refused | HL7 2.5.1 §§2.8–2.8.2, pp.2-20–22; errata and details below |
| 2.6 | Bounded typed ORU/R01 and deferred query reception; bounded O21/O22 sending | Rules below |
| 2.8.2, 2.9.1 | Bounded typed ORU/R01 result reception; legacy queries refused; sending qualified only for 2.8.2 | Modern rules below |

### 2.6 difference rules and evidence

The shared parser uses one 2.6 rule-table entry, with per-field result rules in
`version/result.rs`. The receiving scope matches the bounded historical profiles:
ORU/R01 core groups, QRY/Q02, QCK/Q02, DSR/Q03 and their ACKs. It does not claim
the complete 2.6 schema. SPM/SAC, OUL/R22 and additional ORU segments such as
SFT/UAC/ROL are refused at the mapping boundary; the receipt remains protected.
Query SFT is retained without interpretation. Optional MSH profile declarations
remain raw and do not activate an undeclared profile.

All tests in the next table are in protocol `tests/syntax/receiving.rs` unless noted.

| Rule-table difference or bounded policy | Source (2.6) | Source-named test |
| --- | --- | --- |
| MSH-7 is required scalar DTM, with digit precision and at most four fractional digits; no TS precision component | §2.14.9.7 p.57; §2.A.22 pp.2-129–131 | `msh7_dtm_precision_and_scalar_structure` |
| MSH-10/MSA-2 permit 199 bytes; ACK retains exact edition and ID, uses the expanded ERR shape | §§2.14.8.2, 2.14.9.10 pp.54, 57; §§2.14.5.1–5 pp.48–50 | `control_id_length_ack_and_err_shape` |
| CE and TS value-type declarations are withdrawn and refused; CWE and DTM are qualified instead | §§2.A.6, 2.A.77 pp.2-107–108, 2-182 | `withdrawn_ce_and_ts_are_refused` |
| OBX-3/6 and CWE-valued OBX-5 use nine components and field/component limits; local coding-system versions are supplied, never invented | §§7.4.2.3/.5/.6 pp.7-40–44; §2.A.13 pp.2-115–119 | `obx_cwe_components_lengths_and_versions`, `cwe_component_length_boundaries` |
| Interpreted OBR-7/8/14/22 and OBX-12/14/19 timestamps use scalar DTM | §§7.4.1.7/.8/.14/.22 pp.7-23–28; §§7.4.2.12/.14/.19 pp.7-46–50 | `result_times_use_scalar_dtm` |
| QRD/QRF use DTM and CWE, QRD-8 uses 23-component XCN with changed check-digit, name-context and time limits; nonempty XCN-17 is refused | §5.10.4 pp.97–102; §§2.A.20, 2.A.86 pp.2-129, 2-190–193 | `xcn_check_digit_cwe_dtm_and_deprecated_range`, `qrd_qrf_use_dtm_cwe_and_refuse_tq` |
| Nonempty deprecated QRF-9 TQ is refused rather than ignoring its scheduling constraints; QRF-10 remains validated NM | §5.10.4.2.9–10 p.102 | Same QRD/QRF test |
| OBX-21 EI remains raw and is not a laboratory result identity; nonempty OBX-22 mood is refused | §§7.4.2.21–22 p.7-51 | Same result-DTM test; mapping `receiving_keeps_values_times_and_raw_identity` |
| Original/enhanced ACK custody rules and exact request edition remain independent of successful parsing | §§2.9.2–3 pp.23–26; §§2.14.9.15–16 pp.58–59 | Dialog `every_edition_acks_only_after_store_and_interpret` |

Escape and charset rules are exercised by
`utf8_charset_and_literal_escapes`
(2.6 §§2.7.1, 2.14.9.18, pp.17, 59–61); enhanced custody conditions by
`enhanced_ack_follows_custody`.

The bounded CWE rule requires an explicit version whenever either coding-system
component is populated. This includes text-only tuples and apparent `HL7nnnn`
names: an `HL7` prefix does not establish the table's ownership. Section
2.A.13.7 permits omitted versions for HL7-defined tables, but requires them for
user-defined tables. Analyte chooses the stricter explicit-version profile
rather than supplying a guessed table kind or version; see decisions.

Unselected backward-compatibility fields remain raw: OBR-5/6, OBR-15/27,
OBR-32–35, MSA-3/5/6 and ERR-1 do not acquire new common-model meanings.
Sources: 2.6 §§7.4.1.5–6/.15/.27/.32–35 pp.7-23–32; §§2.14.8.3/.5/.6
pp.54–55; §2.14.5.1 p.48. In particular, embedded NDL/CNN names are not
guessed, deprecated scheduling fields are not executed, and withdrawn delayed
ACK information never changes the dialog. Evidence:
`backward_compatible_obr_fields_stay_raw` and the MSA/ERR
control-length test above. Unknown OBX value types (including
unselected CNE) remain unsupported and cause parking. Field refusal retains the
original bytes; it never trims or converts a value to fit.

Whole-edition progress in this PR: **2.6 bounded profile completed**.
Part A / step 4 is complete for the selected receiving scopes. Part B / step 5
2.6 order writing is qualified below; config still refuses sending for the
three historical compatibility editions and 2.9.1.

Writer source audit for that remaining part: OBR-4 uses CWE in 2.6
§4.5.3.4 pp.4-41–42, 2.8.2 §4.5.3.4 pp.68–69 and 2.9.1 §4.5.3.4 pp.74–75.
An actual coding-system version is needed for non-HL7 and user-defined systems
(2.6 §2.A.13.7 pp.2-118–119; 2.8.2 §2.A.13.7 pp.29–30;
2.9.1 §2A.2.13.7 pp.30–31). It must not be invented from the message edition
or borrowed from an unrelated specimen coding system. The existing syntax
`order::Code::with_version` represents it, and the configured
`orders.hl7_send.test_version` (the machine code system's revision, ADR 0029)
and command mapper carry it for the qualified 2.6 writer.
The historical CE profiles keep their three-component contract.

The existing order scope remains ORM/O01–ORR/O02 and OML/O21–ORL/O22.
Later patient-optional reply variants are not implicitly selected: 2.8.2
§4.4.7.2 pp.16–17 uses the ORL_O41 structure, and 2.9.1 §4.4.7.2 pp.18–19
uses ORL/O53.
The existing O22 profile requires the patient response group for order decisions;
no replacement family or patient information is invented here. The 2.6 and 2.8.2 writers are qualified below; 2.9.1 remains blocked.

### Modern rule rows and tests

Production differences live in the shared edition table. Tests below are in
protocol `modern.rs`, with mapping tests in `mapping/modern.rs`. There are no
per-edition parser or mapping copies.

| Rule / selected behavior | Source | Source-named test |
| --- | --- | --- |
| Up to 22 CWE components; named authority or supplied OID for each populated code; explicit named-system revisions; preserve all tuples and metadata | 2.8.2 §2.A.13 pp.24–32; 2.9.1 §2A.2.13 pp.25–33 | `cwe_third_tuple_metadata_stays_exact`; `cwe_coding_authority_is_not_guessed` |
| Coding-system names retain their normative 12-character bound; conformance capacities are not maxima; composite bytes have an explicit 16,384-byte operational cap | Same CWE sections; 2.8.2 §§2.5.5.3–4 pp.13–14; 2.9.1 §§2.4.5.3–4 pp.15–16 | `conformance_length_is_not_a_maximum` |
| Scalar DTM remains bounded to 24 bytes; CE/TS declarations remain withdrawn and refused | 2.8.2 §§2.A.6/.22/.79 pp.7, 38–39, 82; 2.9.1 §§2A.2.6/.22/.79 pp.8, 39–40, 84 | `dtm_is_scalar_and_ce_ts_are_refused` |
| 2.9.1 negative-zero UTC has no known civil offset; retain that time raw instead of claiming parsed local time or an explicit civil offset | 2.9.1 §2A.2.22 pp.39–40; comparison with 2.8.2 §2.A.22 pp.38–39 | Mapping `utc_without_civil_offset_stays_raw` |
| Nonempty withdrawn OBR-5/6/14/15/27 refused in both; 2.9.1 also refuses OBR-16/28/32–35 | 2.8.2 §7.4.1 pp.36–47; 2.9.1 §7.4.1 pp.49–57 | `withdrawn_obr_fields_are_refused` |
| Nonempty withdrawn MSA-3/5/6 and ERR-1 refused | 2.8.2 §§2.14.8/.5 pp.53–59; 2.9.1 §§2.13.8/.5 pp.61–68 | `withdrawn_msa_and_err_fields_are_refused` |
| 2.9.1 MSH-15/16 must be populated together or both empty; explicit nulls are refused in this profile | 2.9.1 §§2.13.9.15–16 pp.71–72 | `ack_type_requests_must_be_paired` |
| ACK retains edition, delimiter declaration and MSA correlation; 199-byte control bounds; reserved delimiter characters cannot occur literally in control IDs | 2.8.2 §§2.14.8.2/.9.2/.9.10 pp.58–61; 2.9.1 §§2.13.8.2/.9.2/.9.10 pp.67–71; selected control-ID safety bound | `ack_keeps_exact_edition_and_delimiters`; `control_id_limits_and_reserved_characters` |
| An effective reference-range date requires the reference range | Both editions §7.4.2.12; 2.8.2 pp.60–61, 2.9.1 pp.70–71 | `range_date_requires_reference_range` |
| Original query views remain unavailable even though control views now exist | 2.8.2 §5.10.2 p.104; 2.9.1 §5.9.2 p.80 | `withdrawn_legacy_query_has_no_typed_view`; config refusal tests above |
| Observation identifiers use the selected named-primary authority contract; OID-only authority is refused without guessing a configured namespace | 2.8.2 §§7.4.2.3, 2.A.13 pp.53–54, 24–32; 2.9.1 §§7.4.2.3, 2A.2.13 pp.63–64, 25–33; bounded profile decision | Mapping `oid_authority_never_falls_back_to_code` |
| UTF-8 remains a single explicit charset; literal `\P\` data and copied encoded header values stay exact | 2.8.2 §§2.14.9.18, 2.7.1–2 pp.62–63, 20–21; 2.9.1 §§2.13.9.18, 2.6.1–2 pp.72–73, 22–23; chapter 2C table 0211 pp.99–100 and 209–210 respectively | `utf8_and_literal_truncation_escape_stay_exact` |
| Extended XCN in unselected 2.8.2 provider fields stays raw, including new security components; no typed XCN or clinician identity is claimed | 2.8.2 §2.A.88 pp.89–94; 2.9.1 §2A.2.88 pp.92–96 (provider fields withdrawn) | `unselected_xcn_extensions_stay_raw` |

The profile continues to reject unselected SPM/SAC, participation, device,
access-restriction and software segments rather than attach them to guessed
owners (both editions §7.3.1). Mapping tests
`hl7_2_8_2_s7_3_1_and_2_9_1_s7_3_1_modern_results_deliver_exact_values` and
`unselected_groups_and_withdrawn_fields_never_map`
exercise actual mapping and raw retention. Existing store-before-ACK dialog
tests also exercise both modern editions.

Only the named primary coded tuple is mapped today. Alternate/third tuples,
OIDs and value-set metadata are retained exactly, without OID lookup, namespace
invention, or interpreting a value set as the code's meaning. OID-only observation authority
is refused by the selected named-primary receiving profile; its bytes remain
protected. OID-only coded values cannot acquire a guessed namespace and are parked. Unselected value types, including CNE,
remain unsupported and parked. OBX-21 stays raw and OBX-22 remains refused.
The bounded explicit revision policy from 2.6 continues for named systems.

A sender marker still parks the receipt before clinical mapping. Declared
`\P\` is a literal, not a truncation flag: the mapping text inspector now honors
that declaration. Both cases have source-named protocol and mapping tests.
Generic text writing escapes literal markers; ACK control identifiers keep the
existing delimiter-free safety contract, extended to the declared marker.
Analyte does not truncate any outgoing value.

## Compatibility rule

### Source review for the legacy compatibility implementation

HL7 2.5.1 §2.8, pp.2-20–22, supplies the receiving contract for 2.3, 2.4
and 2.5; it does not reconstruct each unavailable edition's entire schema.
The implementation must distinguish these specific cases:

- §2.8.1 admits added constituents; §2.8.2 permits datatype changes only while
  preserving the earlier structure and meaning at the left. Unused information
  stays in the original message rather than receiving invented semantics.
- §2.15.9.7, p.2-76, explicitly dates required MSH-7 to 2.4.
- §2.A.44, pp.2-180–181, dates the MSG replacement of MSH-9's CM to 2.5.
- §2.15.5.1, p.2-67, dates ERR-1's compatibility-only status to 2.5 and
  identifies ERR-2/3 as its replacement.
- The 2002-04-08 errata for 2.3.1 (TS §2.8.44.1) and 2.4 (TS §2.9.47.1)
  explain digit precision, offsets and the retained precision component.
  Both also supply the missing order-sequencing component model. These supplied
  errata are unpaginated; the item names and section numbers identify the source.
- The 2.4 errata's XCN-17 item identifies the unrepresentable nested date range
  and assigns backward-compatibility usage. OBR-32–35 also have unresolved
  embedded-name representations. They must not silently acquire 2.5 meanings.

### Implemented receiving profiles

These are explicitly bounded newer-receiver contracts, not reconstructed copies
of the unavailable editions' complete schemas. The selected ORU/R01 core uses
the existing patient/order/observation grouping and configured identities, units
and codes. Legacy QRY/Q02, QCK/Q02 and DSR/Q03 use the established QRD/QRF
prefix. Every reply preserves the selected MSH-12, original correlation ID and
unavailable-versus-empty distinction; no result or order identity is synthesized.

| Edition | Admission rule and implemented differences | Regression evidence |
| --- | --- | --- |
| 2.3 | §2.8 requires the newer receiver to understand older messages. §§2.8.1–2 preserve earlier constituents and datatype prefixes. Use the established legacy control/query prefix; MSH-7 is optional. No assumption that 2.3.1 internationalization additions or later datatype extensions existed in 2.3. They remain original bytes. | `legacy_msh_identity_is_preserved`; `msh7_requirement_is_not_retroactive` |
| 2.4 | Same §2.8 receiving contract; §2.15.9.7 explicitly makes MSH-7 required. MSH-9 retains the CM-compatible shape; ERR uses the legacy combined location/code field. The 2.4 TS and sequencing errata supply the stated corrections; extended XCN-17 is not interpreted. | `msg_type_replaces_cm_only_in_later_editions`; `xcn17_nested_range_is_refused_and_kept_raw` |
| 2.5 | Same §§2.8.1–2 prefix contract, without relabeling the receipt 2.5.1. §2.A.44 dates MSG's three required components to 2.5; §2.15.5.1 dates the expanded ERR representation to 2.5. Other unverified datatype/table extensions do not inherit 2.5.1 semantics. | `msg_type_replaces_cm_only_in_later_editions`; `legacy_ack_keeps_edition_and_err_shape` |

The three profiles use the existing shared implementation and a conservative
common datatype/query prefix: at most fifteen XCN components, two family-name
subcomponents and nine QRF fields; query selector fields are capped at 60 bytes.
These are selected receiving-profile limits, not claims about every historical
edition's normative maxima. Extended filters are refused at their field, never
ignored to return a falsely trusted lookup. The 2.5 row adds only its sourced
control constraints to this common prefix; it does not copy the 2.5.1 profile.
These restrictions apply only to the base HL7 configuration. LAW uses its own
negotiated profile even when its compatible MSH-12 is `2.5` or another accepted
`2.5`-prefixed VID (TF-2b C.4); its known analysis time is still interpreted.
`compatible_law_versions_keep_analysis_time` guards that boundary.

### Content left raw or refused

- The errata's TS digit precision, offsets and retained degree component are
  accepted without rewriting. The formal fractional spelling supports up to four
  fractional digits; the errata's inconsistent digit-count description does not
  change the received spelling or supply missing precision. Tests:
  `legacy_timestamp_precision_is_checked` and
  `order_sequencing_component_model`.
- XCN extensions beyond the selected prefix, including the errata's problematic
  XCN-17, are refused by the QRD-8 view while the receipt retains the complete
  field. OBR-32–35 remain raw; they never become typed person names.
- Table changes and newly added datatypes are permitted by §2.8, but it does not
  date each code's availability. The three compatibility profiles therefore
  refuse the unverified UTF-8 declaration and URI universal-ID code. CWE result
  values remain original unsupported values and require parking. ASCII and
  Latin-1 remain the selected supported encodings. Tests:
  `utf8_code_is_not_backdated_to_legacy_editions`,
  `unsettled_query_extensions_and_uri_codes_refused`, and
  mapping `legacy_cwe_datatype_is_not_guessed`.
- OBX-19 remains raw instead of acquiring a later analysis-time interpretation.
  SPM/SAC group extensions and OUL message families are refused rather than
  changing legacy result ownership. Tests in mapping `compatibility.rs` check
  stable numeric values, leading-zero sample bytes, opaque interpreter fields,
  raw time extensions and visible rejection of unsettled specimen groups.
- Outbound machine-order commands remain unimplemented for these three profiles
  until writer integration. Config refuses `orders.hl7_send` for them; a valid
  receiving/query configuration cannot accidentally enable that writer.
  `legacy_queries_validate_but_unbuilt_writers_fail`
  guards this activation boundary.

`legacy_query_unavailable_is_not_empty` checks query
correlation and AE replies. The dialog regression
`every_edition_acks_only_after_store_and_interpret`
checks the store-before-ACK boundary for all three editions. Evidence remains
**documented**, with synthetic tests only.

HL7 2.5.1 section 2.8 (pp.2-20 onward) requires newer receivers to understand
older messages and older receivers to tolerate new information. Additions
and changes must preserve existing meanings: extra fields/components extend
the existing structure, and the earlier part of a changed datatype must keep
its earlier interpretation. Changes outside the stated rules cannot be
assumed compatible. Local extensions are a separate agreement (section 2.11).
This covers 2.3, 2.4 and 2.5 within the declared message scope; it does not
promise every transaction from those editions. The receiving profiles above
implement the declared common prefix; command writers remain separately gated.
Unexpected information must not by itself cause application failure under
section 2.8.1; Analyte retains original bytes even when it cannot use a constituent.
Unsupported message semantics, invalid headers and unsupported configured
versions remain explicit failures. Compatibility never supplies missing
identity or clinical meaning. See the checklist for current evidence gaps.

## Implemented grammar rule owner

`crates/protocols/hl7/src/version/mod.rs` owns edition selection and the grammar difference
table. The refactor preserves existing acceptance, including the shared
timestamp precision forms and the explicit LAW and UTF-8 policies.

| Rule | Sources | Regression coverage |
| --- | --- | --- |
| Response trigger omission and MSG lengths | 2.3.1 section 2.24.1.9; 2.5.1 sections 2.15.9.9, 2.A.44 | control message type components |
| Required message time | 2.3.1 section 2.24.1.7; 2.5.1 section 2.15.9.7 | control requires valid message time |
| Timestamp precision (shared acceptance) | 2.3.1 section 2.8.44; 2.5.1 sections 2.A.22, 2.A.77 | query keeps timestamp precision and offsets |
| Coded component lengths | 2.3.1 section 2.8.3; 2.5.1 sections 2.A.6, 2.A.13 | datatype coded component lengths; LAW CWE cases |
| URI universal ID code | 2.3.1 section 2.8.20; 2.5.1 section 2.A.33, table 0301 | query URI code is edition specific |
| XCN component count and lengths | 2.3.1 section 2.8.49; 2.5.1 section 2.A.86 | query XCN extended components and lengths |
| Family name count, required surname and lengths | 2.3.1 section 2.8.49.2; 2.5.1 section 2.A.30 | query XCN extended components and lengths |
| Name type codes | 2.3.1 section 2.8.49.10; 2.5.1 section 2.A.86.10, table 0200 | query name codes |
| Order sequence required components and lengths | 2.3.1 section 4.4; 2.5.1 section 2.A.50 | datatype order sequence required fields and lengths |
| Interval component lengths | 2.3.1 section 2.8.35; 2.5.1 section 2.A.63 | query TQ component constraints |
| Timing component lengths | 2.3.1 section 4.4; 2.5.1 section 2.A.76 | query TQ component constraints |

## Order writer qualification

2.6 and 2.8.2 are qualified for the bounded O21/O22 order profiles below. The historical
blockers remain accepted; 2.9.1 remains blocked on the source conflict below.
Evidence stays documented. Config reports edition-specific blockers.

| Edition | Sending | Specific remaining qualification |
| --- | --- | --- |
| 2.3 | Blocked | Original order-message and field definitions are unavailable. The later 2.5.1 section 2.8 receiving contract cannot establish an earlier writer. |
| 2.4 | Blocked | Original order-message and field definitions are unavailable; the partial historical errata do not supply a complete writer contract. |
| 2.5 | Blocked | Original order-message and field definitions are unavailable; no patch-edition downgrade is justified by section 2.8. |
| 2.6 | Enabled | Bounded OML/O21 and ORL/O22, with explicit test/specimen/priority revisions; see the rule matrix below. |
| 2.8.2 | Enabled | Bounded O21/O22; EIP group identity, required patient identifier type, explicit coding revisions and withdrawn-field refusal. ORL_O41 is unselected. |
| 2.9.1 | Blocked | ORC-4 encoding is unresolved: the attribute table and 4.5.1.4 declare EI, but the component description uses subcomponent separators. Authoritative clarification is needed before writing a group authority. |

Evidence: protocol `writer_qualification_separate_from_receiving`
and config `config_reports_registry_writer_blocker` test the
refusal boundary only. They establish only the remaining blocked-edition refusal boundary. The 2.6
writer evidence below establishes bytes, decisions and timeout/restart behavior. Existing 2.3.1/2.5.1 writer
qualification is unchanged. Evidence level remains documented.

### 2.6 order writer field matrix

The shared `version/order.rs` row selects CWE test encoding, the conservative
250-byte code-field profile cap and 24-byte scalar DTM timing. These caps do not
claim that all normative fields have the same maximum. The existing EI API
keeps its stricter 22-byte complete identity limit even where 2.6 allows 427.
No order replay or retry path is added. ORM writing remains unavailable in 2.6;
OML uses TQ1 and SPM instead of writing legacy OBR-15/27 and ORC-7 content.

| Fields written / validated | Difference and bound | 2.6 source |
| --- | --- | --- |
| MSH-1/2, 3/4/5/6, 7, 9/10/11/12/18 | Same explicit separators and header authorities; MSH-7 DTM replaces TS; exact `2.6`, `OML^O21^OML_O21`; original acknowledgment mode; explicit charset | 2.14.9.1-12/.18; 4.4.6 |
| Optional PID-1/3/5 | Same SI, CX and XPN subset; caller supplies ID, authority and surname. Identifier-only input continues to omit the optional patient group; no name is invented | 3.4.2.1/.3/.5; 2.A.14/.88 |
| ORC-1/2/3/4/6 | NW/CA and F retain their meaning; EI 2/3 have expanded capacity, but the API retains the stricter 22-byte cap; group EI remains 22 | 4.5.1.1-6 |
| OBR-1/2/3/4 | SI and EI retained; OBR-4 changes CE to CWE with supplied revision in component 7; profile cap 250 (standard field capacity 705) | 4.5.3.1-4; 2.A.13.7 |
| TQ1-1/7/8/9 | One window only; 7/8 change TS to scalar DTM (24); priority stays CWE with explicit user-defined table 0485 revision | 4.5.4.1/.7-9; 2.A.22 |
| SPM-1/2/4 | SI, EIP and CWE retained; EIP profile cap 80; explicit specimen coding revision, never borrowed from the test or edition | 7.4.3.1-4; 2.A.13.7 |
| ORL header, MSA-1/2, ERR, PID and ORC order decisions | Exact edition/O22/ORL_O22; MSA-2 matches sent MSH-10; new reply MSH-10; AA alone never accepts an order; order-group PID required when decisions supplied; optional unsupported groups fail visibly | 4.4.7; 2.9; 2.14.5/.8/.9 |
| ORC-16, returned OBR-4, TQ1-7/8/9, SPM-2/4, SAC identities | Edition-aware CWE/DTM checks; stable order/specimen correlation and the existing bounded group ownership; no scheduling or clinical decisions | 4.5.1.16; 4.5.3.4; 4.5.4; 7.4.3; 13.4.3 |
| Withdrawn CE/TS and MSA-5 | Writer emits CWE/DTM and no MSA; refuses the unqualified legacy writer; response refuses nonempty MSA-5. Other backward-compatibility fields do not execute scheduling | 2.A.6/.77; 2.14.8.5 |

Tests in protocol `tests/order/editions.rs` name their source sections in a comment and assert exact bytes,
missing-revision refusal, reply outcomes and bad control ID / MSA-5 refusal.
Config `send_requires_explicit_table_revisions` checks the
activation boundary. Program `send_ahead_accepted_rejected_and_mixed_items` includes 2.6 in acceptance,
rejection, cancellation, timeout/disconnect and restart-during-Sending cases.

For 2.6 outbound tests, OBR-4 is written as seven components: the order's
code, the lab app's optional name, `mappings.test_system`, three empty
alternate-code positions and `orders.hl7_send.test_version`. The
`orders.hl7_send.priority_version` and existing `specimen_version` are separate,
explicit values. The strict explicit-revision profile includes user-defined
HL7 tables; an `HL7` prefix is not proof of table ownership. Config does not
supply revisions, names, timestamps or identities from defaults.

### 2.8.2 order writer differences from 2.5.1

The `LAB_282` row reuses the shared O21 writer, with the changes below. Fields
not listed as changed keep the bounded 2.6 subset and conservative operational
caps in the preceding matrix. Modern conformance lengths are receiver capacities,
not universal upper limits (2.5.5.3-4). Additional participation, authentication
and access-restriction groups remain unselected and fail visibly in replies.

| Written / validated fields | Rule | 2.8.2 source |
| --- | --- | --- |
| MSH fields written by the 2.5.1 writer | Exact `2.8.2`; MSH-7 scalar DTM; standard charset declarations and no optional truncation declaration on generated orders | 2.14.9; 2.A.22 |
| MSH-9 and ORL reply | OML/O21/OML_O21 with ORL/O22/ORL_O22; patient group required for decisions. Reject patient-optional ORL_O41 rather than substitute structures | 4.4.6; 4.4.7.1-2 |
| ORC-1/2/3/4/6 | Controls/response flag retained; ORC-4 changes EI to EIP: placer group and authority use subcomponents (`g1&LAB`) | 4.5.1.1-6; 2.A.26 |
| PID-1/3/5 | SI/CX/XPN retained, but supplied CX.5 type is now required; a patient with no supplied type is refused. Identifier-only input still omits the optional request PID; reply patient minima are checked | 3.4.2; 2.A.14.5; 2.A.90 |
| OBR-1/2/3/4 | SI/EI retained; test is CWE, with explicit named-system revision and 12-character coding-system name bound; modern extended tuples are not generated by this API | 4.5.3.1-4; 2.A.13 |
| TQ1-1/7/8/9 | One SI timing window; scalar DTM and explicit priority CWE revision | 4.5.4.1/.7-9; 2.A.22 |
| SPM-1/2/4 | SI/EIP/CWE retained; explicit specimen revision; no global specimen flattening | 7.4.3.1-4; 2.A.13 |
| Nonempty ORC-7; OBR-5/6/14/15/27; MSA-3/5/6 | Withdrawn: never generated, and refused in replies, including explicit null content | 4.5.1.7; 4.5.3; 2.14.8.3/.5/.6 |

Evidence: `v282_order_and_reply_qualification`
asserts exact bytes, EIP grouping, patient type, revisions, accepted/rejected
replies, wrong control ID, wrong reply structure and withdrawn-field refusal.
Config revision tests and program `send_ahead_accepted_rejected_and_mixed_items` include 2.8.2, including
timeout/disconnect and crash/restart without retransmission. Evidence remains
documented; no machine or simulator qualification is claimed.

### 2.9.1 writer stop: conflicting ORC-4 encoding description

The available September 2024 chapter 4 source changes ORC-4 back to EI in both
its attribute table (4.5.1, p.56) and field definition (4.5.1.4, p.61). Unlike
2.8.2's EIP, this names only a placer group identity. However, the component
description on p.61 separates that EI's parts using subcomponent delimiters.
Chapter 2A's EI datatype uses component delimiters for a top-level field. These
would produce different bytes for the group authority that `order::context`
actually writes. Analyte does not select one reading on the basis of an apparent
editorial error. No 2.9.1 correction resolving this was found in the supplied
errata directory (it contains only the historical 2.3.1/2.4 errata).

Other inspected differences do not resolve that conflict: 4.4.6 retains O21;
4.4.7.1 retains patient-required O22 while 4.4.7.2 selects the distinct O53
patient-optional response. Section 4.5.3 withdraws additional provider and
interpreter fields (16/28/32-35); 2A.2.14.5 still requires CX.5; 2A.2.22 retains
the special UTC-without-civil-offset meaning of negative-zero offset. These
facts are not a qualified 2.9.1 writer row. No writer or config activation is
enabled for 2.9.1. An authoritative correction or explicit source-precedence
resolution is needed for ORC-4 before qualification continues.

Config test `unresolved_group_encoding_refuses_sending` and
`config_reports_registry_writer_blocker` keep the refusal visible.

Modern OBX-4 accepts only legacy OG.1. Populated enhanced grouping components
are refused before identity mapping: this profile does not select the complete
group/sequence contract (2.8.2 2.A.50; 2.9.1 2A.2.50). Raw evidence remains retained.

For 2.9.1 the generic TCP receiver supports original mode and enhanced accept-only
exchanges. Enhanced application responses require a separate channel (2.8.3.2),
which is unsupported. Config cannot enable that capability; incoming requests
are durably retained and parked before any response or interpretation.

The generic result receiver does not negotiate the sequence-number protocol or
assemble continuation messages. Populated MSH-13, populated MSH-14 and DSC
are refused with specific reasons; exact receipts remain retained.


## Adding an edition

All edition-dependent decisions live in `crates/protocols/hl7/src/version/`.
`EditionRules` names the control, query, result and writer capabilities;
`rules.rs` holds the datatype constraints, and `order.rs` holds writer fields.
Every edition row supplies every capability explicitly. `Version::entry()` is
exhaustive, so a new variant cannot compile until its row is selected. A shared
parser, dialog or mapping asks for a rule rather than comparing editions.
The capability tests pin every current row independently of its consumers.

Adding an edition changes that folder, its tests, this document and config's
edition declaration/conversion and enablement tests. It does not require new
edition arms in ACK, query, result, dialog or mapping code. Fixed 2.5.1 bindings
remain for LAW and shared datatype helpers; those are profile selections, not
fallback decisions for an incoming edition.

ORU/OUL segment ordering belongs to the validated `ResultMessage` view in
`views/grammar/`, with named states for pending control, specimen and order
positions. Mapping consumes its selected family and cached header edition.
The state tests cover valid transitions and refusals under HL7 2.3.1 §7.2.1
and 2.5.1 §7.3.1/§7.3.7. Changing an edition's scope still needs source evidence;
this refactor raises no evidence level and enables no new edition or writer.
