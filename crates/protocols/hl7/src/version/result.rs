//! Lists edition-specific rules for fields interpreted by the bounded result profile.
pub(crate) enum FieldKind {
    Dtm,
    Cwe,
    ValueType,
    Value,
    Refuse,
}

pub(crate) struct ResultField {
    pub segment: &'static [u8],
    pub field: usize,
    pub kind: FieldKind,
}

// HL7 2.6 chapter 7 sections 7.4.1.7/.8/.14/.22: DTM replaces TS.
// Only interpreted fields are qualified here; unselected fields remain in the receipt.
pub(crate) const RESULT_FIELDS_26: &[ResultField] = &[
    ResultField {
        segment: b"OBR",
        field: 7,
        kind: FieldKind::Dtm,
    },
    ResultField {
        segment: b"OBR",
        field: 8,
        kind: FieldKind::Dtm,
    },
    ResultField {
        segment: b"OBR",
        field: 14,
        kind: FieldKind::Dtm,
    },
    ResultField {
        segment: b"OBR",
        field: 22,
        kind: FieldKind::Dtm,
    },
    // Sections 7.4.2.2/.5; chapter 2A sections 2.A.6/.77 withdraw CE and TS.
    ResultField {
        segment: b"OBX",
        field: 2,
        kind: FieldKind::ValueType,
    },
    ResultField {
        segment: b"OBX",
        field: 5,
        kind: FieldKind::Value,
    },
    // Sections 7.4.2.3/.6: CWE replaces CE, including nine components and lengths.
    ResultField {
        segment: b"OBX",
        field: 3,
        kind: FieldKind::Cwe,
    },
    ResultField {
        segment: b"OBX",
        field: 6,
        kind: FieldKind::Cwe,
    },
    // Sections 7.4.2.12/.14/.19: DTM normal-range, observation and analysis times.
    ResultField {
        segment: b"OBX",
        field: 12,
        kind: FieldKind::Dtm,
    },
    ResultField {
        segment: b"OBX",
        field: 14,
        kind: FieldKind::Dtm,
    },
    ResultField {
        segment: b"OBX",
        field: 19,
        kind: FieldKind::Dtm,
    },
    // Section 7.4.2.22: nonempty mood changes intent; this profile carries event results.
    ResultField {
        segment: b"OBX",
        field: 22,
        kind: FieldKind::Refuse,
    },
];
