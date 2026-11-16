//! Selects order writer field rules independently of receiving compatibility.
use super::Version;

pub(crate) struct OrderRules {
    pub test_cwe: bool,
    pub group_eip: bool,
    pub patient_type_required: bool,
    pub withdrawn_orc: &'static [usize],
    pub withdrawn_obr: &'static [usize],
    pub withdrawn_msa: &'static [usize],
    pub code_bytes: usize,
    pub time_bytes: usize,
}

// HL7 2.3.1 sections 4.2.1-2, 4.3.1 and 4.4; CE/TS legacy fields.
const LEGACY: OrderRules = OrderRules {
    test_cwe: false,
    group_eip: false,
    patient_type_required: false,
    withdrawn_orc: &[],
    withdrawn_obr: &[],
    withdrawn_msa: &[],
    code_bytes: 200,
    time_bytes: 26,
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
    ..LAB
};
// HL7 2.8.2 sections 4.4.6/.7.1, 4.5.1.4/.7 and 4.5.3;
// sections 2.A.13/.14.5/.22; section 2.14.8.3/.5/.6.
const LAB_282: OrderRules = OrderRules {
    group_eip: true,
    patient_type_required: true,
    withdrawn_orc: &[7],
    withdrawn_obr: &[5, 6, 14, 15, 27],
    withdrawn_msa: &[3, 5, 6],
    ..LAB_26
};
impl Version {
    pub(crate) fn order_rules(self) -> Option<&'static OrderRules> {
        match self {
            Self::V231 => Some(&LEGACY),
            Self::V251 => Some(&LAB),
            Self::V26 => Some(&LAB_26),
            Self::V282 => Some(&LAB_282),
            Self::V23 | Self::V24 | Self::V25 | Self::V291 => None,
        }
    }
}
