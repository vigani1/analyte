# HL7 versions

Only **2.3.1 and 2.5.1** have implemented typed field profiles. MLLP Release 1 is the framing
profile. Evidence remains documented. Documents for 2.6, 2.8.2 and 2.9.1
are available and planned; they are not implemented versions.

The lossless syntax layer now also reads 2.8.2 and 2.9.1 envelopes to retain
their declared truncation character and marker evidence. This is grammar
groundwork, not typed edition support: `Version` recognizes all eight requested
editions, but typed message views still refuse the six additional editions with
an edition-specific `EditionRules` error. No earlier edition is
silently upgraded. Truncated input is refused before result mapping, while
its exact bytes and the field/component marker flags remain available.

| Grammar rule | Source | Test |
| --- | --- | --- |
| Optional fifth MSH-2 character; distinct separators; earlier editions refuse it | 2.8.2 §2.14.9.2, p.59; 2.9.1 §2.13.9.2, p.69 | `hl7_2_8_2_s2_14_9_2_and_2_9_1_s2_13_9_2_encoding_characters` |
| Preserve sender-marked primitives and refuse clinical mapping | 2.8.2 §2.5.5.2, pp.12–13; 2.9.1 §2.4.5.2, pp.14–15 | `hl7_2_8_2_s2_5_5_2_and_2_9_1_s2_4_5_2_preserve_and_refuse_marked_values`; mapping regression `hl7_2_9_1_s2_4_5_2_truncated_input_never_reaches_result_mapping` |
| Literal truncation-character escape; bounded encoding without shortening | 2.8.2 §§2.7.1–2, pp.20–21; 2.9.1 §§2.6.1–2, pp.22–23 | `hl7_2_9_1_s2_6_1_truncation_character_escape` |
| Undeclared `\P\` retains the existing reject/preserve policy | Same escape sections | `hl7_2_8_2_s2_7_2_undeclared_p_escape_remains_unsupported` |

### Edition registry and implementation stages

The single registry in `version/mod.rs` selects exact wire IDs, required MSH-7
usage and truncation-character availability. Each entry explicitly identifies
whether its typed field profile exists. Recognition never aliases a new edition
to 2.5.1: absent field profiles refuse interpretation and writing while syntax
retains the original bytes. Configuration parsing recognizes `v23`, `v231`,
`v24`, `v25`, `v251`, `v26`, `v282` and `v291`, which map to the exact wire IDs
below. Validation accepts only **`v231` and `v251`**, whose typed profiles exist.
The other six return `UnsupportedHl7Edition` before a stack can be activated;
the diagnostic names the requested edition and the currently supported editions.
Unlisted values fail at the positioned TOML schema boundary with the bad value
and the recognized choices. A missing edition is not defaulted.
Compatibility interpretation, message/datatype differences and writer
integration are separate remaining steps. Enabling another edition in config
must accompany its implemented typed support and the corresponding tests.

The registry tests retain exact wire identity for all eight editions. Config
tests in `hl7_editions.rs` cover the two supported selections and canonical
snapshots (`adr0012_s3_typed_supported_config_editions_preserve_exact_wire_identity`),
refusal and actionable diagnostics for the other six
(`adr0012_s3_recognized_editions_without_typed_support_fail_config_validation`),
unlisted values and their diagnostics
(`adr0012_s3_unlisted_config_editions_name_bad_value_and_allowed_values`) and
missing declarations (`adr0012_s3_missing_config_edition_is_not_defaulted`).

| Registry entry | MSH-7 | Fifth MSH-2 character | Source | Source-named test in `version/tests.rs` |
| --- | --- | --- | --- | --- |
| 2.3 | Optional | Refused | 2.5.1 §§2.8, 2.15.9.7, pp.2-20–22, 2-76 | `hl7_2_5_1_s2_8_recognizes_2_3_without_upgrading` |
| 2.3.1 | Optional | Refused | 2.3.1 §§2.24.1.2/.7/.12, pp.2-92–94 | `hl7_2_3_1_s2_24_1_2_7_12_recognizes_edition` |
| 2.4 | Required | Refused | 2.5.1 §§2.8, 2.15.9.7, pp.2-20–22, 2-76 | `hl7_2_5_1_s2_8_and_s2_15_9_7_recognizes_2_4` |
| 2.5 | Required | Refused | Same compatibility and MSH-7 clauses | `hl7_2_5_1_s2_8_recognizes_2_5_without_patch_upgrade` |
| 2.5.1 | Required | Refused | 2.5.1 §§2.15.9.2/.7/.12, pp.2-74–77 | `hl7_2_5_1_s2_15_9_2_7_12_recognizes_edition` |
| 2.6 | Required | Refused | 2.6 §§2.14.9.2/.7/.12, pp.56–58 | `hl7_2_6_s2_14_9_2_7_12_recognizes_edition_without_typed_fallback` |
| 2.8.2 | Required | Accepted | 2.8.2 §§2.14.9.2/.7/.12, pp.59–61 | `hl7_2_8_2_s2_14_9_2_7_12_recognizes_edition_without_typed_fallback` |
| 2.9.1 | Required | Accepted | 2.9.1 §§2.13.9.2/.7/.12, pp.69–71 | `hl7_2_9_1_s2_13_9_2_7_12_recognizes_edition_without_typed_fallback` |

`adr0012_s3_no_unlisted_edition_or_prefix_match` checks unlisted versions,
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
| 2.3, 2.4, 2.5 | Earlier messages fall under the compatibility contract, not separate implemented edition profiles | HL7 2.5.1 section 2.8-2.8.2, pp.2-20-2-21 |
| 2.6, 2.8.2, 2.9.1 | Planned; differences have not been implemented or qualified | [Plan](../../plan.md) scope and ADR 0012 section 3 |

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

This review does not enable config by itself. Each edition remains refused until
its implemented typed receiving path, replies and config tests agree.

HL7 2.5.1 section 2.8 (pp.2-20 onward) requires newer receivers to understand
older messages and older receivers to tolerate new information. Additions
and changes must preserve existing meanings: extra fields/components extend
the existing structure, and the earlier part of a changed datatype must keep
its earlier interpretation. Changes outside the stated rules cannot be
assumed compatible. Local extensions are a separate agreement (section 2.11).
This covers 2.3, 2.4 and 2.5 within the declared message scope; it does not
promise every transaction from those editions. Registry variants now exist;
concrete typed compatibility remains a separate implementation step.
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
