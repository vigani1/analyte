//! Edition selection and the edition-dependent grammar rules. No implied upgrades.
/// Edition selected exclusively from MSH-12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// HL7 2.3.1.
    V231,
    /// HL7 2.5.1.
    V251,
}

impl Version {
    /// Selects only an explicitly supported MSH-12 edition.
    #[must_use]
    pub fn parse(raw: &[u8]) -> Option<Self> {
        match raw {
            b"2.3.1" => Some(Self::V231),
            b"2.5.1" => Some(Self::V251),
            _ => None,
        }
    }
    pub(crate) fn parse_wire(raw: &[u8], law: bool) -> Option<Self> {
        Self::parse(raw).or_else(|| (law && raw.starts_with(b"2.5")).then_some(Self::V251))
    }
    pub(crate) fn rules(self) -> &'static Rules {
        match self {
            Self::V231 => &RULES[0],
            Self::V251 => &RULES[1],
        }
    }
}

pub(crate) struct Rules {
    // HL7 2.3.1 section 2.24.1.9; 2.5.1 section 2.15.9.9 and section 2.A.44 (MSG).
    pub optional_response_trigger: bool,
    pub message_lengths: Option<[usize; 3]>,
    // HL7 2.3.1 section 2.24.1.7; 2.5.1 section 2.15.9.7.
    pub required_timestamp: bool,
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
    // HL7 2.3.1 section 4.3 quantity/timing; 2.5.1 section 2.A.50, section 2.A.63, section 2.A.76.
    pub sequence_required: &'static [usize],
    pub sequence_lengths: &'static [usize],
    pub interval_lengths: &'static [usize],
    pub timing_lengths: &'static [usize],
}

const RULES: [Rules; 2] = [
    Rules {
        optional_response_trigger: true,
        message_lengths: None,
        required_timestamp: false,
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
        optional_response_trigger: false,
        message_lengths: Some([3, 3, 7]),
        required_timestamp: true,
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
