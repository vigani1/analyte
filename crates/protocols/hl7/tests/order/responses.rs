//! Tests HL7 order messages and acknowledgments.
use super::*;
use analyte_fixtures::ConfigEdit as _;

#[test]
fn hl7_231_2_7_and_251_2_5_4_response_delimiters_are_independent() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let edition = match version {
            Version::V23
            | Version::V24
            | Version::V25
            | Version::V26
            | Version::V282
            | Version::V291 => unreachable!("fixture enumerates only 2.3.1 and 2.5.1"),
            Version::V231 => "2.3.1",
            Version::V251 => "2.5.1",
        };
        let bytes = format!(
            "MSH!$%?@!MACHINE!LAB!HOST!LAB!20261115!!ORR$O02$ORR_O02!reply!T!{edition}\rMSA!AA!sent1\rORC!OK!001$LAB$$!F1$M\rORC!CR!002$LAB!F2$M\r"
        );
        let response = Message::parse(bytes.as_bytes(), Options::default())?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert!(matches!(
            parsed.decisions().first().map(order::Decision::outcome),
            Some(Outcome::Accepted)
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_5_and_251_4_5_1_5_snapshot_does_not_change_request_outcome() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let response = reply(version, b"AA", b"ORC|OK|001^LAB|||CM\r")?;
        let parsed = OrderResponse::new(&response, &sent)?;
        let decision = parsed.decisions().first().ok_or(Error::Limit)?;
        assert!(matches!(decision.outcome(), Outcome::Accepted));
        assert_eq!(
            decision
                .common()
                .and_then(|s| s.field(5))
                .map(analyte_hl7::Field::raw),
            Some(b"CM".as_slice())
        );
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_16_and_251_4_5_1_16_rejection_reason_validation() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        for reason in [b"\"\"".as_slice(), b"A~B", b"A^B^C^D^E^F^G", b"A&B"] {
            let response = reply(version, b"AA", &orc(b"UA", b"001^LAB", b"", b"", reason))?;
            assert!(matches!(
                OrderResponse::new(&response, &sent),
                Err(Error::Content { field: 16, .. })
            ));
        }
        let response = reply(
            version,
            b"AE",
            &orc(b"UA", b"001^LAB", b"", b"", b"LOCAL^Rejected^L"),
        )?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert_eq!(parsed.acceptance(), AckCode::Ae);
        assert!(matches!(
            parsed.decisions().first().map(order::Decision::outcome),
            Some(Outcome::Rejected { .. })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_2_and_251_4_4_2_requires_orr_and_msa() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let response = reply(version, b"AA", b"ORC|OK|001^LAB\r")?;
        let text = String::from_utf8(response.raw().to_vec()).map_err(|_| Error::Limit)?;
        let changed = Message::parse(
            text.rewrite("ORR^O02^ORR_O02", "ACK^O01^ACK").as_bytes(),
            Options::default(),
        )?;
        assert!(matches!(
            OrderResponse::new(&changed, &sent),
            Err(Error::Content { field: 9, .. })
        ));
        let changed = Message::parse(
            text.rewrite("MSA|AA|sent1\r", "").as_bytes(),
            Options::default(),
        )?;
        assert!(matches!(
            OrderResponse::new(&changed, &sent),
            Err(Error::Syntax(_))
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_2_8_and_251_2a_caller_data_rejects_injection_and_oversize() -> Result<(), Error> {
    assert!(OrderId::new(b"001|ORC", b"LAB").is_err());
    assert!(OrderId::new(b"001", b"").is_err());
    assert!(OrderId::new(b"1234567890123456789012", b"LAB").is_err());
    assert!(OrderId::with_universal(b"001", b"", Some((b"U", b"ZZZ"))).is_err());
    assert!(Code::new(b"T1", b"Injected\rOBR", b"L").is_err());
    assert!(Code::new(b"T1", b"Synthetic", b"").is_err());
    assert!(SampleId::new(b"").is_err());
    assert!(SampleId::new(b"000\r1").is_err());
    assert!(Timestamp::new(b"20260230").is_err());
    for version in [Version::V231, Version::V251] {
        let sample = vec![b'|'; 21];
        let input = new_order(&sample, SampleField::Placer1)?;
        assert!(matches!(
            order::orm(
                &[input],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
            Err(Error::Content { field: 18, .. })
        ));
        let mut injected = header(version)?;
        injected.control_id = ControlId::new(b"bad|id")?;
        assert!(matches!(
            order::orm(
                &[new_order(b"000aB", SampleField::Placer1)?],
                Subject::NonPatient,
                injected,
                Options::default()
            ),
            Err(Error::Content { field: 10, .. })
        ));
    }
    Ok(())
}

proptest::proptest! {
    #[test]
    fn hl7_231_4_3_1_1_and_251_4_5_1_1_arbitrary_control_codes_never_default_to_acceptance(value in "[A-Z]{2}") {
        let parsed=Control::parse(value.as_bytes());
        match parsed {
            Ok(control)=>proptest::prop_assert_eq!(control.wire(),value.as_bytes()),
            Err(error)=>{
                proptest::prop_assert!(matches!(error,Error::Control));
                proptest::prop_assert!(!["NW","CA","OK","UA","CR","UC"].contains(&value.as_str()));
            },
        }
    }
}

bad_order_reply!(
    hl7_231_4_2_2_and_251_4_4_2_multiple_details_are_not_flattened,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rOBR|2|001^LAB||T2^Other^L\r",
    Error::Content { field: 0, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_bad_second_order_rejects_response_atomically,
    b"ORC|OK|001^LAB\rORC|ZZ|002^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_2_and_251_4_5_1_2_no_numeric_placer_normalization,
    b"ORC|OK|1^LAB\r",
    Error::Correlation {
        segment: "ORC",
        field: 2
    }
);

#[test]
fn hl7_231_4_2_2_and_251_4_4_2_header_error_and_patient_context_retained() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let body=match version {
            Version::V23 | Version::V24 | Version::V25 | Version::V26 | Version::V282 | Version::V291 => unreachable!("fixture enumerates only 2.3.1 and 2.5.1"),
            Version::V231=>b"ERR|ORC^1^1^102\rNTE|1||Header note\rPID|1||P1^^^LAB||Synthetic\rNTE|1||Patient note\rORC|UA|001^LAB\r".as_slice(),
            Version::V251=>b"ERR||ORC^1^1|102|E\rNTE|1||Header note\rPID|1||P1^^^LAB||Synthetic\rNTE|1||Patient note\rORC|UA|001^LAB\r",
        };
        let response = reply(version, b"AE", body)?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert_eq!(parsed.acceptance(), AckCode::Ae);
        assert_eq!(parsed.message().raw(), response.raw());
        assert!(
            parsed
                .decisions()
                .first()
                .is_some_and(|d| d.notes().is_empty())
        );
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_1_and_251_4_4_1_nonpatient_cancel_needs_no_invented_detail() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = order::orm(
            &[Order::Cancel {
                placer: OrderId::new(b"001", b"LAB")?,
                filler: None,
            }],
            Subject::NonPatient,
            header(version)?,
            Options::default(),
        )?;
        assert_eq!(sent.message().segments().count(), 2);
        assert!(sent.message().raw().ends_with(b"ORC|CA|001^LAB|\r"));
        assert!(
            sent.message()
                .segments()
                .all(|s| s.id() != b"OBR" && s.id() != b"PID")
        );
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_1_18_and_251_2_15_9_18_utf8_sample_is_retained() -> Result<(), Error> {
    let sample = "000éB".as_bytes();
    let message = order::orm(
        &[new_order(sample, SampleField::Placer1)?],
        Subject::NonPatient,
        header(Version::V251)?,
        Options {
            charset: analyte_hl7::Charset::Utf8,
            ..Options::default()
        },
    )?;
    let field = message
        .message()
        .segments()
        .find(|s| s.id() == b"OBR")
        .and_then(|s| s.field(18))
        .ok_or(Error::Limit)?;
    assert_eq!(field.raw(), sample);
    Ok(())
}

#[test]
fn hl7_231_7_3_1_3_and_251_7_4_1_3_duplicate_new_filler_assignments_rejected() -> Result<(), Error>
{
    for version in [Version::V231, Version::V251] {
        let orders = [
            Order::Cancel {
                placer: OrderId::new(b"001", b"LAB")?,
                filler: None,
            },
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: None,
            },
        ];
        let sent = order::orm(
            &orders,
            Subject::NonPatient,
            header(version)?,
            Options::default(),
        )?;
        let response = reply(
            version,
            b"AA",
            b"ORC|CR|001^LAB|F1^M\rORC|CR|002^LAB|F1^M\r",
        )?;
        assert!(matches!(
            OrderResponse::new(&response, &sent),
            Err(Error::Duplicate { field: 3 })
        ));
        let first = OrderId::with_universal(b"001", b"A", Some((b"U", b"ISO")))?;
        let second = OrderId::with_universal(b"001", b"B", Some((b"U", b"ISO")))?;
        assert!(matches!(
            order::orm(
                &[
                    Order::Cancel {
                        placer: first,
                        filler: None
                    },
                    Order::Cancel {
                        placer: second,
                        filler: None
                    }
                ],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
            Err(Error::Duplicate { field: 2 })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_3_and_251_4_4_3_unsupported_order_families_are_located_errors() -> Result<(), Error>
{
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let message = reply(version, b"AA", b"ORC|OK|001^LAB\r")?;
        let text = String::from_utf8(message.raw().to_vec()).map_err(|_| Error::Limit)?;
        let families: &[&str] = match version {
            Version::V23
            | Version::V24
            | Version::V25
            | Version::V26
            | Version::V282
            | Version::V291 => unreachable!("fixture enumerates only 2.3.1 and 2.5.1"),
            Version::V231 => &["OSR^Q06^OSR_Q06"],
            Version::V251 => &[
                "OSR^Q06^OSR_Q06",
                "ORG^O20^ORG_O20",
                "ORL^O36^ORL_O36",
                "ORI^O24^ORI_O24",
            ],
        };
        for family in families {
            let wrong = Message::parse(
                text.rewrite("ORR^O02^ORR_O02", family).as_bytes(),
                Options::default(),
            )?;
            assert!(matches!(
                OrderResponse::new(&wrong, &sent),
                Err(Error::Content { field: 9, .. })
            ));
            assert_eq!(
                wrong.raw(),
                text.rewrite("ORR^O02^ORR_O02", family).as_bytes()
            );
        }
    }
    for code in [
        b"SS".as_slice(),
        b"SR",
        b"XO",
        b"XR",
        b"UX",
        b"DC",
        b"DR",
        b"UD",
    ] {
        assert!(matches!(Control::parse(code), Err(Error::Control)));
    }
    Ok(())
}

#[test]
fn hl7_231_ch4_explicit_group_position_and_identifier_do_not_invent_context() -> Result<(), Error> {
    let mut request = new_order(b"000aB", SampleField::Placer1)?;
    if let Order::New { specimen, .. } = &mut request {
        specimen.source = None;
    }
    // HL7 2.3.1 section 4.2.1.1(d): known-patient new work cannot omit PID.
    assert!(
        order::orm(
            &[request],
            Subject::Identifier {
                identifier: b"000Patient",
                namespace: b"LAB"
            },
            header(Version::V231)?,
            Options::default()
        )
        .is_err()
    );
    let built = order::orm(
        &[request],
        Subject::NonPatient,
        header(Version::V231)?,
        Options::default(),
    )?;
    let enriched = order::context(
        &built,
        b"app-order",
        Some((SampleField::Placer2, b"007b")),
        Options::default(),
    )?;
    let common = enriched
        .message()
        .segments()
        .find(|s| s.id() == b"ORC")
        .ok_or(Error::Limit)?;
    assert_eq!(common.field(4).ok_or(Error::Limit)?.raw(), b"app-order^LAB");
    let detail = enriched
        .message()
        .segments()
        .find(|s| s.id() == b"OBR")
        .ok_or(Error::Limit)?;
    assert_eq!(detail.field(15).ok_or(Error::Limit)?.raw(), b"");
    assert_eq!(detail.field(18).ok_or(Error::Limit)?.raw(), b"000aB");
    assert_eq!(detail.field(19).ok_or(Error::Limit)?.raw(), b"007b");
    // HL7 2.3.1 section 3.3.2 p.3-36: PID-5 is required when PID is present.
    assert!(!enriched.message().segments().any(|s| s.id() == b"PID"));
    assert!(
        order::context(
            &built,
            b"app-order",
            Some((SampleField::Placer1, b"overwrite")),
            Options::default()
        )
        .is_err()
    );
    assert!(
        order::context(
            &built,
            b"app-order-identifier-too-long",
            None,
            Options::default()
        )
        .is_err()
    );
    assert!(
        order::oml(
            &[request],
            Subject::Unspecified,
            header(Version::V251)?,
            Options::default()
        )
        .is_err()
    );
    Ok(())
}

// HL7 2.3.1 section 4.3.1.4 and 2.5.1 section 4.5.1.4: same authority as ORC-2.
#[test]
fn hl7_231_4_3_1_4_251_4_5_1_4_group_ei_authority_and_complete_field_limit() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        for universal in [None, Some((b"1.2".as_slice(), b"ISO".as_slice()))] {
            let mut request = new_order(b"sample", SampleField::Placer1)?;
            if let Order::New { placer, .. } = &mut request {
                *placer = OrderId::with_universal(b"x", b"LAB", universal)?;
            }
            let built = order::orm(
                &[request],
                Subject::Unspecified,
                header(version)?,
                Options::default(),
            )?;
            let authority = if universal.is_some() {
                "^LAB^1.2^ISO"
            } else {
                "^LAB"
            };
            let group = "g".repeat(22_usize.checked_sub(authority.len()).ok_or(Error::Limit)?);
            let enriched = order::context(&built, group.as_bytes(), None, Options::default())?;
            let common = enriched
                .message()
                .segments()
                .find(|s| s.id() == b"ORC")
                .ok_or(Error::Limit)?;
            let expected = format!("{group}{authority}");
            assert_eq!(
                common.field(4).ok_or(Error::Limit)?.raw(),
                expected.as_bytes()
            );
            let overflow = format!("{group}g");
            assert!(matches!(
                order::context(&built, overflow.as_bytes(), None, Options::default()),
                Err(Error::Content { field: 4, .. })
            ));
        }
    }
    Ok(())
}

// HL7 2.5.1 section 3.4.2 and 4.4.2: optional PID has required contents when present.
#[test]
fn hl7_251_s3_4_2_present_reply_pid_requires_identifier_and_name() -> Result<(), Error> {
    let sent = sent(Version::V251)?;
    for pid in [
        b"PID\r".as_slice(),
        b"PID|1||P1\r",
        b"PID|1||\"\"||Synthetic\r",
        b"PID|1||P1||\"\"\r",
    ] {
        let mut body = pid.to_vec();
        body.extend_from_slice(b"ORC|OK|001^LAB\r");
        assert!(OrderResponse::new(&reply(Version::V251, b"AA", &body)?, &sent).is_err());
    }
    let message = reply(
        Version::V251,
        b"AA",
        b"PID|1||P1^^^LAB||Synthetic\rORC|OK|001^LAB\r",
    )?;
    OrderResponse::new(&message, &sent)?;
    Ok(())
}
