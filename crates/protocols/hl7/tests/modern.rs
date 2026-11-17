//! Tests the bounded modern result profile against the later editions' rules.
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::*;

fn message(edition: &str, encoding: &str, body: &str) -> Result<Message, Error> {
    Message::parse(
        format!("MSH|{encoding}|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|{edition}\r{body}")
            .as_bytes(),
        Options::default(),
    )
}

fn observation(field: usize, value: &str) -> Result<String, Error> {
    let mut fields = [""; 22];
    fields[0] = "1";
    fields[1] = "NM";
    fields[2] = "SYN^Synthetic^LOCAL^^^^v1";
    fields[4] = "0.10";
    fields[6] = "reference";
    fields[10] = "F";
    *fields
        .get_mut(field.checked_sub(1).ok_or(Error::Encoding)?)
        .ok_or(Error::Encoding)? = value;
    Ok(format!("OBR|1||000AbC\rOBX|{}\r", fields.join("|")))
}

#[test]
fn hl7_2_8_2_s2_a_13_and_2_9_1_s2a_2_13_third_tuple_metadata_stays_exact() -> Result<(), Error> {
    let mut parts = [""; 22];
    parts[0] = "SYN";
    parts[2] = "LOCAL";
    parts[6] = "v1";
    parts[9] = "EQUIVALENT";
    parts[11] = "OTHER";
    parts[12] = "v2";
    parts[13] = "1.2.3";
    parts[14] = "1.2.4";
    parts[15] = "20261116";
    parts[19] = "1.2.5";
    parts[20] = "1.2.6";
    parts[21] = "2026";
    let value = parts.join("^");
    for edition in ["2.8.2", "2.9.1"] {
        let input = message(edition, "^~\\&", &observation(3, &value)?)?;
        validate_result_fields(&input)?;
        assert_eq!(
            input
                .segments()
                .nth(2)
                .and_then(|s| s.field(3))
                .ok_or(Error::Encoding)?
                .raw(),
            value.as_bytes()
        );
        assert_eq!(input.encode(), input.raw());
        let extra = format!("{value}^extra");
        assert_eq!(
            validate_result_fields(&message(edition, "^~\\&", &observation(3, &extra)?)?),
            Err(Error::Control {
                segment: "OBX",
                field: 3
            })
        );
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_5_5_3_and_2_9_1_s2_4_5_3_conformance_is_not_a_maximum() -> Result<(), Error> {
    let long = "X".repeat(300);
    for edition in ["2.8.2", "2.9.1"] {
        validate_result_fields(&message(
            edition,
            "^~\\&",
            &observation(3, &format!("SYN^{long}^LOCAL^^^^v1"))?,
        )?)?;
        validate_result_fields(&message(
            edition,
            "^~\\&",
            &observation(3, "IDENTIFIER_LONGER_THAN_TWENTY^Text^LOCAL^^^^v1")?,
        )?)?;
        assert!(
            validate_result_fields(&message(
                edition,
                "^~\\&",
                &observation(3, "SYN^Text^TWELVECHARSX^^^^v1")?
            )?)
            .is_ok()
        );
        assert_eq!(
            validate_result_fields(&message(
                edition,
                "^~\\&",
                &observation(3, "SYN^Text^FOURTEENCHARSX^^^^v1")?
            )?),
            Err(Error::Control {
                segment: "OBX",
                field: 3
            })
        );
        let oversized = "X".repeat(16385);
        assert!(
            validate_result_fields(&message(
                edition,
                "^~\\&",
                &observation(3, &format!("^{oversized}"))?
            )?)
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_a_13_3_7_12_and_2_9_1_s2a_2_13_coding_authority_not_guessed() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        for invalid in [
            "SYN",
            "SYN^Text^LOCAL",
            "SYN^Text^HL7fake",
            "SYN^Text^LOCAL^^^^v1^^^OTHER",
            "SYN^Text^LOCAL^^^^v1^^^OTHER^^ALT",
        ] {
            assert_eq!(
                validate_result_fields(&message(edition, "^~\\&", &observation(3, invalid)?)?),
                Err(Error::Control {
                    segment: "OBX",
                    field: 3
                })
            );
        }
        // OID-only authority is preserved but refused by the named-primary receiving profile.
        let mut parts = [""; 14];
        parts[0] = "SYN";
        parts[13] = "1.2.3";
        let input = message(edition, "^~\\&", &observation(3, &parts.join("^"))?)?;
        assert_eq!(
            validate_result_fields(&input),
            Err(Error::Control {
                segment: "OBX",
                field: 3
            })
        );
        assert_eq!(input.encode(), input.raw());
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_and_2_9_1_s7_4_1_withdrawn_obr_fields_are_refused() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        for field in [5, 6, 14, 15, 27, 16, 28, 32, 33, 34, 35] {
            let mut parts = vec![""; field];
            parts[field - 1] = "opaque";
            let input = message(edition, "^~\\&", &format!("OBR|{}\r", parts.join("|")))?;
            let refused = edition == "2.9.1" || [5, 6, 14, 15, 27].contains(&field);
            assert_eq!(
                validate_result_fields(&input).is_err(),
                refused,
                "{edition} OBR.{field}"
            );
            assert_eq!(input.encode(), input.raw());
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_14_8_and_2_9_1_s2_13_8_withdrawn_msa_err_fields() -> Result<(), Error> {
    for edition in [Version::V282, Version::V291] {
        for field in [3, 5, 6] {
            let mut parts = vec![""; field];
            parts[0] = "AE";
            parts[1] = "id";
            parts[field - 1] = "opaque";
            let input = message(
                edition.as_str(),
                "^~\\&",
                &format!("MSA|{}\r", parts.join("|")),
            )?;
            assert!(
                matches!(Msa::for_version(input.segments().nth(1).ok_or(Error::Encoding)?, edition), Err(Error::Control {segment: "MSA", field: f}) if f == field)
            );
        }
        let input = message(edition.as_str(), "^~\\&", "ERR|legacy||102|E\r")?;
        assert!(matches!(
            ErrView::new(input.segments().nth(1).ok_or(Error::Encoding)?, edition),
            Err(Error::Control {
                segment: "ERR",
                field: 1
            })
        ));
    }
    Ok(())
}

#[test]
fn hl7_2_9_1_s2_13_9_15_16_ack_requests_are_paired() -> Result<(), Error> {
    for (accept, application, valid) in [
        ("", "", true),
        ("AL", "NE", true),
        ("", "AL", false),
        ("AL", "", false),
        ("\"\"", "\"\"", false),
    ] {
        let input = Message::parse(
            format!(
                "MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|2.9.1|||{accept}|{application}\r"
            )
            .as_bytes(),
            Options::default(),
        )?;
        assert_eq!(Msh::new(&input).is_ok(), valid);
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_14_9_and_2_9_1_s2_13_9_ack_preserves_exact_edition() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        let input = message(edition, "^~\\&#", "")?;
        let ack = acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"20261116",
            },
            AckCode::Aa,
            Responsibility::StoredAndInterpreted,
            None,
            Options::default(),
        )?
        .ok_or(Error::Encoding)?;
        assert_eq!(Msh::new(&ack)?.version().as_str(), edition);
        assert_eq!(ack.delimiters().truncation(), Some(b'#'));
        assert_eq!(
            Msa::for_version(
                ack.segments().nth(1).ok_or(Error::Encoding)?,
                Msh::new(&ack)?.version()
            )?
            .control_id(),
            b"id"
        );
        assert!(ack.raw().ends_with(b"MSA|AA|id\r"));
        assert!(
            acknowledgment(
                &input,
                Response {
                    control_id: ControlId::new(b"reply#literal")?,
                    timestamp: b"20261116"
                },
                AckCode::Aa,
                Responsibility::StoredAndInterpreted,
                None,
                Options::default()
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_5_5_2_and_2_9_1_s2_4_5_2_truncation_parks_before_mapping() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        let input = message(edition, "^~\\&#", &observation(5, "0.1#")?)?;
        assert!(matches!(
            input.require_complete_values(),
            Err(Error::Truncated {
                segment: 3,
                field: 5
            })
        ));
        assert_eq!(input.encode(), input.raw());
        let value = message(edition, "^~\\&#", &observation(5, "literal\\P\\")?)?;
        value.require_complete_values()?;
        assert_eq!(decode_text(b"literal\\P\\", &value)?, "literal#");
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s5_10_2_and_2_9_1_s5_9_2_no_typed_legacy_query() -> Result<(), Error> {
    for (edition, section) in [("2.8.2", "5.10.2"), ("2.9.1", "5.9.2")] {
        let input = message(edition, "^~\\&", "")?;
        assert!(
            matches!(query::Request::new(&input), Err(Error::WithdrawnQuery {edition: e, section: s}) if e == edition && s == section)
        );
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_a_22_and_2_9_1_s2a_2_22_scalar_dtm_and_withdrawn_value_types() -> Result<(), Error>
{
    for edition in ["2.8.2", "2.9.1"] {
        for field in [12, 14, 19] {
            for time in [
                "2026",
                "20261116",
                "20261116123045.1234+0000",
                "20261116123045-0000",
            ] {
                validate_result_fields(&message(edition, "^~\\&", &observation(field, time)?)?)?;
            }
            for time in ["20261116^D", "20260230", "20261116123045.12345"] {
                assert!(
                    validate_result_fields(&message(edition, "^~\\&", &observation(field, time)?)?)
                        .is_err()
                );
            }
        }
        for kind in ["CE", "TS"] {
            assert!(
                validate_result_fields(&message(edition, "^~\\&", &observation(2, kind)?)?)
                    .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_14_9_10_and_2_9_1_s2_13_9_10_control_profile_limits() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        for (id, accepted) in [
            ("X".repeat(199), true),
            ("X".repeat(200), false),
            ("literal#id".to_owned(), false),
        ] {
            let input = Message::parse(
                format!("MSH|^~\\&#|S|F|R|F|20261116||ORU^R01^ORU_R01|{id}|P|{edition}\r")
                    .as_bytes(),
                Options::default(),
            )?;
            assert_eq!(Msh::new(&input).is_ok(), accepted);
            if accepted {
                let ack = acknowledgment(
                    &input,
                    Response {
                        control_id: ControlId::new(b"reply")?,
                        timestamp: b"20261116",
                    },
                    AckCode::Ae,
                    Responsibility::Stored,
                    Some(AckError::unlocated(
                        analyte_hl7::APPLICATION_INTERNAL_ERROR,
                        b"review",
                    )),
                    Options::default(),
                )?
                .ok_or(Error::Encoding)?;
                let msh = Msh::new(&ack)?;
                assert_eq!(
                    Msa::for_version(ack.segments().nth(1).ok_or(Error::Encoding)?, msh.version())?
                        .control_id(),
                    id.as_bytes()
                );
                ErrView::new(ack.segments().nth(2).ok_or(Error::Encoding)?, msh.version())?;
            }
        }
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_and_2_9_1_s7_4_2_12_range_date_requires_reference_range() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        let body = observation(12, "20261116")?;
        validate_result_fields(&message(edition, "^~\\&", &body)?)?;
        let invalid = body.rewrite("reference", "");
        assert_eq!(
            validate_result_fields(&message(edition, "^~\\&", &invalid)?),
            Err(Error::Control {
                segment: "OBX",
                field: 7
            })
        );
    }
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_a_88_and_2_9_1_s2a_2_88_unselected_xcn_extensions_stay_raw() -> Result<(), Error> {
    let mut fields = [""; 16];
    let mut name = [""; 25];
    name[0] = "person";
    name[1] = "Synthetic";
    name[8] = "LOCAL";
    name[9] = "L";
    name[12] = "PN";
    name[23] = "ABCD";
    name[24] = "CHK";
    let name = name.join("^");
    for edition in ["2.8.2", "2.9.1"] {
        let input = message(edition, "^~\\&", &observation(16, &name)?)?;
        validate_result_fields(&input)?;
        assert_eq!(input.encode(), input.raw());
    }
    fields[15] = &name;
    let input = message("2.8.2", "^~\\&", &format!("OBR|{}\r", fields.join("|")))?;
    validate_result_fields(&input)?;
    assert_eq!(
        input
            .segments()
            .nth(1)
            .and_then(|s| s.field(16))
            .ok_or(Error::Encoding)?
            .raw(),
        name.as_bytes()
    );
    assert_eq!(input.encode(), input.raw());
    Ok(())
}

#[test]
fn hl7_2_8_2_s2_14_9_18_and_2_9_1_s2_13_9_18_utf8_and_literal_p() -> Result<(), Error> {
    let options = Options {
        charset: Charset::Utf8,
        ..Options::default()
    };
    for edition in ["2.8.2", "2.9.1"] {
        let raw = format!(
            "MSH|^~\\&#|S\\P\\X|F|R|F|20261116||ORU^R01^ORU_R01|id|P|{edition}||||||UNICODE UTF-8\rOBX|1|ST|SYN^Synthetic^LOCAL^^^^v1||Synthetic é\\P\\\r"
        );
        let input = Message::parse(raw.as_bytes(), options)?;
        Msh::new(&input)?;
        validate_result_fields(&input)?;
        input.require_complete_values()?;
        let value = input
            .segments()
            .nth(1)
            .and_then(|s| s.field(5))
            .ok_or(Error::Encoding)?;
        assert_eq!(decode_text(value.raw(), &input)?, "Synthetic é#");
        assert_eq!(
            Message::parse(raw.as_bytes(), Options::default()),
            Err(Error::CharsetMismatch)
        );
        let mixed = raw.rewrite("UNICODE UTF-8", "UNICODE UTF-8~ASCII");
        assert_eq!(
            Message::parse(mixed.as_bytes(), options),
            Err(Error::UnsupportedCharset)
        );
        let ack = acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"20261116",
            },
            AckCode::Aa,
            Responsibility::StoredAndInterpreted,
            None,
            options,
        )?
        .ok_or(Error::Encoding)?;
        Msh::new(&ack)?;
        assert!(
            ack.raw()
                .windows(b"S\\P\\X".len())
                .any(|raw| raw == b"S\\P\\X")
        );
    }
    Ok(())
}

// HL7 2.8.2 section 2.A.50 and 7.4.2.4; 2.9.1 section 2A.2.50 and 7.4.2.4.
#[test]
fn hl7_282_s2_a_50_291_s7_4_2_4_enhanced_og_requires_full_grouping_contract() -> Result<(), Error> {
    for edition in ["2.8.2", "2.9.1"] {
        for group in ["^1^1", "^1^2", "legacy^1", "legacy^^^isolate"] {
            let message = message(edition, "^~\\&", &observation(4, group)?)?;
            assert!(matches!(
                validate_result_fields(&message),
                Err(Error::Control {
                    segment: "OBX",
                    field: 4
                })
            ));
            assert!(
                message
                    .raw()
                    .windows(group.len())
                    .any(|bytes| bytes == group.as_bytes())
            );
        }
        validate_result_fields(&message(edition, "^~\\&", &observation(4, "legacy")?)?)?;
    }
    Ok(())
}

// HL7 2.9.1 section 2.8.3.2: an application acknowledgment cannot request another.
#[test]
fn hl7_291_s2_8_3_2_application_ack_declares_ne_without_recursion() -> Result<(), Error> {
    let raw = b"MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|2.9.1|||AL|AL\r";
    let input = Message::parse(raw, Options::default())?;
    for code in [AckCode::Ca, AckCode::Aa, AckCode::Ae, AckCode::Ar] {
        let ack = acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"20261116",
            },
            code,
            Responsibility::StoredAndInterpreted,
            None,
            Options::default(),
        )?
        .ok_or(Error::Encoding)?;
        let msh = Msh::new(&ack)?;
        for field in [15, 16] {
            assert_eq!(
                msh.segment().field(field).ok_or(Error::Encoding)?.raw(),
                if code == AckCode::Ca {
                    b"".as_slice()
                } else {
                    b"NE".as_slice()
                }
            );
        }
    }
    Ok(())
}
