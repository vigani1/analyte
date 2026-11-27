//! Tests the bounded 2.6 receiving and backward-compatible query profile.
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::{
    AckCode, AckError, ControlId, ErrView, Error, Message, Msa, Msh, Options, Response,
    Responsibility, Version, acknowledgment, query, validate_result_fields,
};

fn message(body: &str, time: &str, id: &str) -> Result<Message, Error> {
    Message::parse(
        format!("MSH|^~\\&|S|F|R|F|{time}||ORU^R01^ORU_R01|{id}|P|2.6\r{body}").as_bytes(),
        Options::default(),
    )
}

fn observation(field: usize, value: &str) -> Result<Message, Error> {
    let mut fields = [""; 25];
    fields[0] = "1";
    fields[1] = "NM";
    fields[2] = "SYN^Synthetic^HL70001^^^^2.6";
    fields[4] = "0.10";
    fields[10] = "F";
    *fields
        .get_mut(field.checked_sub(1).ok_or(Error::Encoding)?)
        .ok_or(Error::Encoding)? = value;
    message(
        &format!("OBR|1||000AbC\rOBX|{}\r", fields.join("|")),
        "20261116",
        "id",
    )
}

// HL7 2.6 §2.14.9.7, §2.A.22
#[test]
fn msh7_dtm_precision_and_scalar_structure() -> Result<(), Error> {
    for time in [
        "2026",
        "202611",
        "20261116",
        "2026111612",
        "202611161230",
        "20261116123045.1234-0530",
    ] {
        Msh::new(&message("", time, "id")?)?;
    }
    for time in [
        "",
        "20261116^D",
        "20260230",
        "20261116123045.12345",
        "20261116123045+2460",
    ] {
        assert!(matches!(
            Msh::new(&message("", time, "id")?),
            Err(Error::Control {
                segment: "MSH",
                field: 7
            })
        ));
    }
    Ok(())
}

// HL7 2.6 §2.14.8.2, §2.14.9.10
#[test]
fn control_id_length_ack_and_err_shape() -> Result<(), Error> {
    let id = "A".repeat(199);
    let input = message("", "20261116", &id)?;
    assert_eq!(Msh::new(&input)?.control_id(), id.as_bytes());
    let ack = acknowledgment(
        &input,
        Response {
            control_id: ControlId::new(b"reply")?,
            timestamp: b"20261116",
        },
        AckCode::Ae,
        Responsibility::Stored,
        Some(AckError {
            segment: b"OBX",
            occurrence: 1,
            field: 5,
            code: b"102",
            application_code: b"profile",
        }),
        Options::default(),
    )?
    .ok_or(Error::Encoding)?;
    let msh = Msh::new(&ack)?;
    assert_eq!(msh.version(), Version::V26);
    assert_eq!(
        msh.message_type().collect::<Vec<_>>(),
        [b"ACK".as_slice(), b"R01", b"ACK"]
    );
    let msa = Msa::for_version(ack.segments().nth(1).ok_or(Error::Encoding)?, Version::V26)?;
    assert_eq!(msa.control_id(), id.as_bytes());
    assert!(matches!(
        ErrView::new(ack.segments().nth(2).ok_or(Error::Encoding)?, Version::V26)?,
        ErrView::V251 { .. }
    ));
    assert!(matches!(
        Msh::new(&message("", "20261116", &"A".repeat(200))?),
        Err(Error::Control {
            segment: "MSH",
            field: 10
        })
    ));
    let deprecated = message("MSA|AA|id|opaque||D|legacy\r", "20261116", "reply")?;
    let msa = Msa::for_version(
        deprecated.segments().nth(1).ok_or(Error::Encoding)?,
        Version::V26,
    )?;
    assert_eq!(msa.segment().field(5).ok_or(Error::Encoding)?.raw(), b"D");
    assert_eq!(deprecated.encode(), deprecated.raw());
    Ok(())
}

// HL7 2.6 §2.9.3, §2.14.9.15, §2.14.9.16
#[test]
fn enhanced_ack_follows_custody() -> Result<(), Error> {
    let input = Message::parse(
        b"MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|2.6|||AL|ER\r",
        Options::default(),
    )?;
    for (code, custody, expected) in [
        (AckCode::Ca, Responsibility::Stored, true),
        (AckCode::Aa, Responsibility::StoredAndInterpreted, false),
        (AckCode::Ae, Responsibility::Stored, true),
    ] {
        assert_eq!(
            acknowledgment(
                &input,
                Response {
                    control_id: ControlId::new(b"reply")?,
                    timestamp: b"20261116"
                },
                code,
                custody,
                None,
                Options::default()
            )?
            .is_some(),
            expected
        );
    }
    assert_eq!(
        acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"20261116"
            },
            AckCode::Ca,
            Responsibility::NotStored,
            None,
            Options::default()
        ),
        Err(Error::NotStored)
    );
    assert_eq!(
        acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"20260230"
            },
            AckCode::Ca,
            Responsibility::Stored,
            None,
            Options::default()
        ),
        Err(Error::Control {
            segment: "MSH",
            field: 7
        })
    );
    Ok(())
}

// HL7 2.5.1 §2.9.2; ADR 0010 (2026-11-26): AA accepts responsibility, so a
// held interpretation may be accepted, but bytes merely stored may not.
#[test]
fn held_custody_allows_aa_and_stored_alone_does_not() -> Result<(), Error> {
    let input = Message::parse(
        b"MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|2.5.1\r",
        Options::default(),
    )?;
    let ack = |custody| {
        acknowledgment(
            &input,
            Response {
                control_id: ControlId::new(b"reply")?,
                timestamp: b"20261116",
            },
            AckCode::Aa,
            custody,
            None,
            Options::default(),
        )
    };
    let held = ack(Responsibility::StoredAndHeld)?.ok_or(Error::Encoding)?;
    assert!(held.raw().ends_with(b"MSA|AA|id\r"));
    assert_eq!(ack(Responsibility::Stored), Err(Error::NotStored));
    assert_eq!(ack(Responsibility::NotStored), Err(Error::NotStored));
    Ok(())
}

// HL7 2.6 §2.7.1, §2.14.9.18
#[test]
fn utf8_charset_and_literal_escapes() -> Result<(), Error> {
    let raw = "MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|2.6||||||UNICODE UTF-8\rOBX|1|ST|SYN||Synthetic é\\F\\\\S\\\\T\\\\R\\\\E\\\r";
    let input = Message::parse(
        raw.as_bytes(),
        Options {
            charset: analyte_hl7::Charset::Utf8,
            ..Options::default()
        },
    )?;
    let value = input
        .segments()
        .nth(1)
        .and_then(|s| s.field(5))
        .ok_or(Error::Encoding)?;
    assert_eq!(
        analyte_hl7::decode_text(value.raw(), &input)?,
        "Synthetic é|^&~\\"
    );
    assert_eq!(input.encode(), raw.as_bytes());
    assert_eq!(
        Message::parse(raw.as_bytes(), Options::default()),
        Err(Error::CharsetMismatch)
    );
    Ok(())
}

// HL7 2.6 §7.4.1.5, §7.4.1.6, §7.4.1.15, §7.4.1.27, §7.4.1.32–35
#[test]
fn backward_compatible_obr_fields_stay_raw() -> Result<(), Error> {
    for field in [5, 6, 15, 27, 32, 33, 34, 35] {
        let mut fields = vec![""; field];
        fields[field - 1] = "opaque&source";
        let input = message(&format!("OBR|{}\r", fields.join("|")), "20261116", "id")?;
        validate_result_fields(&input)?;
        assert_eq!(
            input
                .segments()
                .nth(1)
                .and_then(|s| s.field(field))
                .ok_or(Error::Encoding)?
                .raw(),
            b"opaque&source"
        );
        assert_eq!(input.encode(), input.raw());
    }
    Ok(())
}

// HL7 2.6 §2.A.6, §2.A.77
#[test]
fn withdrawn_ce_and_ts_are_refused() -> Result<(), Error> {
    for datatype in ["CE", "TS"] {
        let input = observation(2, datatype)?;
        assert_eq!(
            validate_result_fields(&input),
            Err(Error::Control {
                segment: "OBX",
                field: 2
            })
        );
        assert!(
            input
                .encode()
                .windows(datatype.len())
                .any(|v| v == datatype.as_bytes())
        );
    }
    Ok(())
}

// HL7 2.6 §7.4.2.3, §7.4.2.6, §2.A.13
#[test]
fn obx_cwe_components_lengths_and_versions() -> Result<(), Error> {
    for field in [3, 6] {
        validate_result_fields(&observation(field, "SYN^Synthetic^LOCAL^^^^v1")?)?;
        for bad in ["SYN^Synthetic^LOCAL", "SYN^Text^HL70001^^^^^^^extra"] {
            assert_eq!(
                validate_result_fields(&observation(field, bad)?),
                Err(Error::Control {
                    segment: "OBX",
                    field
                })
            );
        }
        let code = "A".repeat(20);
        validate_result_fields(&observation(field, &format!("{code}^Text^HL70001^^^^2.6"))?)?;
        assert!(
            validate_result_fields(&observation(
                field,
                &format!("{code}A^Text^HL70001^^^^2.6")
            )?)
            .is_err()
        );
    }
    let input = observation(2, "CWE")?;
    // A nine-component CWE permits plain original text as well as coded tuples.
    validate_result_fields(&input)?;
    Ok(())
}

// HL7 2.6 §7.4.1.7, §7.4.1.8, §7.4.1.14, §7.4.1.22, §7.4.2.12, §7.4.2.14, §7.4.2.19
#[test]
fn result_times_use_scalar_dtm() -> Result<(), Error> {
    for field in [12, 14, 19] {
        validate_result_fields(&observation(field, "20261116123045.1234+0000")?)?;
        assert_eq!(
            validate_result_fields(&observation(field, "20261116^D")?),
            Err(Error::Control {
                segment: "OBX",
                field
            })
        );
    }
    for field in [7, 8, 14, 22] {
        let mut fields = vec![""; field];
        fields[field - 1] = "20261116";
        validate_result_fields(&message(
            &format!("OBR|{}\r", fields.join("|")),
            "20261116",
            "id",
        )?)?;
        fields[field - 1] = "20261116^D";
        assert_eq!(
            validate_result_fields(&message(
                &format!("OBR|{}\r", fields.join("|")),
                "20261116",
                "id"
            )?),
            Err(Error::Control {
                segment: "OBR",
                field
            })
        );
    }
    validate_result_fields(&observation(21, "opaque^namespace")?)?;
    assert_eq!(
        validate_result_fields(&observation(22, "INT^Intent^HL70725")?),
        Err(Error::Control {
            segment: "OBX",
            field: 22
        })
    );
    Ok(())
}

fn request(qrd: &str, qrf: &str) -> Result<Message, Error> {
    Message::parse(
        format!("MSH|^~\\&|S|F|R|F|20261116||QRY^Q02^QRY_Q02|query|P|2.6\rQRD|{qrd}\r{qrf}")
            .as_bytes(),
        Options::default(),
    )
}

// HL7 2.6 §2.A.13
#[test]
fn cwe_component_length_boundaries() -> Result<(), Error> {
    for (offset, maximum) in [20, 199, 20, 20, 199, 20, 10, 10, 199]
        .into_iter()
        .enumerate()
    {
        for (length, accepted) in [(maximum, true), (maximum + 1, false)] {
            let value = "A".repeat(length);
            let mut parts = [""; 9];
            parts[6] = "v1";
            parts[7] = "v1";
            parts[offset] = &value;
            let input = observation(3, &parts.join("^"))?;
            assert_eq!(
                validate_result_fields(&input).is_ok(),
                accepted,
                "component {}",
                offset + 1
            );
        }
    }
    Ok(())
}

// HL7 2.6 §2.A.86
#[test]
fn xcn_check_digit_cwe_dtm_and_deprecated_range() -> Result<(), Error> {
    for (component, value, accepted) in [
        (11, "1234", true),
        (11, "12345", false),
        (16, "LOCAL&Text&HL70001&&&&v1&&original", true),
        (16, "LOCAL&Text&LOCAL", false),
        (17, "20260101&20261231", false),
        (19, "20261116", true),
        (19, "20261116&D", false),
        (20, "20261116123045.1234-0530", true),
        (23, "LOCAL&Text&HL70001&&&&2.6", true),
    ] {
        let mut who = vec![""; 23];
        who[1] = "Synthetic";
        who[11] = "M10";
        who[component - 1] = value;
        let qrd = format!("20261116|D|D|tag|||5^LI|{}|LOCAL|LOCAL", who.join("^"));
        assert_eq!(
            query::Request::new(&request(&qrd, "")?).is_ok(),
            accepted,
            "XCN.{component}"
        );
    }
    let long_name = "A".repeat(251);
    let qrd = format!("20261116|D|D|tag|||5^LI|^{long_name}|LOCAL|LOCAL");
    assert!(matches!(
        query::Request::new(&request(&qrd, "")?),
        Err(Error::Control {
            segment: "QRD",
            field: 8
        })
    ));
    Ok(())
}

// HL7 2.6 §5.10.4.1, §5.10.4.2
#[test]
fn qrd_qrf_use_dtm_cwe_and_refuse_tq() -> Result<(), Error> {
    let qrd = "20261116|D|D|tag|||5^LI|^Synthetic|LOCAL|LOCAL";
    let input = request(qrd, "QRF|LAB|20260101|20261231|||||||0.9\r")?;
    assert_eq!(query::Request::new(&input)?.query_id(), b"tag");
    for (bad_qrd, bad_qrf, segment, field) in [
        (
            qrd.rewrite("20261116", "20261116^D"),
            "".to_owned(),
            "QRD",
            1,
        ),
        (
            qrd.rewrite("LOCAL|LOCAL", "SYN^Text^LOCAL|LOCAL"),
            "".to_owned(),
            "QRD",
            9,
        ),
        (qrd.to_owned(), "QRF|LAB|20261116^D\r".to_owned(), "QRF", 2),
        (qrd.to_owned(), "QRF|LAB||||||||1\r".to_owned(), "QRF", 9),
    ] {
        assert!(
            matches!(query::Request::new(&request(&bad_qrd, &bad_qrf)?), Err(Error::Control {segment: s, field: f}) if s == segment && f == field)
        );
    }
    let cwe = qrd.rewrite("LOCAL|LOCAL", "SYN^Text^LOCAL^^^^v1|LOCAL");
    query::Request::new(&request(&cwe, "")?)?;
    Ok(())
}
