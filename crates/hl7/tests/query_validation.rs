//! Synthetic negative fixtures for HL7 2.3.1 §§2.24.4–5 / 2.5.1 §§5.10.5.3–4.
use analyte_hl7::{Error, Message, Options, query::Request};

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

macro_rules! negative {
    ($name:ident,$segment:literal,$field:literal,$value:expr) => {
        #[test]
        fn $name() -> Result<(), Error> {
            for edition in ["2.3.1", "2.5.1"] {
                let message = fixture(edition, $segment, $field, $value)?;
                assert!(
                    matches!(
                        Request::new(&message),
                        Err(Error::Control {
                            segment: $segment,
                            field: $field
                        })
                    ),
                    "wrong outcome for {}",
                    edition
                );
            }
            Ok(())
        }
    };
}

negative!(hl7_231_2_24_4_1_251_5_10_5_3_2_required_time, "QRD", 1, b"");
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_calendar,
    "QRD",
    1,
    b"20260230"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_precision,
    "QRD",
    1,
    b"20261"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_fraction,
    "QRD",
    1,
    b"202611151200.1"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_fraction_size,
    "QRD",
    1,
    b"20261115120000.12345"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_offset,
    "QRD",
    1,
    b"202611151200+2460"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_clock,
    "QRD",
    1,
    b"202611152400"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_zero_year,
    "QRD",
    1,
    b"0000"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_precision_code,
    "QRD",
    1,
    b"20261115^X"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_precision_overstatement,
    "QRD",
    1,
    b"2026^S"
);
negative!(
    hl7_231_2_24_4_1_251_5_10_5_3_2_time_components,
    "QRD",
    1,
    b"20261115^D^x"
);
negative!(hl7_231_2_24_4_2_251_5_10_5_3_3_format_table, "QRD", 2, b"X");
negative!(
    hl7_231_2_24_4_3_251_5_10_5_3_4_priority_table,
    "QRD",
    3,
    b"X"
);
negative!(
    hl7_231_2_24_4_4_251_5_10_5_3_5_id_structure,
    "QRD",
    4,
    b"tag^wrong"
);
negative!(
    hl7_231_2_24_4_5_251_5_10_5_3_6_deferred_table,
    "QRD",
    5,
    b"X"
);
negative!(
    hl7_231_2_24_4_6_251_5_10_5_3_7_deferred_time,
    "QRD",
    6,
    b"tomorrow"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_quantity_nm,
    "QRD",
    7,
    b"five^LI"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_quantity_missing_amount,
    "QRD",
    7,
    b"^LI"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_quantity_unit_table,
    "QRD",
    7,
    b"5^XX"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_quantity_unit_structure,
    "QRD",
    7,
    b"5^LI&&&&&&x"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_quantity_components,
    "QRD",
    7,
    b"5^LI^x"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_authority,
    "QRD",
    8,
    b"0001"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_empty_composite,
    "QRD",
    8,
    b"^"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_name_table,
    "QRD",
    8,
    b"^Name^^^^^^^^Z"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_hd_pair,
    "QRD",
    8,
    b"0001^^^^^^^^&uid"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_hd_table,
    "QRD",
    8,
    b"0001^^^^^^^^&uid&XYZ"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_check_scheme,
    "QRD",
    8,
    b"^Name^^^^^^^^^^XYZ"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_missing_scheme,
    "QRD",
    8,
    b"^Name^^^^^^^^^1"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_representation,
    "QRD",
    8,
    b"^Name^^^^^^^^^^^^^Z"
);
negative!(
    hl7_231_2_24_4_8_251_5_10_5_3_9_subject_scalar,
    "QRD",
    8,
    b"^Name^Given&nested"
);
negative!(
    hl7_231_2_24_4_9_251_5_10_5_3_10_what_components,
    "QRD",
    9,
    b"A^B^C^D^E^F^G"
);
negative!(
    hl7_231_2_24_4_9_251_5_10_5_3_10_what_scalar,
    "QRD",
    9,
    b"A&B"
);
negative!(
    hl7_231_2_24_4_9_251_5_10_5_3_10_what_empty_composite,
    "QRD",
    9,
    b"^"
);
negative!(
    hl7_231_2_24_4_10_251_5_10_5_3_11_department_components,
    "QRD",
    10,
    b"A^B^C^D^E^F^G"
);
negative!(
    hl7_231_2_24_4_11_251_5_10_5_3_12_range_components,
    "QRD",
    11,
    b"a^b^c"
);
negative!(
    hl7_231_2_24_4_11_251_5_10_5_3_12_range_scalar,
    "QRD",
    11,
    b"a&b"
);
negative!(
    hl7_231_2_24_4_12_251_5_10_5_3_13_level_table,
    "QRD",
    12,
    b"X"
);
negative!(
    hl7_231_2_24_5_1_251_5_10_5_4_1_where_required,
    "QRF",
    1,
    b""
);
negative!(
    hl7_231_2_24_5_1_251_5_10_5_4_1_where_scalar,
    "QRF",
    1,
    b"LAB^subsystem"
);
negative!(
    hl7_231_2_24_5_1_251_5_10_5_4_1_where_empty_repeat,
    "QRF",
    1,
    b"LAB~"
);
negative!(
    hl7_231_2_24_5_2_251_5_10_5_4_2_start_timestamp,
    "QRF",
    2,
    b"202613"
);
negative!(
    hl7_231_2_24_5_3_251_5_10_5_4_3_end_timestamp,
    "QRF",
    3,
    b"20261131"
);
negative!(
    hl7_231_2_24_5_4_251_5_10_5_4_4_user_scalar,
    "QRF",
    4,
    b"a^b"
);
negative!(
    hl7_231_2_24_5_5_251_5_10_5_4_5_other_scalar,
    "QRF",
    5,
    b"a&b"
);
negative!(
    hl7_231_2_24_5_6_251_5_10_5_4_6_date_qualifier_table,
    "QRF",
    6,
    b"NEW"
);
negative!(
    hl7_231_2_24_5_7_251_5_10_5_4_7_status_qualifier_table,
    "QRF",
    7,
    b"NEW"
);
negative!(
    hl7_231_2_24_5_8_251_5_10_5_4_8_selection_table,
    "QRF",
    8,
    b"NEW"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_components,
    "QRF",
    9,
    b"^^^^^^^^^^^^x"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_quantity,
    "QRF",
    9,
    b"bad"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_quantity_components,
    "QRF",
    9,
    b"1&u&x"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_interval,
    "QRF",
    9,
    b"^LOCAL&1200&x"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_duration,
    "QRF",
    9,
    b"^^Dno"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_start,
    "QRF",
    9,
    b"^^^20260230"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_end,
    "QRF",
    9,
    b"^^^^20260230"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_priority_scalar,
    "QRF",
    9,
    b"^^^^^R&x"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_condition_scalar,
    "QRF",
    9,
    b"^^^^^^condition&x"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_text_scalar,
    "QRF",
    9,
    b"^^^^^^^text&x"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_conjunction_table,
    "QRF",
    9,
    b"^^^^^^^^Z"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_sequence_flag,
    "QRF",
    9,
    b"^^^^^^^^^Z"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_occurrence_duration,
    "QRF",
    9,
    b"^^^^^^^^^^a&b&c&d&e&f&g"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_total_nm,
    "QRF",
    9,
    b"^^^^^^^^^^^many"
);

#[test]
fn hl7_231_2_24_4_5_and_251_5_10_5_3_4_field_usage_lengths_and_repetitions() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for (segment, last) in [("QRD", 12), ("QRF", 10)] {
            for field in 1..=last {
                if segment == "QRF" && field == 10 && version == "2.3.1" {
                    continue;
                }
                for value in [b"\"\"".to_vec(), vec![b'x'; 251]] {
                    let message = fixture(version, segment, field, &value)?;
                    assert!(
                        matches!(Request::new(&message),Err(Error::Control {segment:s,field:f}) if s==segment && f==field)
                    );
                }
            }
        }
        for field in [1, 2, 3, 4, 7, 8, 9, 10] {
            let message = fixture(version, "QRD", field, b"")?;
            assert!(
                matches!(Request::new(&message),Err(Error::Control {segment:"QRD",field:f}) if f==field)
            );
        }
        for (segment, fields) in [
            ("QRD", &[1, 2, 3, 4, 5, 6, 7, 12][..]),
            ("QRF", &[2, 3, 9, 10][..]),
        ] {
            for field in fields {
                let message = fixture(version, segment, *field, b"1~2")?;
                assert!(
                    matches!(Request::new(&message),Err(Error::Control {segment:s,field:f}) if s==segment && f==*field)
                );
            }
        }
        for (segment, field) in [("QRD", 13), ("QRF", 11)] {
            let message = fixture(version, segment, field, b"unknown")?;
            assert!(
                matches!(Request::new(&message),Err(Error::Control {segment:s,field:f}) if s==segment && f==field)
            );
        }
    }
    Ok(())
}

#[test]
fn hl7_251_5_10_5_4_10_threshold_nm_and_231_exclusion() -> Result<(), Error> {
    for (version, value) in [("2.3.1", b"0.5".as_slice()), ("2.5.1", b"not-number")] {
        let message = fixture(version, "QRF", 10, value)?;
        assert!(matches!(
            Request::new(&message),
            Err(Error::Control {
                segment: "QRF",
                field: 10
            })
        ));
    }
    let message = fixture("2.5.1", "QRF", 10, b"+8.25")?;
    assert!(Request::new(&message).is_ok());
    Ok(())
}

#[test]
fn hl7_231_2_24_4_5_and_251_5_10_5_3_4_preserve_local_filters() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for (segment, field, value) in [
            ("QRD", 8, b"0001^^^^^^^LCL~^Synthetic".as_slice()),
            ("QRD", 9, b"ZZLOCAL^Local filter^L~ORD"),
            ("QRD", 10, b"ZTEST^Local test^L"),
            ("QRD", 11, b"a^z~A^Z"),
            ("QRF", 1, b"UNKNOWN~OTHER"),
            ("QRF", 4, b"0007~CaseSensitive"),
            ("QRF", 5, b"VendorLocal~Other"),
            ("QRF", 9, b"1^LOCAL^D2^20261115&L^^^^note"),
        ] {
            let message = fixture(version, segment, field, value)?;
            let request = Request::new(&message)?;
            let source = if segment == "QRD" {
                Some(request.definition())
            } else {
                request.filter()
            };
            assert_eq!(
                source
                    .and_then(|s| s.field(field))
                    .map(analyte_hl7::Field::raw),
                Some(value)
            );
            assert_eq!(request.message().raw(), message.raw());
        }
    }
    Ok(())
}

proptest::proptest! {
    #[test]
    fn hl7_231_2_24_5_1_251_5_10_5_4_1_unknown_where_is_preserved(value in "[a-zA-Z0-9]{1,20}") {
        for version in ["2.3.1","2.5.1"] {
            let message=fixture(version,"QRF",1,value.as_bytes());
            proptest::prop_assert!(message.is_ok());
            if let Ok(message)=message {
                let request=Request::new(&message);
                proptest::prop_assert!(request.is_ok());
                if let Ok(request)=request {
                    proptest::prop_assert_eq!(request.filter().and_then(|s|s.field(1)).map(analyte_hl7::Field::raw),Some(value.as_bytes()));
                }
            }
        }
    }
}

fn component_value(number: usize, value: &[u8]) -> Vec<u8> {
    let mut raw = vec![b'^'; number.saturating_sub(1)];
    raw.extend_from_slice(value);
    raw
}

#[test]
fn hl7_251_5_10_5_3_9_xcn_extended_components_and_lengths() -> Result<(), Error> {
    for (component, value) in [
        (1, b"1234567890123456".as_slice()),
        (2, b"&prefix"),
        (2, &[b'n'; 51]),
        (16, b"a&b&c&d&e&f&g"),
        (17, b"20260230"),
        (18, b"Z"),
        (19, b"20260230"),
        (20, b"20260230"),
        (22, b"CODE&Text&L"),
        (22, b"a&b&c&d&e&f&g&h&i&j"),
        (23, b"CODE&Text&L"),
        (24, b"extra"),
    ] {
        let mut value = component_value(component, value);
        // Use a name when testing the first ID length, so the namespace
        // requirement is independently satisfied before exercising its length.
        if component == 1 {
            value.extend_from_slice(b"^^^^^^^LCL");
        }
        let message = fixture("2.5.1", "QRD", 8, &value)?;
        assert!(
            matches!(
                Request::new(&message),
                Err(Error::Control {
                    segment: "QRD",
                    field: 8
                })
            ),
            "component {}",
            component
        );
    }
    for (component, value) in [
        (17, b"20261115&20261116".as_slice()),
        (18, b"G"),
        (19, b"20261115&D"),
        (22, b"LOCAL&Text&L&&&&v1"),
        (23, b"LOCAL&Text&L&&&&v1"),
    ] {
        let detail = component_value(component, value);
        // Add the subject without changing the optional component's position.
        let mut value = b"^N".to_vec();
        value.extend_from_slice(detail.get(1..).unwrap_or_default());
        let message = fixture("2.5.1", "QRD", 8, &value)?;
        assert!(Request::new(&message).is_ok(), "component {}", component);
    }
    let message = fixture("2.3.1", "QRD", 8, &component_value(16, b"context"))?;
    assert!(matches!(
        Request::new(&message),
        Err(Error::Control {
            segment: "QRD",
            field: 8
        })
    ));
    Ok(())
}

#[test]
fn hl7_251_5_10_5_4_9_tq_component_constraints() -> Result<(), Error> {
    for (component, value) in [
        (2, b"123456789012345678901".as_slice()),
        (3, b"INDEFX"),
        (6, b"1234567"),
        (10, b"S"),
        (10, b"S&p&&f&&XX+H1&&u&&v"),
        (10, b"S&p&&f&&SS+H1&bad&u&&v"),
        (10, b"S&p&&f&&SS+H1&&u&XYZ&v"),
        (10, b"S&p&&f&&SS+INDEF&&u&&v"),
        (11, b"123456789012345678901"),
        (12, b"12345"),
    ] {
        let message = fixture("2.5.1", "QRF", 9, &component_value(component, value))?;
        assert!(
            matches!(
                Request::new(&message),
                Err(Error::Control {
                    segment: "QRF",
                    field: 9
                })
            ),
            "component {}",
            component
        );
    }
    let message = fixture("2.5.1", "QRF", 9, b"^^^^^^^^^S&p&&f&&SS+H1&&u&&v")?;
    assert!(Request::new(&message).is_ok());
    Ok(())
}

#[test]
fn hl7_231_2_24_4_1_251_5_10_5_3_2_keeps_timestamp_precision_and_offsets() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for value in [
            b"2026".as_slice(),
            b"202611",
            b"20240229",
            b"2026111512",
            b"202611151234",
            b"20261115123456.1234+0530",
            b"20261115123456-0000",
            b"20261115^L",
        ] {
            let message = fixture(version, "QRD", 1, value)?;
            let request = Request::new(&message)?;
            assert_eq!(
                request.definition().field(1).map(analyte_hl7::Field::raw),
                Some(value)
            );
        }
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_4_7_251_5_10_5_3_8_nm_and_coded_units_are_not_coerced() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for value in [
            b".5^LI".as_slice(),
            b"+5^LI",
            b"0^LI",
            b"5^LI&L",
            b"5",
            b"5^",
            b"5^ZO",
        ] {
            let message = fixture(version, "QRD", 7, value)?;
            let request = Request::new(&message)?;
            assert_eq!(
                request.definition().field(7).map(analyte_hl7::Field::raw),
                Some(value)
            );
        }
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_4_12_251_5_10_5_3_13_all_query_levels() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for value in [b"O".as_slice(), b"R", b"S", b"T"] {
            let message = fixture(version, "QRD", 12, value)?;
            assert!(Request::new(&message).is_ok());
        }
    }
    Ok(())
}

negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_nm_no_locale_comma,
    "QRD",
    7,
    b"1,2^LI"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_nm_no_exponent,
    "QRD",
    7,
    b"1e2^LI"
);
negative!(
    hl7_231_2_24_4_7_251_5_10_5_3_8_nm_requires_digit,
    "QRD",
    7,
    b".^LI"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_quantity_nm_length,
    "QRF",
    9,
    b"12345678901234567"
);

#[test]
fn hl7_231_2_24_4_8_251_5_10_5_3_9_subject_component_cardinality() -> Result<(), Error> {
    for (version, count) in [("2.3.1", 16), ("2.5.1", 24)] {
        let message = fixture(version, "QRD", 8, &component_value(count, b"unknown"))?;
        assert!(matches!(
            Request::new(&message),
            Err(Error::Control {
                segment: "QRD",
                field: 8
            })
        ));
    }
    let message = fixture("2.3.1", "QRD", 8, b"0001^^^^^^^^&urn:synthetic&URI")?;
    assert!(matches!(
        Request::new(&message),
        Err(Error::Control {
            segment: "QRD",
            field: 8
        })
    ));
    Ok(())
}

#[test]
fn hl7_251_5_10_5_3_10_11_ce_identifier_and_system_lengths() -> Result<(), Error> {
    for field in [9, 10] {
        for value in [
            vec![b'a'; 21],
            [b"X^Text^".as_slice(), &[b'L'; 21]].concat(),
        ] {
            let message = fixture("2.5.1", "QRD", field, &value)?;
            assert!(
                matches!(Request::new(&message),Err(Error::Control {segment:"QRD",field:f}) if f==field)
            );
        }
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_4_5_251_5_10_5_3_4_declared_lengths() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for (segment, field, maximum) in [
            ("QRD", 4, 10),
            ("QRD", 7, 10),
            ("QRD", 11, 20),
            ("QRF", 1, 20),
            ("QRF", 4, 60),
            ("QRF", 5, 60),
            ("QRF", 9, 60),
        ] {
            // Each value remains lexically valid so removing the field length
            // rule actually makes this negative fixture fail.
            let mut value = vec![b'1'; maximum];
            if segment == "QRF" && field == 9 {
                value = vec![b'p'; maximum];
                value.splice(..0, b"^^^^^^^".iter().copied());
                value.truncate(maximum);
            }
            let accepted = fixture(version, segment, field, &value)?;
            assert!(
                Request::new(&accepted).is_ok(),
                "{}-{} boundary",
                segment,
                field
            );
            value.push(b'1');
            let rejected = fixture(version, segment, field, &value)?;
            assert!(
                matches!(Request::new(&rejected),Err(Error::Control {segment:s,field:f}) if s==segment && f==field)
            );
        }
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_5_4_5_251_5_10_5_4_4_5_escaped_local_filters_survive() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for value in [b"Local\\F\\Filter".as_slice(), b"First~Second\\R\\Part"] {
            let message = fixture(version, "QRF", 4, value)?;
            let request = Request::new(&message)?;
            assert_eq!(
                request
                    .filter()
                    .and_then(|s| s.field(4))
                    .map(analyte_hl7::Field::raw),
                Some(value)
            );
        }
    }
    Ok(())
}

negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_explicit_time_clock,
    "QRF",
    9,
    b"^LOCAL&2460"
);
negative!(
    hl7_231_2_24_5_9_251_5_10_5_4_9_tq_explicit_time_list,
    "QRF",
    9,
    b"^LOCAL&0900,"
);

#[test]
fn hl7_231_2_24_5_9_251_5_10_5_4_9_tq_explicit_times_preserved() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        let message = fixture(version, "QRF", 9, b"^LOCAL&0900,1700")?;
        let request = Request::new(&message)?;
        assert_eq!(
            request
                .filter()
                .and_then(|s| s.field(9))
                .map(analyte_hl7::Field::raw),
            Some(b"^LOCAL&0900,1700".as_slice())
        );
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_4_8_10_251_5_10_5_3_9_11_filter_length_boundaries() -> Result<(), Error> {
    for (version, maximum, family, universal) in [("2.3.1", 60, 30, 17), ("2.5.1", 250, 50, 187)] {
        let mut who = vec![b'^'];
        who.extend(vec![b'N'; family]);
        who.extend_from_slice(b"^^^^^^^&");
        who.extend(vec![b'1'; universal]);
        who.extend_from_slice(b"&ISO");
        assert_eq!(who.len(), maximum);
        let message = fixture(version, "QRD", 8, &who)?;
        assert!(Request::new(&message).is_ok());
        // Grow the namespace's ST while leaving its HD components valid.
        let mut longer = vec![b'^'];
        longer.extend(vec![b'N'; family]);
        longer.extend_from_slice(b"^^^^^^^&");
        longer.extend(vec![b'1'; universal.saturating_add(1)]);
        longer.extend_from_slice(b"&ISO");
        let message = fixture(version, "QRD", 8, &longer)?;
        assert!(matches!(
            Request::new(&message),
            Err(Error::Control {
                segment: "QRD",
                field: 8
            })
        ));
        for field in [9, 10] {
            let mut value = if version == "2.3.1" {
                [b"A^".as_slice(), &[b'T'; 58]].concat()
            } else {
                [
                    [b'C'; 20].as_slice(),
                    b"^",
                    &[b'T'; 199],
                    b"^",
                    &[b'L'; 20],
                    b"^",
                    &[b'A'; 8],
                ]
                .concat()
            };
            assert_eq!(value.len(), maximum);
            let message = fixture(version, "QRD", field, &value)?;
            assert!(Request::new(&message).is_ok());
            value.push(b'A');
            let message = fixture(version, "QRD", field, &value)?;
            assert!(
                matches!(Request::new(&message),Err(Error::Control {segment:"QRD",field:f}) if f==field)
            );
        }
    }
    Ok(())
}
