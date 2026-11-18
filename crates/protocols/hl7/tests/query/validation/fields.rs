//! Tests HL7 QRD and QRF field validation.
use super::*;

// HL7 2.3.1 §2.24.4, §2.24.5; HL7 2.5.1 §5.10.5.3, §5.10.5.4
#[test]
fn field_usage_lengths_and_repetitions() -> Result<(), Error> {
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

// HL7 2.5.1 §5.10.5.4.10
#[test]
fn qrf10_threshold_numeric_and_refused_in_older_edition() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.4, §2.24.5; HL7 2.5.1 §5.10.5.3, §5.10.5.4
#[test]
fn local_filters_are_preserved() -> Result<(), Error> {
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
    // HL7 2.3.1 §2.24.5.1; HL7 2.5.1 §5.10.5.4.1
    #[test]
    fn unknown_where_subject_is_preserved(value in "[a-zA-Z0-9]{1,20}") {
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

// HL7 2.5.1 §5.10.5.3.9
#[test]
fn xcn_extended_components_and_lengths() -> Result<(), Error> {
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

// HL7 2.5.1 §5.10.5.4.9
#[test]
fn tq_component_constraints() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.4.1; HL7 2.5.1 §5.10.5.3.2
#[test]
fn query_time_keeps_precision_and_offset() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
#[test]
fn quantity_number_and_unit_are_not_coerced() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.4.12; HL7 2.5.1 §5.10.5.3.13
#[test]
fn all_query_levels_are_accepted() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        for value in [b"O".as_slice(), b"R", b"S", b"T"] {
            let message = fixture(version, "QRD", 12, value)?;
            assert!(Request::new(&message).is_ok());
        }
    }
    Ok(())
}

// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_refuses_locale_comma, "QRD", 7, b"1,2^LI");
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_refuses_exponent, "QRD", 7, b"1e2^LI");
// HL7 2.3.1 §2.24.4.7; HL7 2.5.1 §5.10.5.3.8
negative!(quantity_requires_a_digit, "QRD", 7, b".^LI");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(
    tq_quantity_number_length_is_bounded,
    "QRF",
    9,
    b"12345678901234567"
);

// HL7 2.3.1 §2.24.4.8; HL7 2.5.1 §5.10.5.3.9
#[test]
fn subject_component_count_is_bounded() -> Result<(), Error> {
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

// HL7 2.5.1 §5.10.5.3.10, §5.10.5.3.11
#[test]
fn ce_identifier_and_system_lengths() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.4, §2.24.5; HL7 2.5.1 §5.10.5.3, §5.10.5.4
#[test]
fn declared_field_lengths_are_enforced() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.5.4, §2.24.5.5; HL7 2.5.1 §5.10.5.4.4, §5.10.5.4.5
#[test]
fn escaped_local_filters_survive() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(tq_explicit_time_refuses_bad_clock, "QRF", 9, b"^LOCAL&2460");
// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
negative!(
    tq_explicit_time_refuses_trailing_comma,
    "QRF",
    9,
    b"^LOCAL&0900,"
);

// HL7 2.3.1 §2.24.5.9; HL7 2.5.1 §5.10.5.4.9
#[test]
fn tq_explicit_times_are_preserved() -> Result<(), Error> {
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

// HL7 2.3.1 §2.24.4.8–10; HL7 2.5.1 §5.10.5.3.9–11
#[test]
fn qrd_filter_length_boundaries() -> Result<(), Error> {
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

// HL7 2.3.1 §2.8.20; HL7 2.5.1 §2.A.33
#[test]
fn uri_code_is_edition_specific() -> Result<(), Error> {
    for (edition, accepted) in [("2.3.1", false), ("2.5.1", true)] {
        let message = fixture(edition, "QRD", 8, b"^Name^^^^^^^LAB&urn:synthetic&URI")?;
        assert_eq!(Request::new(&message).is_ok(), accepted);
    }
    Ok(())
}

// HL7 2.3.1 §2.8.49; HL7 2.5.1 §2.A.86, table 0200
#[test]
fn name_type_codes_are_edition_specific() -> Result<(), Error> {
    for (edition, code, accepted) in [
        ("2.3.1", b"I", false),
        ("2.5.1", b"I", true),
        ("2.3.1", b"N", false),
        ("2.5.1", b"N", true),
        ("2.3.1", b"R", false),
        ("2.5.1", b"R", true),
    ] {
        let mut value = b"^Name^^^^^^^^".to_vec();
        value.extend_from_slice(code);
        let message = fixture(edition, "QRD", 8, &value)?;
        assert_eq!(Request::new(&message).is_ok(), accepted);
    }
    Ok(())
}
