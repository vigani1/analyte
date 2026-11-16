//! Selects order writer field rules independently of receiving compatibility.
use super::Version;

pub(crate) struct OrderRules {
    pub test_cwe: bool,
    pub withdrawn_msa: &'static [usize],
    pub code_bytes: usize,
    pub time_bytes: usize,
    pub revision_required: bool,
}

// HL7 2.3.1 sections 4.2.1-2, 4.3.1 and 4.4; CE/TS legacy fields.
const LEGACY: OrderRules = OrderRules {
    test_cwe: false,
    withdrawn_msa: &[],
    code_bytes: 200,
    time_bytes: 26,
    revision_required: false,
};
// HL7 2.5.1 sections 4.4.6-7, 4.5.1, 4.5.3-4; O21/O22, CE OBR-4.
const LAB: OrderRules = OrderRules {
    code_bytes: 250,
    ..LEGACY
};
// HL7 2.6 sections 4.4.6-7, 4.5.1, 4.5.3.4, 4.5.4.7-9;
// sections 2.A.13.7 and 2.A.22: CWE revisions and scalar DTM.
const LAB_26: OrderRules = OrderRules {
    test_cwe: true,
    // HL7 2.6 section 2.14.8.5.
    withdrawn_msa: &[5],
    time_bytes: 24,
    revision_required: true,
    ..LAB
};
impl Version {
    pub(crate) fn order_rules(self) -> Option<&'static OrderRules> {
        match self {
            Self::V231 => Some(&LEGACY),
            Self::V251 => Some(&LAB),
            Self::V26 => Some(&LAB_26),
            Self::V23 | Self::V24 | Self::V25 | Self::V282 | Self::V291 => None,
        }
    }
}
