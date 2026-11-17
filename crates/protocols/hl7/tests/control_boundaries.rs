//! Regressions for HL7 chapter 2 control boundaries.
use analyte_hl7::*;
fn message(version: &str, body: &str) -> Result<Message, Error> {
    Message::parse(
        format!("MSH|^~\\&|S|F|R|F|20261116||ACK^R01^ACK|id|P|{version}\r{body}").as_bytes(),
        Options::default(),
    )
}
#[test]
fn hl7_251_s2_9_2_2_s2_15_9_7_and_231_s2_12_1_2_ack_response_time_is_valid() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        let input = message(version, "")?;
        for timestamp in [
            b"20260230".as_slice(),
            b"202613",
            b"20261",
            b"20260101+2460",
            b"20260101120000.12345",
        ] {
            assert!(matches!(
                acknowledgment(
                    &input,
                    Response {
                        control_id: ControlId::new(b"reply")?,
                        timestamp
                    },
                    AckCode::Aa,
                    Responsibility::StoredAndInterpreted,
                    None,
                    Options::default()
                ),
                Err(Error::Control {
                    segment: "MSH",
                    field: 7
                })
            ));
        }
        let ack = acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"2026",
            },
            AckCode::Aa,
            Responsibility::StoredAndInterpreted,
            None,
            Options::default(),
        )?
        .ok_or(Error::AckMode)?;
        assert_eq!(
            Msh::new(&ack)?
                .segment()
                .field(7)
                .ok_or(Error::Encoding)?
                .raw(),
            b"2026"
        );
    }
    Ok(())
}
#[test]
fn hl7_251_s2_15_5_3_4_err_tables_and_zero_code_severity() -> Result<(), Error> {
    for (code, severity, valid) in [
        ("0", "I", true),
        ("0", "E", false),
        ("207", "F", false),
        ("999", "E", false),
        ("207", "W", true),
        ("207", "I", true),
        ("207", "E", true),
    ] {
        let input = message("2.5.1", &format!("ERR|||{code}|{severity}\r"))?;
        assert_eq!(
            ErrView::new(
                input.segments().nth(1).ok_or(Error::Encoding)?,
                Version::V251
            )
            .is_ok(),
            valid,
            "{code}/{severity}"
        );
    }
    // HL7 2.6 section 2.15.5.4 adds F; 2.8.2 table 0357 adds 104.
    for (edition, version, code, severity, valid) in [
        ("2.6", Version::V26, "207", "F", true),
        ("2.6", Version::V26, "104", "E", false),
        ("2.8.2", Version::V282, "104", "E", true),
        ("2.9.1", Version::V291, "104", "E", true),
    ] {
        let input = message(edition, &format!("ERR|||{code}|{severity}\r"))?;
        assert_eq!(
            ErrView::new(input.segments().nth(1).ok_or(Error::Encoding)?, version).is_ok(),
            valid
        );
    }
    for code in [b"0".as_slice(), b"999"] {
        assert!(matches!(
            acknowledgment(
                &message("2.5.1", "")?,
                Response {
                    control_id: ControlId::new(b"reply")?,
                    timestamp: b"2026"
                },
                AckCode::Ae,
                Responsibility::Stored,
                Some(AckError::unlocated(code, b"")),
                Options::default()
            ),
            Err(Error::Control {
                segment: "ERR",
                field: 3
            })
        ));
    }
    Ok(())
}
#[test]
fn hl7_282_s2_14_8_and_291_s2_13_8_withdrawn_msa_err_explicit_null_policy() -> Result<(), Error> {
    for version in [Version::V282, Version::V291] {
        for field in [3, 5, 6] {
            let mut fields = vec![""; field];
            fields[0] = "AE";
            fields[1] = "id";
            fields[field - 1] = "\"\"";
            let input = message(version.as_str(), &format!("MSA|{}\r", fields.join("|")))?;
            assert!(
                matches!(Msa::for_version(input.segments().nth(1).ok_or(Error::Encoding)?, version), Err(Error::Control {segment: "MSA", field: f}) if f == field)
            );
        }
        let input = message(version.as_str(), "ERR|\"\"||207|E\r")?;
        assert!(matches!(
            ErrView::new(input.segments().nth(1).ok_or(Error::Encoding)?, version),
            Err(Error::Control {
                segment: "ERR",
                field: 1
            })
        ));
    }
    Ok(())
}
