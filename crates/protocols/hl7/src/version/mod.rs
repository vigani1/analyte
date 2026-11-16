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

struct EditionRules {
    edition: Version,
    wire: &'static str,
    truncation: bool,
    required_timestamp: bool,
    // None refuses typed interpretation until the edition's field rules exist.
    grammar: Option<&'static Rules>,
}

const EDITIONS: [EditionRules; 8] = [
    // HL7 2.5.1 sections 2.8.1-2, pp.2-20-22: receive the established earlier prefix.
    EditionRules {
        edition: Version::V23,
        wire: "2.3",
        truncation: false,
        required_timestamp: false,
        grammar: Some(&GRAMMARS[0]),
    },
    // HL7 2.3.1 sections 2.24.1.2/.7/.12, pp.2-94-95, 2-107.
    EditionRules {
        edition: Version::V231,
        wire: "2.3.1",
        truncation: false,
        required_timestamp: false,
        grammar: Some(&GRAMMARS[0]),
    },
    // HL7 2.5.1 sections 2.8, 2.15.9.7: MSH-7 became required in 2.4.
    EditionRules {
        edition: Version::V24,
        wire: "2.4",
        truncation: false,
        required_timestamp: true,
        grammar: Some(&GRAMMARS[0]),
    },
    // HL7 2.5.1 sections 2.8, 2.A.44 and 2.15.5.1: common prefix plus 2.5 control changes.
    EditionRules {
        edition: Version::V25,
        wire: "2.5",
        truncation: false,
        required_timestamp: true,
        grammar: Some(&COMPATIBILITY_25),
    },
    // HL7 2.5.1 sections 2.15.9.2/.7/.12, pp.2-74-77.
    EditionRules {
        edition: Version::V251,
        wire: "2.5.1",
        truncation: false,
        required_timestamp: true,
        grammar: Some(&GRAMMARS[1]),
    },
    // HL7 2.6 sections 2.14.9.2/.7/.12, pp.56-58.
    EditionRules {
        edition: Version::V26,
        wire: "2.6",
        truncation: false,
        required_timestamp: true,
        grammar: None,
    },
    // HL7 2.8.2 sections 2.14.9.2/.7/.12, pp.59-61.
    EditionRules {
        edition: Version::V282,
        wire: "2.8.2",
        truncation: true,
        required_timestamp: true,
        grammar: None,
    },
    // HL7 2.9.1 sections 2.13.9.2/.7/.12, pp.69-71.
    EditionRules {
        edition: Version::V291,
        wire: "2.9.1",
        truncation: true,
        required_timestamp: true,
        grammar: None,
    },
];

impl Version {
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

#[derive(Clone, Copy)]
pub(crate) struct Rules {
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

const GRAMMARS: [Rules; 2] = [
    Rules {
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
const COMPATIBILITY_25: Rules = Rules {
    optional_response_trigger: false,
    message_lengths: Some([3, 3, 7]),
    ..GRAMMARS[0]
};

#[cfg(test)]
mod tests;
