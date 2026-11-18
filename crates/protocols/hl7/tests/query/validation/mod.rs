//! Tests HL7 QRD and QRF field validation.
#[macro_use]
mod assertions;
use analyte_hl7::{Error, Message, Options, query::Request};
mod fields;

fn fixture(
    version: &str,
    segment: &'static str,
    field: usize,
    value: &[u8],
) -> Result<Message, Error> {
    let mut qrd: Vec<Vec<u8>> = [
        b"20261115".as_slice(),
        b"D",
        b"D",
        b"query",
        b"",
        b"",
        b"5^LI",
        b"0001^^^^^^^LCL",
        b"ORD",
        b"LOCAL",
        b"",
        b"",
    ]
    .iter()
    .map(|v| v.to_vec())
    .collect();
    let mut qrf: Vec<Vec<u8>> = [
        b"LAB".as_slice(),
        b"",
        b"",
        b"",
        b"",
        b"",
        b"",
        b"",
        b"",
        b"",
    ]
    .iter()
    .map(|v| v.to_vec())
    .collect();
    let fields = if segment == "QRD" { &mut qrd } else { &mut qrf };
    while fields.len() < field {
        fields.push(Vec::new());
    }
    let slot = field
        .checked_sub(1)
        .and_then(|n| fields.get_mut(n))
        .ok_or(Error::Encoding)?;
    *slot = value.to_vec();
    let mut raw =
        format!("MSH|^~\\&|A|B|C|D|20261115||QRY^Q02^QRY_Q02|m1|P|{version}\rQRD").into_bytes();
    for item in qrd {
        raw.push(b'|');
        raw.extend_from_slice(&item);
    }
    raw.extend_from_slice(b"\rQRF");
    for item in qrf {
        raw.push(b'|');
        raw.extend_from_slice(&item);
    }
    raw.push(b'\r');
    Message::parse(&raw, Options::default())
}

// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(query_time_is_required, "QRD", 1, b"");
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(query_time_refuses_impossible_date, "QRD", 1, b"20260230");
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(query_time_refuses_partial_precision, "QRD", 1, b"20261");
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(
    query_time_refuses_fraction_without_seconds,
    "QRD",
    1,
    b"202611151200.1"
);
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(
    query_time_refuses_five_fraction_digits,
    "QRD",
    1,
    b"20261115120000.12345"
);
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(
    query_time_refuses_invalid_offset,
    "QRD",
    1,
    b"202611151200+2460"
);
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(query_time_refuses_hour_24, "QRD", 1, b"202611152400");
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(query_time_refuses_year_zero, "QRD", 1, b"0000");
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(
    query_time_refuses_unknown_precision_code,
    "QRD",
    1,
    b"20261115^X"
);
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(query_time_refuses_overstated_precision, "QRD", 1, b"2026^S");
// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
negative!(
    query_time_refuses_extra_components,
    "QRD",
    1,
    b"20261115^D^x"
);
// HL7 2.3.1 §2.24.4.2; HL7 2.5.1 §5.10.5.3.3
negative!(query_format_must_be_in_table, "QRD", 2, b"X");
// HL7 2.3.1 §2.24.4.3; HL7 2.5.1 §5.10.5.3.4
negative!(query_priority_must_be_in_table, "QRD", 3, b"X");
// HL7 2.3.1 §2.24.4.4; HL7 2.5.1 §5.10.5.3.5
negative!(query_id_must_be_scalar, "QRD", 4, b"tag^wrong");
// HL7 2.3.1 §2.24.4.5; HL7 2.5.1 §5.10.5.3.6
negative!(deferred_response_type_must_be_in_table, "QRD", 5, b"X");
// HL7 2.3.1 §2.24.4.6; HL7 2.5.1 §5.10.5.3.7
negative!(deferred_response_time_must_be_valid, "QRD", 6, b"tomorrow");
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_amount_must_be_numeric, "QRD", 7, b"five^LI");
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_amount_is_required, "QRD", 7, b"^LI");
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_unit_must_be_in_table, "QRD", 7, b"5^XX");
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(
    quantity_unit_refuses_extra_subcomponents,
    "QRD",
    7,
    b"5^LI&&&&&&x"
);
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_refuses_extra_components, "QRD", 7, b"5^LI^x");
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(subject_id_requires_authority, "QRD", 8, b"0001");
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(subject_refuses_empty_composite, "QRD", 8, b"^");
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_name_type_must_be_in_table,
    "QRD",
    8,
    b"^Name^^^^^^^^Z"
);
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_universal_id_needs_type,
    "QRD",
    8,
    b"0001^^^^^^^^&uid"
);
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_universal_id_type_must_be_in_table,
    "QRD",
    8,
    b"0001^^^^^^^^&uid&XYZ"
);
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_check_scheme_must_be_in_table,
    "QRD",
    8,
    b"^Name^^^^^^^^^^XYZ"
);
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_check_digit_needs_scheme,
    "QRD",
    8,
    b"^Name^^^^^^^^^1"
);
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_name_representation_must_be_in_table,
    "QRD",
    8,
    b"^Name^^^^^^^^^^^^^Z"
);
// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
negative!(
    subject_name_part_refuses_subcomponents,
    "QRD",
    8,
    b"^Name^Given&nested"
);
// HL7 2.3.1 §2.24.4.9; HL7 2.5.1 §5.10.5.3.10
negative!(
    what_subject_refuses_extra_components,
    "QRD",
    9,
    b"A^B^C^D^E^F^G"
);
// HL7 2.3.1 §2.24.4.9; HL7 2.5.1 §5.10.5.3.10
negative!(what_subject_refuses_subcomponents, "QRD", 9, b"A&B");
// HL7 2.3.1 §2.24.4.9; HL7 2.5.1 §5.10.5.3.10
negative!(what_subject_refuses_empty_composite, "QRD", 9, b"^");
// HL7 2.3.1 §2.24.4.10; HL7 2.5.1 §5.10.5.3.11
negative!(
    department_refuses_extra_components,
    "QRD",
    10,
    b"A^B^C^D^E^F^G"
);
// HL7 2.3.1 §2.24.4.11; HL7 2.5.1 §5.10.5.3.12
negative!(value_range_refuses_extra_components, "QRD", 11, b"a^b^c");
// HL7 2.3.1 §2.24.4.11; HL7 2.5.1 §5.10.5.3.12
negative!(value_range_refuses_subcomponents, "QRD", 11, b"a&b");
// HL7 2.3.1 §2.24.4.12; HL7 2.5.1 §5.10.5.3.13
negative!(query_level_must_be_in_table, "QRD", 12, b"X");
// HL7 2.3.1 §2.24.5.1; HL7 2.5.1 §5.10.5.4.1
negative!(where_subject_is_required, "QRF", 1, b"");
// HL7 2.3.1 §2.24.5.1; HL7 2.5.1 §5.10.5.4.1
negative!(where_subject_refuses_components, "QRF", 1, b"LAB^subsystem");
// HL7 2.3.1 §2.24.5.1; HL7 2.5.1 §5.10.5.4.1
negative!(where_subject_refuses_empty_repeat, "QRF", 1, b"LAB~");
// HL7 2.3.1 §2.24.5.2; HL7 2.5.1 §5.10.5.4.2
negative!(start_time_must_be_valid, "QRF", 2, b"202613");
// HL7 2.3.1 §2.24.5.3; HL7 2.5.1 §5.10.5.4.3
negative!(end_time_must_be_valid, "QRF", 3, b"20261131");
// HL7 2.3.1 §2.24.5.4; HL7 2.5.1 §5.10.5.4.4
negative!(user_qualifier_refuses_components, "QRF", 4, b"a^b");
// HL7 2.3.1 §2.24.5.5; HL7 2.5.1 §5.10.5.4.5
negative!(other_subject_refuses_subcomponents, "QRF", 5, b"a&b");
// HL7 2.3.1 §2.24.5.6; HL7 2.5.1 §5.10.5.4.6
negative!(date_qualifier_must_be_in_table, "QRF", 6, b"NEW");
// HL7 2.3.1 §2.24.5.7; HL7 2.5.1 §5.10.5.4.7
negative!(status_qualifier_must_be_in_table, "QRF", 7, b"NEW");
// HL7 2.3.1 §2.24.5.8; HL7 2.5.1 §5.10.5.4.8
negative!(selection_qualifier_must_be_in_table, "QRF", 8, b"NEW");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_refuses_extra_components, "QRF", 9, b"^^^^^^^^^^^^x");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_quantity_must_be_numeric, "QRF", 9, b"bad");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_quantity_refuses_extra_subcomponents, "QRF", 9, b"1&u&x");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(
    tq_interval_refuses_extra_subcomponents,
    "QRF",
    9,
    b"^LOCAL&1200&x"
);
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_duration_must_be_valid, "QRF", 9, b"^^Dno");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_start_must_be_valid_time, "QRF", 9, b"^^^20260230");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_end_must_be_valid_time, "QRF", 9, b"^^^^20260230");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_priority_refuses_subcomponents, "QRF", 9, b"^^^^^R&x");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(
    tq_condition_refuses_subcomponents,
    "QRF",
    9,
    b"^^^^^^condition&x"
);
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_text_refuses_subcomponents, "QRF", 9, b"^^^^^^^text&x");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_conjunction_must_be_in_table, "QRF", 9, b"^^^^^^^^Z");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_sequencing_refuses_unknown_flag, "QRF", 9, b"^^^^^^^^^Z");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(
    tq_occurrence_duration_refuses_extra_parts,
    "QRF",
    9,
    b"^^^^^^^^^^a&b&c&d&e&f&g"
);
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(
    tq_total_occurrences_must_be_numeric,
    "QRF",
    9,
    b"^^^^^^^^^^^many"
);

fn component_value(number: usize, value: &[u8]) -> Vec<u8> {
    let mut raw = vec![b'^'; number.saturating_sub(1)];
    raw.extend_from_slice(value);
    raw
}
