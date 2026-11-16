//! Tests LAW order, timing and test-detail segments.
use super::*;

#[test]
fn law_tf2b_c8_orl_control_status_matrix_matches_each_request_type_exhaustively() -> TestResult {
    use law::{Address, Awos, Capabilities, WorkAction};
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = analyte_hl7::Response {
        control_id: law::control_id(b"m1")?,
        timestamp: b"20260101120000+0000",
    };
    for action in [WorkAction::New, WorkAction::Cancel] {
        let awos = Awos::new(
            b"awos-1",
            b"sample&Local",
            b"SER^^HL70487",
            b"P",
            b"tube",
            TEST.as_bytes(),
            action,
        )?;
        let sent = law::work_order(
            &[awos],
            None,
            address,
            response,
            Capabilities::default(),
            options(),
        )?;
        let valid = match action {
            WorkAction::New => vec![("OK", "SC"), ("OK", "IP"), ("OK", "CM"), ("UA", "CA")],
            WorkAction::Cancel => vec![("CR", "CA"), ("UC", "IP"), ("UC", "CM")],
        };
        for control in ["OK", "UA", "CR", "UC"] {
            for status in ["SC", "IP", "CM", "CA"] {
                let wire = format!(
                    "MSH|^~\\&|A|L|M|L|20260101120000+0000||ORL^O34^ORL_O42|response|P|2.5.1||||||UNICODE UTF-8|||LAB-28^IHE\rMSA|AA|m1\rSPM|1|||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rORC|{control}|awos-1|||{status}\r"
                );
                let incoming = law::parse_message(wire.as_bytes(), options())?;
                let parsed = law::WorkResponse::new(&incoming, &sent);
                assert_eq!(
                    parsed.is_ok(),
                    valid.contains(&(control, status)),
                    "{action:?}/{control}/{status}"
                );
                if let Ok(response) = parsed {
                    assert_eq!(
                        response.decisions().first().ok_or("decision")?.status,
                        OrderStatus::parse(status.as_bytes())?
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_c8_c6_c13_work_builder_preserves_supplied_event_and_validates_details() -> TestResult {
    use law::{Address, Awos, Capabilities, Instructions, WorkAction};
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = analyte_hl7::Response {
        control_id: law::control_id(b"m1")?,
        timestamp: b"20260101120000+0000",
    };
    let awos = Awos::new(
        b"awos-1",
        b"sample&Local",
        b"SER^^HL70487",
        b"P",
        b"tube",
        TEST.as_bytes(),
        WorkAction::New,
    )?
    .with_transaction_time(b"20251231235959");
    let capabilities = Capabilities {
        instructions: law::PeerInstructions {
            dilution: true,
            ..Capabilities::default().instructions
        },
        ..Capabilities::default()
    };
    assert!(law::work_order(&[awos], None, address, response, capabilities, options()).is_err());
    let supplied = Instructions {
        dilution: Some(b"^1^:^5.50"),
        ..Instructions::default()
    };
    let work = awos.with_instructions(supplied);
    let output = law::work_order(&[work], None, address, response, capabilities, options())?;
    let event = output
        .segments()
        .find(|segment| segment.id() == b"ORC")
        .and_then(|s| s.field(9))
        .ok_or("event")?;
    assert_eq!(event.raw(), b"20251231235959");
    assert!(
        law::work_order(
            &[work.with_transaction_time(b"20230229120000")],
            None,
            address,
            response,
            capabilities,
            options()
        )
        .is_err()
    );
    for invalid in [
        Instructions {
            dilution: Some(b"^1^:^0"),
            ..supplied
        },
        Instructions {
            dilution: Some(b"^1^+^5"),
            ..supplied
        },
        Instructions {
            dilution: Some(b"^1^:^5.50"),
            dilution_type: Some(b"D1^^99LAB"),
            ..Instructions::default()
        },
    ] {
        assert!(
            law::work_order(
                &[awos.with_instructions(invalid)],
                None,
                address,
                response,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    let bindings = [law::DilutionBinding::new(
        TEST.as_bytes(),
        b"D1^^99LAB",
        b"^1^:^5.50",
    )?];
    let with_type = awos.with_instructions(Instructions {
        dilution_type: Some(b"D1^^99LAB"),
        ..supplied
    });
    law::work_order_with_profile(
        &[with_type],
        None,
        address,
        response,
        capabilities,
        options(),
        Profile::for_peer(capabilities).with_dilution_bindings(&bindings),
    )?;
    validate_builder_repeat_permissions(awos, work, address, response, capabilities)?;
    Ok(())
}

#[test]
fn law_tf2b_c6_c8_c13_c14_x_fields_components_and_composite_ceiling() -> TestResult {
    for (id, supported) in [
        ("ORC", vec![1, 2, 4, 5, 8, 9, 21, 27]),
        ("OBR", vec![2, 3, 4, 11, 16, 49]),
        ("TCD", vec![1, 2, 3, 5, 6, 7, 8, 9, 10, 11]),
        ("TQ1", vec![9]),
    ] {
        for number in 1..=51 {
            if supported.contains(&number) {
                continue;
            }
            check(
                id,
                &[(number, "ignored&here~too")],
                Profile::receive(),
                ControlDirection::Receive,
            )?;
            assert!(
                check(
                    id,
                    &[(number, "ignored")],
                    Profile::receive(),
                    ControlDirection::Send
                )
                .is_err()
            );
        }
    }
    for (id, field, value, length) in [
        ("ORC", 4, "group", 50),
        ("ORC", 9, "20260101120000", 26),
        ("OBR", 2, "awos", 50),
        ("OBR", 4, TEST, 250),
        ("TCD", 1, TEST, 250),
        ("TCD", 2, "^1^:^5", 20),
        ("TCD", 3, "^1^:^5", 20),
        ("TCD", 5, "^1^:^5", 20),
        ("TCD", 11, "D1^^99LAB", 250),
        ("TQ1", 9, "R^^HL70485", 250),
    ] {
        let padded = format!("{value}{}", "^".repeat(length - value.len()));
        check(
            id,
            &[(field, &padded)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
        assert!(
            check(
                id,
                &[(field, &format!("{padded}^"))],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    for (id, field, value) in [
        ("ORC", 4, "group&ignored^filler"),
        ("ORC", 9, "20260101120000^ignored"),
        ("ORC", 21, "Name^ignored"),
        ("OBR", 2, "awos^ignored"),
        ("OBR", 4, "test^Synthetic^99LAB^ignored"),
        ("TCD", 1, "test^Synthetic^99LAB^ignored"),
        ("TCD", 2, "ignored^1^:^5^ignored"),
        ("TCD", 11, "D1^^99LAB^ignored"),
        ("TQ1", 9, "R^^HL70485^ignored"),
    ] {
        check(
            id,
            &[(field, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
        assert!(
            check(
                id,
                &[(field, value)],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    for value in ["1^mL&&UCUM&ignored", "1^mL&&UCUM^extra"] {
        manager("TCD", &[(9, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c13_tcd_8_required_text_and_9_nested_cq_limits() -> TestResult {
    let profile = Profile::receive().with_repeat_options(true, false);
    check(
        "TCD",
        &[(8, &format!("O^{}^HL70389", "x".repeat(199)))],
        profile,
        ControlDirection::Send,
    )?;
    for text in [String::new(), "x".repeat(200), String::from("\"\"")] {
        assert!(
            check(
                "TCD",
                &[(8, &format!("O^{text}^HL70389"))],
                profile,
                ControlDirection::Send
            )
            .is_err()
        );
    }
    let units = format!("{}&{}&UCUM", "x".repeat(20), "x".repeat(199));
    let value = format!("{}^{units}", "1".repeat(16));
    assert_eq!(units.len(), 225);
    assert_eq!(value.len(), 242);
    manager("TCD", &[(9, &value)], true)?;
    manager("TCD", &[(9, &format!("{value}^"))], false)?;
    assert!(matches!(
        check(
            "TCD",
            &[(9, &format!("{value}&"))],
            Profile::receive(),
            ControlDirection::Send
        )
        .err()
        .and_then(|e| e.downcast::<Error>().ok())
        .as_deref(),
        Some(Error::Component {
            segment: "TCD",
            field: 9,
            component: 2
        })
    ));
    for (identifier, text) in [
        ("x".repeat(21), "x".repeat(199)),
        ("x".repeat(20), "x".repeat(200)),
    ] {
        manager("TCD", &[(9, &format!("1^{identifier}&{text}&UCUM"))], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c13_safety_rule_6_type_only_dilution_has_adjustment_metadata() -> TestResult {
    for adjustment in [
        law::ResultAdjustment::AlreadyApplied,
        law::ResultAdjustment::NotApplied,
        law::ResultAdjustment::Unknown,
    ] {
        let message = message("TCD", ControlDirection::Receive, &[(11, "D1^^99LAB")])?;
        let detail = law::TestDetail::new(
            message.segments().nth(1).ok_or("fixture")?,
            Profile::receive().with_result_adjustment(adjustment),
            ControlDirection::Receive,
        )?;
        assert!(detail.auto_dilution().is_none());
        assert_eq!(detail.dilution_type(), Some(b"D1^^99LAB".as_slice()));
        assert_eq!(detail.result_adjustment(), Some(adjustment));
        assert_eq!(
            detail.requires_reconciliation(),
            adjustment == law::ResultAdjustment::Unknown
        );
        let ignored = law::TestDetail::new(
            detail.segment(),
            Profile::receive().with_dilution(false),
            ControlDirection::Receive,
        )?;
        assert!(ignored.dilution_type().is_none());
        assert_eq!(ignored.result_adjustment(), None);
        assert!(!ignored.requires_reconciliation());
        assert_eq!(ignored.segment().raw(), detail.segment().raw());
    }
    Ok(())
}
