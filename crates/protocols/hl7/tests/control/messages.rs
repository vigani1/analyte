//! Tests HL7 control fields and acknowledgments.
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::*;
use proptest::prelude::*;

// HL7 2.5.1 section 2.15.5.2 pp.2-67-68; 2.3.1 section 2.24.3.1.
// HL7 2.5.1 §2.15.5.2
#[test]
fn unknown_error_location_stays_unvalued() -> Result<(), Error> {
    for (version, expected) in [
        ("2.5.1", "ERR|||207|E|operator-review\r"),
        ("2.3.1", "ERR|^^^207\r"),
    ] {
        let message = parse(&wire(version, "", "", "", ""))?;
        let ack = acknowledgment(
            &message,
            response()?,
            AckCode::Ae,
            Responsibility::Stored,
            Some(AckError::unlocated(
                APPLICATION_INTERNAL_ERROR,
                b"operator-review",
            )),
            Options::default(),
        )?
        .ok_or(Error::AckMode)?;
        assert!(ack.raw().ends_with(expected.as_bytes()));
    }
    Ok(())
}

// HL7 2.5.1 section 2.7.6 pp.2-19-20; synthetic command/argument cases.
// HL7 2.5.1 §2.7.6
#[test]
fn formatting_command_grammar() -> Result<(), Error> {
    for command in [
        ".br", ".fi", ".nf", ".ce", ".sp", ".sp3", ".sp 3", ".in+2", ".in -3", ".ti2", ".ti-2",
        ".sk4",
    ] {
        let raw = wire(
            "2.5.1",
            "",
            "",
            "",
            &format!("OBX|1|FT|SYN||\\{command}\\\r"),
        );
        assert_eq!(parse(&raw)?.encode(), raw);
    }
    for command in [
        ".unknown", ".br2", ".ce x", ".sp0", ".sp-2", ".sp1.5", ".in", ".ti+", ".in2x", ".sk",
        ".sk-2",
    ] {
        let raw = wire(
            "2.5.1",
            "",
            "",
            "",
            &format!("OBX|1|FT|SYN||\\{command}\\\r"),
        );
        assert!(
            matches!(
                parse(&raw),
                Err(Error::Escape {
                    segment: 2,
                    field: 5,
                    ..
                })
            ),
            "{command}"
        );
    }
    Ok(())
}

fn wire(version: &str, accept: &str, application: &str, charset: &str, body: &str) -> Vec<u8> {
    format!("MSH|^~\\&|SENDER|SF|RECEIVER|RF|202601011200||ORU^R01^ORU_R01|00017|P|{version}|||{accept}|{application}||{charset}\r{body}").into_bytes()
}
fn parse(raw: &[u8]) -> Result<Message, Error> {
    Message::parse(raw, Options::default())
}
fn response() -> Result<Response<'static>, Error> {
    Ok(Response {
        control_id: ControlId::new(b"reply-1")?,
        timestamp: b"202601011201",
    })
}

// HL7 2.5.1 §2.5.4
#[test]
fn alternate_delimiters_and_msh_indexing() -> Result<(), Error> {
    let raw =
        b"MSH*$%!@*SENDER*SF*RECEIVER*RF*202601011200**ORU$R01$ORU_R01*00017*P*2.5.1\rOBX**A$$B@C%%\r";
    let m = parse(raw)?;
    let msh = Msh::new(&m)?;
    assert_eq!(m.delimiters().bytes(), *b"*$%!@");
    assert_eq!(msh.segment().field(1).map(Field::raw), Some(&b"*"[..]));
    assert_eq!(msh.segment().field(2).map(Field::raw), Some(&b"$%!@"[..]));
    assert_eq!(msh.control_id(), b"00017");
    let obx = m.segments().nth(1).ok_or(Error::Segment(2))?;
    let f = obx.field(2).ok_or(Error::Segment(2))?;
    assert_eq!(f.repetitions().count(), 3);
    assert_eq!(
        f.repetitions().next().map(|r| r.components().count()),
        Some(3)
    );
    assert_eq!(m.encode(), raw);
    Ok(())
}
// HL7 2.5.1 §2.5.3
#[test]
fn null_empty_absent_and_trailing_positions() -> Result<(), Error> {
    let m = parse(&wire("2.5.1", "", "", "", "OBX|\"\"||A^^B&C|\r"))?;
    let s = m.segments().nth(1).ok_or(Error::Segment(2))?;
    assert!(s.field(1).is_some_and(Field::is_null));
    assert_eq!(s.field(2).map(Field::raw), Some(&b""[..]));
    assert_eq!(s.field(4).map(Field::raw), Some(&b""[..]));
    assert!(s.field(5).is_none());
    let f = s.field(3).ok_or(Error::Segment(2))?;
    let components: Vec<_> = f
        .repetitions()
        .flat_map(Repetition::components)
        .map(Component::raw)
        .collect();
    assert_eq!(components, vec![&b"A"[..], b"", b"B&C"]);
    Ok(())
}
// HL7 2.5.1 §2.7
#[test]
fn escape_sequences_in_text_fields() -> Result<(), Error> {
    let body = "NTE|1||a\\F\\b\\S\\c\\R\\d\\T\\e\\E\\f\\X0D\\\\Zlocal\\\\.br\\\\H\\hi\\N\\\r";
    let m = parse(&wire("2.5.1", "", "", "", body))?;
    let f = m
        .segments()
        .nth(1)
        .and_then(|s| s.field(3))
        .ok_or(Error::Segment(2))?;
    assert_eq!(f.repetitions().count(), 1);
    assert_eq!(
        decode_text(f.raw(), &m)?,
        "a|b^c~d&e\\f\\X0D\\\\Zlocal\\\\.br\\\\H\\hi\\N\\"
    );
    for bad in ["\\X0\\", "\\XGG\\", "\\F", "\\Q\\"] {
        let raw = wire("2.5.1", "", "", "", &format!("NTE|1||{bad}\r"));
        assert!(matches!(
            parse(&raw),
            Err(Error::Escape {
                segment: 2,
                field: 3,
                ..
            })
        ));
        let m = Message::parse(
            &raw,
            Options {
                escapes: EscapePolicy::Preserve,
                ..Options::default()
            },
        )?;
        assert_eq!(m.encode(), raw);
    }
    Ok(())
}
// HL7 2.5.1 §2.5.4
#[test]
fn terminators_require_explicit_policy() -> Result<(), Error> {
    let raw = wire("2.5.1", "", "", "", "NTE|1||ok\r");
    for (ending, policy) in [("\n", Terminator::Lf), ("\r\n", Terminator::CrLf)] {
        let altered = String::from_utf8_lossy(&raw)
            .replace('\r', ending)
            .into_bytes();
        assert!(parse(&altered).is_err());
        let m = Message::parse(
            &altered,
            Options {
                terminator: policy,
                ..Options::default()
            },
        )?;
        assert_eq!(m.segments().count(), 2);
        assert_eq!(m.encode(), altered);
    }
    let empty_segment = wire("2.5.1", "", "", "", "\r");
    assert!(parse(&empty_segment).is_err());
    Ok(())
}
// HL7 2.5.1 §2.15.9.18
#[test]
fn charset_is_lossless_and_connection_checked() -> Result<(), Error> {
    let mut raw = wire("2.5.1", "", "", "8859/1", "NTE|1||");
    raw.extend_from_slice(&[0xe9, b'\r']);
    assert_eq!(parse(&raw), Err(Error::CharsetMismatch));
    let m = Message::parse(
        &raw,
        Options {
            charset: Charset::Latin1,
            ..Options::default()
        },
    )?;
    assert_eq!(decode_text(&[0xe9], &m)?, "\u{e9}");
    let utf = wire("2.5.1", "", "", "UNICODE UTF-8", "NTE|1||?\r");
    assert!(
        Message::parse(
            &utf,
            Options {
                charset: Charset::Utf8,
                ..Options::default()
            }
        )
        .is_ok()
    );
    let unknown = wire("2.5.1", "", "", "8859/2", "");
    assert_eq!(parse(&unknown), Err(Error::UnsupportedCharset));
    let mut invalid = wire("2.5.1", "", "", "UNICODE UTF-8", "NTE|1||");
    invalid.extend_from_slice(&[0xff, b'\r']);
    assert_eq!(
        Message::parse(
            &invalid,
            Options {
                charset: Charset::Utf8,
                ..Options::default()
            }
        ),
        Err(Error::CharacterEncoding)
    );
    Ok(())
}
// Deliberate tolerance: an empty MSH-18 takes the machine definition's set.
// HL7 2.5.1 §2.15.9.18
#[test]
fn empty_msh18_uses_definition_charset() -> Result<(), Error> {
    let mut raw = wire("2.5.1", "", "", "", "NTE|1||");
    raw.extend_from_slice(&[0xe9, b'\r']);
    let latin1 = Options {
        charset: Charset::Latin1,
        ..Options::default()
    };
    let m = Message::parse(&raw, latin1)?;
    assert_eq!(m.raw(), raw.as_slice());
    assert_eq!(m.charset(), Charset::Latin1);
    assert_eq!(decode_text(&[0xe9], &m)?, "\u{e9}");
    assert_eq!(
        wire_compatibility(&m).charset_from_definition,
        Some(Charset::Latin1)
    );
    // Plain ASCII needs no note; an explicit ASCII declaration still disagrees.
    let ascii = wire("2.5.1", "", "", "", "NTE|1||e\r");
    let plain = Message::parse(&ascii, latin1)?;
    assert_eq!(wire_compatibility(&plain).charset_from_definition, None);
    let explicit = wire("2.5.1", "", "", "ASCII", "NTE|1||e\r");
    assert_eq!(
        Message::parse(&explicit, latin1),
        Err(Error::CharsetMismatch)
    );
    // A definition naming no other set keeps refusing, with a specific reason.
    assert_eq!(parse(&raw), Err(Error::UndeclaredCharset));
    // LAW requires MSH-18 (TF-2b §C.4), so its parse keeps the standard default.
    assert!(law::parse_message(&raw, latin1).is_err());
    Ok(())
}

// HL7 2.5.1 §2.9; HL7 2.3.1 §2.12
#[test]
fn acknowledgments_require_explicit_responsibility() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        let m = parse(&wire(version, "", "", "", ""))?;
        for state in [Responsibility::NotStored, Responsibility::Stored] {
            assert_eq!(
                acknowledgment(
                    &m,
                    response()?,
                    AckCode::Aa,
                    state,
                    None,
                    Options::default()
                ),
                Err(Error::NotStored)
            );
        }
        for code in [AckCode::Aa, AckCode::Ae, AckCode::Ar] {
            let ack = acknowledgment(
                &m,
                response()?,
                code,
                Responsibility::StoredAndInterpreted,
                None,
                Options::default(),
            )?
            .ok_or(Error::AckMode)?;
            let header = Msh::new(&ack)?;
            assert_eq!(header.control_id(), b"reply-1");
            assert_eq!(
                header.segment().field(5).map(Field::raw),
                Some(&b"SENDER"[..])
            );
            assert_eq!(
                header.segment().field(3).map(Field::raw),
                Some(&b"RECEIVER"[..])
            );
            assert_eq!(header.segment().field(16).map(Field::raw), Some(&b""[..]));
            let msa = Msa::new(ack.segments().nth(1).ok_or(Error::Segment(2))?)?;
            assert_eq!(msa.code(), code);
            assert_eq!(msa.control_id(), b"00017");
        }
    }
    Ok(())
}
// HL7 2.5.1 §2.9.3
#[test]
fn enhanced_ack_conditions_and_suppression() -> Result<(), Error> {
    for accept in ["AL", "NE", "ER", "SU"] {
        for application in ["AL", "NE", "ER", "SU"] {
            let m = parse(&wire("2.5.1", accept, application, "", ""))?;
            for (code, success, condition) in [
                (AckCode::Ca, true, accept),
                (AckCode::Ce, false, accept),
                (AckCode::Cr, false, accept),
                (AckCode::Aa, true, application),
                (AckCode::Ae, false, application),
                (AckCode::Ar, false, application),
            ] {
                let expected = condition == "AL"
                    || (condition == "SU" && success)
                    || (condition == "ER" && !success);
                assert_eq!(
                    acknowledgment(
                        &m,
                        response()?,
                        code,
                        Responsibility::StoredAndInterpreted,
                        None,
                        Options::default()
                    )?
                    .is_some(),
                    expected
                );
            }
            assert_eq!(
                acknowledgment(
                    &m,
                    response()?,
                    AckCode::Ca,
                    Responsibility::NotStored,
                    None,
                    Options::default()
                ),
                Err(Error::NotStored)
            );
        }
    }
    Ok(())
}
// HL7 2.3.1 §2.24.3; HL7 2.5.1 §2.15.5
#[test]
fn err_segment_is_edition_specific() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1"] {
        let m = parse(&wire(version, "", "", "", ""))?;
        let detail = AckError {
            segment: b"OBX",
            occurrence: 2,
            field: 5,
            code: b"102",
            application_code: b"LOCAL",
        };
        let ack = acknowledgment(
            &m,
            response()?,
            AckCode::Ae,
            Responsibility::Stored,
            Some(detail),
            Options::default(),
        )?
        .ok_or(Error::AckMode)?;
        let err = ack.segments().nth(2).ok_or(Error::Segment(3))?;
        match ErrView::new(err, Msh::new(&ack)?.version())? {
            ErrView::V231(field) => assert_eq!(field.raw(), b"OBX^2^5^102"),
            ErrView::V251 {
                locations,
                code,
                severity,
                application,
                ..
            } => {
                assert_eq!(locations.map(Field::raw), Some(&b"OBX^2^5"[..]));
                assert_eq!(code.raw(), b"102");
                assert_eq!(severity.raw(), b"E");
                assert_eq!(application.map(Field::raw), Some(&b"LOCAL"[..]));
            }
        }
    }
    Ok(())
}
// HL7 2.5.1 §2.6
#[test]
fn configured_limits_reject_without_truncation() {
    let raw = wire("2.5.1", "", "", "", "NTE|1||A~B\r");
    for limits in [
        Limits {
            message_bytes: 10,
            ..Limits::default()
        },
        Limits {
            segments: 1,
            ..Limits::default()
        },
        Limits {
            fields: 3,
            ..Limits::default()
        },
        Limits {
            repetitions: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            Message::parse(
                &raw,
                Options {
                    limits,
                    ..Options::default()
                }
            ),
            Err(Error::Limit { .. })
        ));
    }
}
proptest! {
    // HL7 2.5.1 §2.6
    #[test]
    fn serialize_parse_identity(value in "[a-zA-Z0-9 ^&~]{0,300}") {
        let raw = wire("2.5.1", "", "", "", &format!("NTE|1||{value}|\r"));
        let first = parse(&raw)?;
        prop_assert_eq!(first.encode(), raw);
        prop_assert_eq!(parse(&first.encode())?, first);
    }
    // HL7 2.5.1 §2.6
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
        if let Ok(message) = parse(&bytes) { prop_assert_eq!(parse(&message.encode())?, message); }
    }
}

// HL7 2.5.1 §2.15
#[test]
fn control_fields_reject_bad_codes_and_injection() -> Result<(), Error> {
    let raw = wire("2.5.1", "BAD", "", "", "");
    let m = parse(&raw)?;
    assert!(matches!(
        Msh::new(&m),
        Err(Error::Control { field: 15, .. })
    ));
    for (from, to, field) in [
        ("00017", "x^y", 10),
        ("|P|", "|Q|", 11),
        ("ORU^R01", "ORU^", 9),
    ] {
        let changed = String::from_utf8_lossy(&wire("2.5.1", "", "", "", "")).rewrite(from, to);
        let m = parse(changed.as_bytes())?;
        assert!(
            matches!(Msh::new(&m), Err(Error::Control { field: actual, .. }) if actual == field)
        );
    }
    let m = parse(&wire("2.5.1", "", "", "", "MSA|ZZ|00017\r"))?;
    assert!(matches!(
        m.segments().nth(1).map(Msa::new),
        Some(Err(Error::Control {
            segment: "MSA",
            field: 1
        }))
    ));
    Ok(())
}
// HL7 2.5.1 §2.15.9.2
#[test]
fn later_version_truncation_is_rejected() {
    assert_eq!(
        parse(b"MSH|^~\\&#|A|B|C|D|20260101||ORU^R01^ORU_R01|1|P|2.5.1\r"),
        Err(Error::Truncation)
    );
}
// HL7 2.5.1 §2.7.2
#[test]
fn charset_switches_kept_but_not_misdecoded() -> Result<(), Error> {
    let m = parse(&wire("2.5.1", "", "", "", "NTE|1||\\C2D41\\abc\r"))?;
    let f = m
        .segments()
        .nth(1)
        .and_then(|s| s.field(3))
        .ok_or(Error::Segment(2))?;
    assert_eq!(f.raw(), b"\\C2D41\\abc");
    assert_eq!(decode_text(f.raw(), &m), Err(Error::UnsupportedCharset));
    Ok(())
}
proptest! {
    // HL7 2.5.1 §2.7
    #[test]
    fn delimiter_escape_never_adds_component(prefix in "[a-z]{0,50}", suffix in "[a-z]{0,50}") {
        let raw = wire("2.5.1", "", "", "", &format!("NTE|1||{prefix}\\S\\{suffix}\r"));
        let m = parse(&raw)?;
        let f = m.segments().nth(1).and_then(|s| s.field(3)).ok_or(Error::Segment(2))?;
        prop_assert_eq!(f.repetitions().flat_map(Repetition::components).count(), 1);
        prop_assert_eq!(decode_text(f.raw(), &m)?, format!("{prefix}^{suffix}"));
    }
}

// HL7 2.5.1 §2.5.4
#[test]
fn crlf_policy_rejects_mixed_line_endings() {
    let raw = wire("2.5.1", "", "", "", "NTE|1||ok\r\n");
    assert_eq!(
        Message::parse(
            &raw,
            Options {
                terminator: Terminator::CrLf,
                ..Options::default()
            }
        ),
        Err(Error::Terminator)
    );
}
// HL7 2.5.1 §2.9
#[test]
fn ack_uses_negotiated_separators() -> Result<(), Error> {
    let m = parse(b"MSH*$%!@*S*SF*R*RF*20260101**ORU$R01$ORU_R01*00017*P*2.5.1***AL*AL\r")?;
    let ack = acknowledgment(
        &m,
        response()?,
        AckCode::Ca,
        Responsibility::Stored,
        None,
        Options::default(),
    )?
    .ok_or(Error::AckMode)?;
    assert!(ack.raw().windows(13).any(|s| s == b"MSA*CA*00017\r"));
    assert_eq!(
        Msh::new(&ack)?.message_type().collect::<Vec<_>>(),
        vec![&b"ACK"[..], b"R01", b"ACK"]
    );
    Ok(())
}
// HL7 2.3.1 §2.24.2.6
#[test]
fn legacy_msa_fields_preserved() -> Result<(), Error> {
    let m = parse(&wire(
        "2.3.1",
        "",
        "",
        "",
        "MSA|AE|00017|detail|||102^type^HL70357\r",
    ))?;
    let msa = Msa::new(m.segments().nth(1).ok_or(Error::Segment(2))?)?;
    assert_eq!(
        msa.segment().field(6).map(Field::raw),
        Some(&b"102^type^HL70357"[..])
    );
    Ok(())
}

// HL7 2.5.1 sections 2.15.9, 2.15.9.7 pp.2-74, 2-76;
// HL7 2.3.1 section 2.24.1.7: timestamp usage remains optional.
// HL7 2.5.1 §2.15.9.7; HL7 2.3.1
#[test]
fn message_time_required_except_in_v231() -> Result<(), Error> {
    for version in ["2.5.1", "2.3.1"] {
        for stamp in [
            "",
            "\"\"",
            "20260230",
            "202613",
            "202601011260",
            "20260101~20260102",
            "20260101&x",
            "20260101^Q",
            "20260101^D^extra",
        ] {
            let raw = String::from_utf8(wire(version, "", "", "", ""))
                .expect("synthetic ASCII")
                .rewrite("202601011200", stamp)
                .into_bytes();
            let m = parse(&raw)?;
            if version == "2.3.1" && stamp.is_empty() {
                assert!(Msh::new(&m).is_ok());
            } else {
                assert!(
                    matches!(
                        Msh::new(&m),
                        Err(Error::Control {
                            segment: "MSH",
                            field: 7
                        })
                    ),
                    "{version}: {stamp}"
                );
            }
            assert_eq!(m.encode(), raw);
        }
        for stamp in [
            "2026",
            "202601",
            "20260228",
            "20260101120000.1234+0130",
            "20260101^D",
        ] {
            let raw = String::from_utf8(wire(version, "", "", "", ""))
                .expect("synthetic ASCII")
                .rewrite("202601011200", stamp)
                .into_bytes();
            let m = parse(&raw)?;
            assert!(Msh::new(&m).is_ok(), "{version}: {stamp}");
            assert_eq!(m.encode(), raw);
        }
    }
    Ok(())
}

// HL7 2.5.1 section 2.A.44 pp.2-180-181; 2.3.1 section 2.24.1.9 p.2-95.
// HL7 2.5.1 §2.A.44; HL7 2.3.1 §2.24.1.9
#[test]
fn message_type_components_are_checked() -> Result<(), Error> {
    for (version, kind, valid) in [
        ("2.5.1", "ORU^R01", false),
        ("2.5.1", "ACK", false),
        ("2.5.1", "ACK^^ACK", false),
        ("2.5.1", "ORUU^R01^ORU_R01", false),
        ("2.5.1", "ORU^R001^ORU_R01", false),
        ("2.5.1", "ORU^R01^ORU_R011", false),
        ("2.5.1", "ORU^R01^ORU_R01", true),
        ("2.3.1", "ACK", true),
        ("2.3.1", "ACK^^ACK", true),
        ("2.3.1", "DSR", true),
        ("2.3.1", "ORU", false),
        ("2.3.1", "ORU^^ORU_R01", false),
        ("2.3.1", "ORU^R01", true),
    ] {
        let raw = String::from_utf8(wire(version, "", "", "", ""))
            .expect("synthetic ASCII")
            .rewrite("ORU^R01^ORU_R01", kind)
            .into_bytes();
        let message = parse(&raw)?;
        assert_eq!(Msh::new(&message).is_ok(), valid, "{version}: {kind}");
        assert_eq!(message.encode(), raw);
    }
    Ok(())
}

// HL7 2.3.1 section 2.24.1.18 p.2-108; 2.5.1 section 2.15.9.18 pp.2-78-81.
// HL7 2.3.1 §2.24.1.18; HL7 2.5.1 §2.15.9.18
#[test]
fn charset_is_edition_specific() -> Result<(), Error> {
    let options = Options {
        charset: Charset::Utf8,
        ..Options::default()
    };
    let legacy = wire("2.3.1", "", "", "UNICODE UTF-8", "");
    assert!(matches!(
        Message::parse(&legacy, options),
        Err(Error::UnsupportedCharset)
    ));
    assert_eq!(
        Message::parse(
            &legacy,
            Options {
                allow_utf8_in_v231: true,
                ..options
            }
        )?
        .encode(),
        legacy
    );
    let modern = wire("2.5.1", "", "", "UNICODE UTF-8", "");
    assert_eq!(Message::parse(&modern, options)?.encode(), modern);
    for version in ["2.3.1", "2.5.1"] {
        for charset in ["", "ASCII"] {
            let raw = wire(version, "", "", charset, "");
            assert_eq!(parse(&raw)?.encode(), raw);
        }
        let raw = wire(version, "", "", "8859/1", "");
        assert_eq!(
            Message::parse(
                &raw,
                Options {
                    charset: Charset::Latin1,
                    ..Options::default()
                }
            )?
            .encode(),
            raw
        );
    }
    Ok(())
}

// HL7 2.5.1 §2.9.2.1 b, §2.9.3.2 b, §2.15.5 (table 0357 code 203): an MSH-12
// we do not recognise is refused by a 2.5.1 reply whose MSA-2 echoes MSH-10.
#[test]
fn unsupported_version_rejection_declares_fallback() -> Result<(), Error> {
    for (accept, application, code, msa) in [
        ("", "", AckCode::Ar, "MSA|AR|00017\r"),
        ("AL", "AL", AckCode::Cr, "MSA|CR|00017\r"),
    ] {
        let raw = wire("9.9", accept, application, "", "OBX|1|ST|SYN||caf\u{e9}\r");
        assert_eq!(
            parse(&raw).err(),
            Some(Error::Control {
                segment: "MSH",
                field: 12
            })
        );
        let header = Message::parse_header(&raw, Options::default())?;
        assert_eq!(header.segments().count(), 1);
        let msh = Msh::unsupported_version(&header)?.ok_or(Error::AckMode)?;
        let ack = rejection(
            msh,
            response()?,
            code,
            Responsibility::Stored,
            Some(AckError {
                segment: b"MSH",
                occurrence: 1,
                field: 12,
                code: UNSUPPORTED_VERSION,
                application_code: b"",
            }),
            Options::default(),
        )?
        .ok_or(Error::AckMode)?;
        let ack = String::from_utf8_lossy(ack.raw()).into_owned();
        assert!(
            ack.starts_with(
                "MSH|^~\\&|RECEIVER|RF|SENDER|SF|202601011201||ACK^R01^ACK|reply-1|P|2.5.1"
            ),
            "{ack}"
        );
        assert!(ack.contains(msa), "{ack}");
        assert!(ack.ends_with("ERR||MSH^1^12|203|E|\r"), "{ack}");
    }
    Ok(())
}

// HL7 2.5.1 §2.9.2: AA and CA claim acceptance, which an unsupported version never gets.
#[test]
fn rejection_never_builds_a_positive_code() -> Result<(), Error> {
    let header = Message::parse_header(&wire("9.9", "AL", "AL", "", ""), Options::default())?;
    let msh = Msh::unsupported_version(&header)?.ok_or(Error::AckMode)?;
    for code in [AckCode::Aa, AckCode::Ca] {
        assert_eq!(
            rejection(
                msh,
                response()?,
                code,
                Responsibility::StoredAndInterpreted,
                None,
                Options::default(),
            )
            .err(),
            Some(Error::AckMode)
        );
    }
    Ok(())
}

// HL7 2.5.1 §2.15.9.12: a recognised edition uses the ordinary header view.
#[test]
fn unsupported_version_view_refuses_recognised_editions() -> Result<(), Error> {
    for version in ["2.3.1", "2.5.1", "2.9.1"] {
        let header = Message::parse_header(&wire(version, "", "", "", ""), Options::default())?;
        assert!(Msh::unsupported_version(&header)?.is_none());
    }
    Ok(())
}
