//! Selects explicit HL7 editions without treating recognition as semantic qualification.
use crate::Error;

/// Explicit HL7 edition, never inferred from familiar segment content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// HL7 2.3 under the 2.5.1 compatibility contract.
    V23,
    /// HL7 2.3.1.
    V231,
    /// HL7 2.4 under the 2.5.1 compatibility contract.
    V24,
    /// HL7 2.5 under the 2.5.1 compatibility contract.
    V25,
    /// HL7 2.5.1.
    V251,
    /// HL7 2.6.
    V26,
    /// HL7 2.8.2.
    V282,
    /// HL7 2.9.1.
    V291,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent edition capabilities form an explicit flat rule table"
)]
struct EditionRules {
    // HL7 2.5.1 §2.15.5: structured ERR and MSG.
    modern_error_layout: bool,
    // HL7 2.5.1 §2.A.44: ACK message structure component.
    ack_structure: bool,
    // HL7 2.6 §2.14.5: ERR severity F.
    fatal_error_severity: bool,
    // HL7 2.8.2 §7.4.2.4: OG observation identity.
    enhanced_result_identity: bool,
    // HL7 2.8.2 §7.4.1: report status timestamp qualification.
    report_status_time: bool,
    // HL7 2.8.2 table 0357: error code 104.
    unknown_key_error: bool,
    // HL7 2.9.1 §2.13.9.15–16: paired acknowledgment requests.
    paired_application_ack: bool,
    // HL7 2.5.1 §5.3: original query structure identifiers.
    legacy_query_structure: bool,
    // HL7 2.3.1 §4.2.1: ORM patient requirements.
    orm_patient_name_required: bool,
    // HL7 2.3.1 §4.2.1: qualified ORM writer.
    orm_writer: bool,
    // Named 2.3.1 UTF-8 receiving departure (ADR 0002).
    utf8_departure: bool,
    // HL7 2.3.1 §4.3.1: ORM position in the other placer field.
    placer_position: bool,
    // HL7 2.3.1 §4.4: legacy specimen coding has no explicit version.
    specimen_code_version: bool,
    // HL7 2.3.1 §3.3.2.3: bounded patient identifier.
    short_patient_identifier: bool,
    // HL7 2.3.1 §4.2.2: one ERR in an order response.
    single_order_error: bool,
    // HL7 2.5.1 §4.4.6: qualified OML writer editions.
    oml_writer: bool,
    // HL7 2.5.1 §2.A.13: selected CWE validation.
    legacy_cwe: bool,
    // HL7 2.5.1 §7.3.7: selected OUL R22 receiving profile.
    oul_reception: bool,
    // HL7 2.5.1 §7.4.2: analysis timestamp field.
    analysis_time: bool,
    // IHE LAW: HL7 2.5.1 profile binding.
    law_edition: bool,
    // HL7 2.3.1 §5.4 / 2.5.1 §5.5: QRD person and coded field limits.
    query_person_bytes: Option<usize>,
    // HL7 2.3.1 §5.4 / 2.5.1 §5.5: QRF trailing quantity field.
    query_filter_fields: Option<usize>,
    edition: Version,
    wire: &'static str,
    truncation: bool,
    required_timestamp: bool,
    legacy_query_withdrawal: Option<&'static str>,
    // None refuses typed interpretation until the edition's field rules exist.
    grammar: Option<&'static Rules>,
}

const EDITIONS: [EditionRules; 8] = [
    // HL7 2.5.1 sections 2.8.1-2, pp.2-20-22: receive the established earlier prefix.
    EditionRules {
        modern_error_layout: false,
        ack_structure: false,
        fatal_error_severity: false,
        enhanced_result_identity: false,
        report_status_time: false,
        unknown_key_error: false,
        paired_application_ack: false,
        legacy_query_structure: false,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: false,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: false,
        law_edition: false,
        query_person_bytes: Some(60),
        query_filter_fields: Some(9),
        edition: Version::V23,
        wire: "2.3",
        truncation: false,
        required_timestamp: false,
        legacy_query_withdrawal: None,
        grammar: Some(&GRAMMARS[0]),
    },
    // HL7 2.3.1 sections 2.24.1.2/.7/.12, pp.2-94-95, 2-107.
    EditionRules {
        modern_error_layout: false,
        ack_structure: false,
        fatal_error_severity: false,
        enhanced_result_identity: false,
        report_status_time: false,
        unknown_key_error: false,
        paired_application_ack: false,
        legacy_query_structure: false,
        orm_patient_name_required: true,
        orm_writer: true,
        utf8_departure: true,
        placer_position: true,
        specimen_code_version: false,
        short_patient_identifier: true,
        single_order_error: true,
        oml_writer: false,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: false,
        law_edition: false,
        query_person_bytes: Some(60),
        query_filter_fields: Some(9),
        edition: Version::V231,
        wire: "2.3.1",
        truncation: false,
        required_timestamp: false,
        legacy_query_withdrawal: None,
        grammar: Some(&GRAMMARS[0]),
    },
    // HL7 2.5.1 sections 2.8, 2.15.9.7: MSH-7 became required in 2.4.
    EditionRules {
        modern_error_layout: false,
        ack_structure: false,
        fatal_error_severity: false,
        enhanced_result_identity: false,
        report_status_time: false,
        unknown_key_error: false,
        paired_application_ack: false,
        legacy_query_structure: false,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: false,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: false,
        law_edition: false,
        query_person_bytes: Some(60),
        query_filter_fields: Some(9),
        edition: Version::V24,
        wire: "2.4",
        truncation: false,
        required_timestamp: true,
        legacy_query_withdrawal: None,
        grammar: Some(&GRAMMARS[0]),
    },
    // HL7 2.5.1 sections 2.8, 2.A.44 and 2.15.5.1: common prefix plus 2.5 control changes.
    EditionRules {
        modern_error_layout: true,
        ack_structure: true,
        fatal_error_severity: false,
        enhanced_result_identity: false,
        report_status_time: false,
        unknown_key_error: false,
        paired_application_ack: false,
        legacy_query_structure: true,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: false,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: false,
        law_edition: false,
        query_person_bytes: Some(60),
        query_filter_fields: Some(9),
        edition: Version::V25,
        wire: "2.5",
        truncation: false,
        required_timestamp: true,
        legacy_query_withdrawal: None,
        grammar: Some(&COMPATIBILITY_25),
    },
    // HL7 2.5.1 sections 2.15.9.2/.7/.12, pp.2-74-77.
    EditionRules {
        modern_error_layout: true,
        ack_structure: true,
        fatal_error_severity: false,
        enhanced_result_identity: false,
        report_status_time: false,
        unknown_key_error: false,
        paired_application_ack: false,
        legacy_query_structure: true,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: true,
        legacy_cwe: true,
        oul_reception: true,
        analysis_time: true,
        law_edition: true,
        query_person_bytes: Some(250),
        query_filter_fields: Some(10),
        edition: Version::V251,
        wire: "2.5.1",
        truncation: false,
        required_timestamp: true,
        legacy_query_withdrawal: None,
        grammar: Some(&GRAMMARS[1]),
    },
    // HL7 2.6 sections 2.14.9.2/.7/.12, pp.56-58;
    // section 5.10.2, pp.95-96 retains original queries for backward compatibility.
    EditionRules {
        modern_error_layout: true,
        ack_structure: true,
        fatal_error_severity: true,
        enhanced_result_identity: false,
        report_status_time: false,
        unknown_key_error: false,
        paired_application_ack: false,
        legacy_query_structure: true,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: true,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: true,
        law_edition: false,
        query_person_bytes: Some(250),
        query_filter_fields: Some(10),
        edition: Version::V26,
        wire: "2.6",
        truncation: false,
        required_timestamp: true,
        legacy_query_withdrawal: None,
        grammar: Some(&GRAMMAR_26),
    },
    // HL7 2.8.2 sections 2.14.9.2/.7/.12, pp.59-61.
    EditionRules {
        modern_error_layout: true,
        ack_structure: true,
        fatal_error_severity: true,
        enhanced_result_identity: true,
        report_status_time: true,
        unknown_key_error: true,
        paired_application_ack: false,
        legacy_query_structure: false,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: true,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: true,
        law_edition: false,
        query_person_bytes: None,
        query_filter_fields: None,
        edition: Version::V282,
        wire: "2.8.2",
        truncation: true,
        required_timestamp: true,
        // HL7 2.8.2 chapter 5 section 5.10.2: withdrawn since 2.7.
        legacy_query_withdrawal: Some("5.10.2"),
        grammar: Some(&GRAMMAR_282),
    },
    // HL7 2.9.1 sections 2.13.9.2/.7/.12, pp.69-71.
    EditionRules {
        modern_error_layout: true,
        ack_structure: true,
        fatal_error_severity: true,
        enhanced_result_identity: true,
        report_status_time: true,
        unknown_key_error: true,
        paired_application_ack: true,
        legacy_query_structure: false,
        orm_patient_name_required: false,
        orm_writer: false,
        utf8_departure: false,
        placer_position: false,
        specimen_code_version: true,
        short_patient_identifier: false,
        single_order_error: false,
        oml_writer: false,
        legacy_cwe: false,
        oul_reception: false,
        analysis_time: true,
        law_edition: false,
        query_person_bytes: None,
        query_filter_fields: None,
        edition: Version::V291,
        wire: "2.9.1",
        truncation: true,
        required_timestamp: true,
        // HL7 2.9.1 chapter 5 section 5.9.2: withdrawn since 2.7.
        legacy_query_withdrawal: Some("5.9.2"),
        grammar: Some(&GRAMMAR_291),
    },
];

impl Version {
    /// Named 2.3.1 UTF-8 receiving departure (ADR 0002).
    #[must_use]
    pub fn utf8_departure(self) -> bool {
        self.entry().utf8_departure
    }
    /// HL7 2.3.1 §4.3.1: ORM position in the other placer field.
    #[must_use]
    pub fn placer_position(self) -> bool {
        self.entry().placer_position
    }
    /// HL7 2.5.1 §2.A.44: ACK message structure component.
    #[must_use]
    pub fn ack_structure(self) -> bool {
        self.entry().ack_structure
    }
    /// HL7 2.3.1 §4.2.1: qualified ORM writer.
    #[must_use]
    pub fn orm_writer(self) -> bool {
        self.entry().orm_writer
    }
    /// HL7 2.3.1 §4.4: legacy specimen coding has no explicit version.
    #[must_use]
    pub fn specimen_code_version(self) -> bool {
        self.entry().specimen_code_version
    }
    /// HL7 2.3.1 §3.3.2.3: bounded patient identifier.
    #[must_use]
    pub fn short_patient_identifier(self) -> bool {
        self.entry().short_patient_identifier
    }
    /// HL7 2.3.1 §4.2.2: one ERR in an order response.
    #[must_use]
    pub fn single_order_error(self) -> bool {
        self.entry().single_order_error
    }
    /// HL7 2.8.2 §7.4.1: report status timestamp qualification.
    #[must_use]
    pub fn report_status_time(self) -> bool {
        self.entry().report_status_time
    }
    /// HL7 2.8.2 table 0357: error code 104.
    #[must_use]
    pub fn unknown_key_error(self) -> bool {
        self.entry().unknown_key_error
    }
    /// HL7 2.5.1 §2.15.5: structured ERR and MSG.
    #[must_use]
    pub fn modern_error_layout(self) -> bool {
        self.entry().modern_error_layout
    }
    /// HL7 2.6 §2.14.5: ERR severity F.
    #[must_use]
    pub fn fatal_error_severity(self) -> bool {
        self.entry().fatal_error_severity
    }
    /// HL7 2.8.2 §7.4.2.4: OG observation identity.
    #[must_use]
    pub fn enhanced_result_identity(self) -> bool {
        self.entry().enhanced_result_identity
    }
    /// HL7 2.9.1 §2.13.9.15–16: paired acknowledgment requests.
    #[must_use]
    pub fn paired_application_ack(self) -> bool {
        self.entry().paired_application_ack
    }
    /// HL7 2.5.1 §5.3: original query structure identifiers.
    #[must_use]
    pub fn legacy_query_structure(self) -> bool {
        self.entry().legacy_query_structure
    }
    /// HL7 2.3.1 §4.2.1: ORM patient requirements.
    #[must_use]
    pub fn orm_patient_name_required(self) -> bool {
        self.entry().orm_patient_name_required
    }
    /// HL7 2.5.1 §4.4.6: qualified OML writer editions.
    #[must_use]
    pub fn oml_writer(self) -> bool {
        self.entry().oml_writer
    }
    /// HL7 2.5.1 §2.A.13: selected CWE validation.
    #[must_use]
    pub fn legacy_cwe(self) -> bool {
        self.entry().legacy_cwe
    }
    /// HL7 2.5.1 §7.3.7: selected OUL R22 receiving profile.
    #[must_use]
    pub fn oul_reception(self) -> bool {
        self.entry().oul_reception
    }
    /// HL7 2.5.1 §7.4.2: analysis timestamp field.
    #[must_use]
    pub fn analysis_time(self) -> bool {
        self.entry().analysis_time
    }
    /// IHE LAW: HL7 2.5.1 profile binding.
    #[must_use]
    pub fn law_edition(self) -> bool {
        self.entry().law_edition
    }
    /// Returns the qualified original-query limit, or refuses withdrawn queries.
    pub(crate) fn query_person_bytes(self) -> Result<usize, Error> {
        self.entry()
            .query_person_bytes
            .ok_or(Error::EditionRules(self.as_str()))
    }
    /// Returns the qualified original-query limit, or refuses withdrawn queries.
    pub(crate) fn query_filter_fields(self) -> Result<usize, Error> {
        self.entry()
            .query_filter_fields
            .ok_or(Error::EditionRules(self.as_str()))
    }

    /// Returns why order sending remains unqualified for this exact edition.
    /// Receiving compatibility never establishes permission to write older fields.
    #[must_use]
    pub fn order_sending_blocker(self) -> Option<&'static str> {
        match self {
            // HL7 2.5.1 section 2.8 specifies receiving compatibility, not
            // complete earlier-edition writer field definitions.
            Self::V23 => Some(
                "2.3 order writers are not yet implemented: original 2.3 order field definitions are unavailable; 2.5.1 section 2.8 only establishes receiving compatibility",
            ),
            Self::V24 => Some(
                "2.4 order writers are not yet implemented: original 2.4 order field definitions are unavailable; 2.5.1 section 2.8 only establishes receiving compatibility",
            ),
            Self::V25 => Some(
                "2.5 order writers are not yet implemented: original 2.5 order field definitions are unavailable; 2.5.1 section 2.8 only establishes receiving compatibility",
            ),
            Self::V231 | Self::V251 | Self::V26 | Self::V282 => None,
            // HL7 2.9.1 sections 4.4.7.1-2, 4.5.3.4 and 2A.2.13.7.
            Self::V291 => Some(
                "2.9.1 order writers are not yet implemented: section 4.5.1.4 and the ORC table declare ORC-4 as EI but its component description uses subcomponent separators; group encoding needs authoritative clarification",
            ),
        }
    }

    /// Recognizes exactly the eight declared editions, without a nearest-version fallback.
    #[must_use]
    pub fn parse(raw: &[u8]) -> Option<Self> {
        EDITIONS
            .iter()
            .find(|row| row.wire.as_bytes() == raw)
            .map(|row| row.edition)
    }

    /// Returns the exact MSH-12 edition spelling, including patch editions.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.entry().wire
    }

    fn entry(self) -> &'static EditionRules {
        match self {
            Self::V23 => &EDITIONS[0],
            Self::V231 => &EDITIONS[1],
            Self::V24 => &EDITIONS[2],
            Self::V25 => &EDITIONS[3],
            Self::V251 => &EDITIONS[4],
            Self::V26 => &EDITIONS[5],
            Self::V282 => &EDITIONS[6],
            Self::V291 => &EDITIONS[7],
        }
    }

    /// Identifies editions received under the 2.5.1 section 2.8 compatibility contract.
    /// Edition-specific additions without established meaning remain raw or refused.
    #[must_use]
    pub fn uses_compatibility_contract(self) -> bool {
        matches!(self, Self::V23 | Self::V24 | Self::V25)
    }

    /// Returns the section withdrawing original queries, independently of result qualification.
    /// LAW's separately selected QBP contract does not use this original-query capability.
    #[must_use]
    pub fn legacy_query_withdrawal(self) -> Option<&'static str> {
        self.entry().legacy_query_withdrawal
    }

    /// Reports whether specimen/container extensions are outside the selected receiving profile.
    #[must_use]
    pub fn uses_bounded_result_groups(self) -> bool {
        self == Self::V231
            || self.uses_compatibility_contract()
            || self.rules().is_some_and(|rules| rules.core_groups_only)
    }

    /// Reports whether this qualified receiving profile uses scalar DTM values.
    #[must_use]
    pub fn uses_dtm(self) -> bool {
        self.rules()
            .is_some_and(|rules| rules.time_format == TimeFormat::Dtm)
    }

    /// Reports whether this edition has a qualified bounded result profile.
    #[must_use]
    pub fn supports_result_reception(self) -> bool {
        self.rules().is_some()
    }

    /// Reports UTC-without-civil-offset spelling that must remain raw in the common model.
    #[must_use]
    pub fn keeps_utc_without_civil_offset_raw(self) -> bool {
        self.rules()
            .is_some_and(|rules| rules.zero_offset == ZeroOffset::UtcOnly)
    }

    // Table 0211 can change under section 2.8.2; do not assume that a later
    // character-set code was already defined in an unavailable earlier edition.
    pub(crate) fn standard_utf8(raw: &[u8]) -> bool {
        matches!(
            Self::parse(raw),
            Some(Self::V251 | Self::V26 | Self::V282 | Self::V291)
        )
    }

    pub(crate) fn defines_truncation(raw: &[u8]) -> bool {
        Self::parse(raw).is_some_and(|edition| edition.entry().truncation)
    }

    pub(crate) fn required_timestamp(self) -> bool {
        self.entry().required_timestamp
    }

    pub(crate) fn parse_wire(raw: &[u8], law: bool) -> Option<Self> {
        Self::parse(raw).or_else(|| (law && raw.starts_with(b"2.5")).then_some(Self::V251))
    }

    pub(crate) fn rules(self) -> Option<&'static Rules> {
        self.entry().grammar
    }

    pub(crate) fn require_rules(self) -> Result<&'static Rules, Error> {
        self.rules().ok_or(Error::EditionRules(self.as_str()))
    }
}

mod rules;
pub(crate) use rules::*;

mod result;
pub(crate) use result::{FieldKind, ResultField};

#[cfg(test)]
mod tests;

mod order;
