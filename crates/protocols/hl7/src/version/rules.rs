//! Holds edition-specific datatype and result field constraints.
use super::ResultField;
use super::result::RESULT_FIELDS_26;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeFormat {
    Ts,
    Dtm,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodingVersions {
    LocalSystems,
    Explicit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZeroOffset {
    Known,
    UtcOnly,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AckPair {
    Independent,
    Paired,
}

#[derive(Clone, Copy)]
pub(crate) struct Rules {
    pub observation_authority: Option<usize>,
    pub observation_dependencies: &'static [(usize, usize)],
    pub zero_offset: ZeroOffset,
    pub ack_pair: AckPair,
    pub withdrawn_obr: &'static [usize],
    pub withdrawn_msa: &'static [usize],
    pub withdrawn_err: &'static [usize],
    pub coded_field_bytes: usize,
    // HL7 2.6 section 7.3.1; Analyte's selected core group scope (ADR 0007).
    pub core_groups_only: bool,
    pub result_fields: &'static [ResultField],
    // HL7 2.6 sections 2.14.9.7/.10, 2.14.8.2 and 2.A.22.
    pub time_format: TimeFormat,
    pub control_bytes: usize,
    // HL7 2.6 sections 2.A.6/.13, 2.A.20/.76/.86 and 5.10.4.2.9.
    pub coding_versions: CodingVersions,
    pub coded_components: usize,
    pub query_timing_bytes: Option<usize>,
    pub refused_deprecated: &'static [usize],
    // HL7 2.3.1 section 2.24.1.9; 2.5.1 section 2.15.9.9 and section 2.A.44 (MSG).
    pub optional_response_trigger: bool,
    pub message_lengths: Option<[usize; 3]>,
    // HL7 2.3.1 section 2.8.44; 2.5.1 section 2.A.22, section 2.A.77.
    // Existing shared acceptance, including hour precision, is unchanged.
    pub timestamp_precisions: &'static [usize],
    // HL7 2.3.1 section 2.8.3; 2.5.1 section 2.A.6, section 2.A.13.
    pub coded_lengths: &'static [usize],
    // HL7 2.3.1 section 2.8.20 table 0301; 2.5.1 section 2.A.33 table 0301.
    pub uri: bool,
    // HL7 2.3.1 section 2.8.49; 2.5.1 section 2.A.86 and section 2.A.30.
    pub xcn_components: usize,
    pub family_components: usize,
    pub family_lengths: &'static [usize],
    pub xcn_lengths: &'static [usize],
    pub name_types: &'static [&'static [u8]],
    // HL7 2.3.1 section 4.4 quantity/timing; 2.5.1 section 2.A.50, section 2.A.63, section 2.A.76.
    pub sequence_required: &'static [usize],
    pub sequence_lengths: &'static [usize],
    pub interval_lengths: &'static [usize],
    pub timing_lengths: &'static [usize],
}

pub(super) const GRAMMARS: [Rules; 2] = [
    Rules {
        observation_authority: None,
        observation_dependencies: &[],
        zero_offset: ZeroOffset::Known,
        ack_pair: AckPair::Independent,
        withdrawn_obr: &[],
        withdrawn_msa: &[],
        withdrawn_err: &[],
        coded_field_bytes: 705,
        core_groups_only: false,
        result_fields: &[],
        time_format: TimeFormat::Ts,
        control_bytes: 20,
        coding_versions: CodingVersions::LocalSystems,
        coded_components: 6,
        query_timing_bytes: Some(60),
        refused_deprecated: &[],
        optional_response_trigger: true,
        message_lengths: None,
        timestamp_precisions: &[4, 6, 8, 10, 12, 14],
        coded_lengths: &[],
        uri: false,
        xcn_components: 15,
        family_components: 2,
        family_lengths: &[],
        xcn_lengths: &[],
        name_types: &[b"A", b"D", b"L", b"M", b"C", b"B", b"P", b"S", b"T", b"U"],
        sequence_required: &[],
        sequence_lengths: &[],
        interval_lengths: &[],
        timing_lengths: &[],
    },
    Rules {
        observation_authority: None,
        observation_dependencies: &[],
        zero_offset: ZeroOffset::Known,
        ack_pair: AckPair::Independent,
        withdrawn_obr: &[],
        withdrawn_msa: &[],
        withdrawn_err: &[],
        coded_field_bytes: 705,
        core_groups_only: false,
        result_fields: &[],
        time_format: TimeFormat::Ts,
        control_bytes: 20,
        coding_versions: CodingVersions::LocalSystems,
        coded_components: 6,
        query_timing_bytes: Some(60),
        refused_deprecated: &[],
        optional_response_trigger: false,
        message_lengths: Some([3, 3, 7]),
        timestamp_precisions: &[4, 6, 8, 10, 12, 14],
        coded_lengths: &[20, 199, 20, 20, 199, 20, 10, 10, 199],
        uri: true,
        xcn_components: 23,
        family_components: 5,
        family_lengths: &[50, 20, 50, 20, 50],
        xcn_lengths: &[
            15, 194, 30, 30, 20, 20, 5, 4, 227, 1, 1, 3, 5, 227, 1, 483, 53, 1, 26, 26, 199, 705,
            705,
        ],
        name_types: &[
            b"A", b"B", b"C", b"D", b"I", b"L", b"M", b"N", b"P", b"R", b"S", b"T", b"U",
        ],
        sequence_required: &[1, 2, 4, 8, 10],
        sequence_lengths: &[1, 15, 6, 15, 6, 12, 3, 15, 6, 15, 6],
        interval_lengths: &[6, 199],
        timing_lengths: &[267, 206, 6, 26, 26, 6, 199, 200, 1, 110, 483, 4],
    },
];

// HL7 2.5.1 section 2.8.2: interpret the established shared prefix, not
// unverified later extensions. Section 2.A.44 explicitly dates MSG to 2.5.
pub(super) const COMPATIBILITY_25: Rules = Rules {
    optional_response_trigger: false,
    message_lengths: Some([3, 3, 7]),
    ..GRAMMARS[0]
};

// HL7 2.6 sections 2.14.9/.8, 2.A.6/.13/.22/.86 and 5.10.4.
// Reuse unchanged constraints; differing lengths and component meanings stay here.
pub(super) const GRAMMAR_26: Rules = Rules {
    core_groups_only: true,
    result_fields: RESULT_FIELDS_26,
    time_format: TimeFormat::Dtm,
    control_bytes: 199,
    coding_versions: CodingVersions::Explicit,
    coded_components: 9,
    query_timing_bytes: None,
    refused_deprecated: &[17],
    xcn_lengths: &[
        15, 194, 30, 30, 20, 20, 5, 4, 227, 1, 4, 3, 5, 227, 1, 705, 49, 1, 24, 24, 199, 705, 705,
    ],
    ..GRAMMARS[1]
};

// HL7 2.8.2 sections 2.A.13 (pp.24-32), 2.5.5.3-4 (pp.13-14),
// 2.14.5.1/.8.3/.5/.6 and 7.4.1.5/.6/.14/.15/.27.
// Composite bytes are an operational cap, not a normative HL7 composite maximum.
pub(super) const GRAMMAR_282: Rules = Rules {
    // Section 7.4.2.3 and chapter 2A CWE authority rules: selected named-primary
    // mapping contract; OID-only authority stays unresolved without a registry.
    observation_authority: Some(3),
    // Both editions section 7.4.2.12: effective range date requires a reference range.
    observation_dependencies: &[(12, 7)],
    coded_components: 22,
    coded_field_bytes: 16384,
    coded_lengths: &[
        16384, 16384, 12, 16384, 16384, 12, 16384, 16384, 16384, 16384, 16384, 12, 16384, 16384,
        16384, 24, 16384, 16384, 24, 16384, 16384, 24,
    ],
    withdrawn_obr: &[5, 6, 14, 15, 27],
    withdrawn_msa: &[3, 5, 6],
    withdrawn_err: &[1],
    ..GRAMMAR_26
};

// HL7 2.9.1 sections 2A.2.13 (pp.25-33), 2A.2.22 (pp.39-40),
// 2.13.9.15-16 (pp.71-72), 7.4.1.16/.28/.32-.35.
pub(super) const GRAMMAR_291: Rules = Rules {
    zero_offset: ZeroOffset::UtcOnly,
    ack_pair: AckPair::Paired,
    withdrawn_obr: &[5, 6, 14, 15, 16, 27, 28, 32, 33, 34, 35],
    ..GRAMMAR_282
};
