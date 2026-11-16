//! Tests LAW OBX, NTE and INV segment validation.
use super::*;

#[test]
fn law_tf2b_c2_c5_c7_x_fields_components_and_composite_send_ceilings() -> TestResult {
    for (id, supported) in [
        (
            "OBX",
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 18, 19, 21, 29],
        ),
        ("NTE", vec![1, 2, 3, 4]),
        ("INV", vec![1, 2, 3, 4, 12, 16]),
    ] {
        for number in 1..40 {
            if !supported.contains(&number) {
                check(
                    id,
                    &[(number, "ignored~content&here")],
                    Profile::receive(),
                    ControlDirection::Receive,
                )?;
                assert!(
                    check(
                        id,
                        &[(number, "content")],
                        Profile::receive(),
                        ControlDirection::Send
                    )
                    .is_err()
                );
            }
        }
    }
    for (id, number, value, maximum) in [
        ("OBX", 3, TEST, 250_usize),
        ("OBX", 4, "1", 20),
        ("OBX", 6, "id^Text^UCUM", 250),
        ("OBX", 16, "operator", 250),
        ("OBX", 21, "instance", 427),
        ("NTE", 4, "note^^99LAB", 250),
    ] {
        let padded = format!(
            "{value}{}",
            "^".repeat(maximum.checked_sub(value.len()).ok_or("fixture")?)
        );
        check(
            id,
            &[(number, &padded)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
        assert!(
            check(
                id,
                &[(number, &format!("{padded}^"))],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    for (number, value) in [
        (3, "test^Synthetic^99LAB^^^^ignored"),
        (5, "code^Text^system^ignored"),
        (6, "id^Text^UCUM^ignored"),
        (16, "operator^ignored"),
        (21, "instance^ignored"),
    ] {
        let changes = if number == 5 {
            vec![(2, "CE"), (5, value), (6, "")]
        } else {
            vec![(number, value)]
        };
        check(
            "OBX",
            &changes,
            Profile::receive(),
            ControlDirection::Receive,
        )?;
        assert!(check("OBX", &changes, Profile::receive(), ControlDirection::Send).is_err());
    }
    Ok(())
}

#[test]
fn law_tf2b_c5_nte_4_extended_vendor_namespace_has_12_character_limit() -> TestResult {
    both("NTE", &[(4, "note^^99ABCDEFGHIJ")], true)?;
    both("NTE", &[(4, "note^^99ABCDEFGHIJK")], false)?;
    both("NTE", &[(4, "note^^99A!B")], false)?;
    Ok(())
}

#[test]
fn law_tf2b_c7_remaining_component_boundaries_and_located_failures() -> TestResult {
    for value in [
        format!("{TEST}^alt^{}^LN", "x".repeat(199)),
        format!("{TEST}^^^^^^^alt^{}^LN", "x".repeat(199)),
        format!("{TEST}^^^^^^^{}^Text^99LAB", "i".repeat(20)),
    ] {
        both("OBX", &[(3, &value)], true)?;
    }
    both(
        "OBX",
        &[(3, &format!("{TEST}^^^^^^^alt^{}^LN", "x".repeat(200)))],
        false,
    )?;
    for value in [
        format!("=^1^:^{}", "1".repeat(15)),
        "<^1^+".to_owned(),
        "<^1^^2".to_owned(),
    ] {
        both("OBX", &[(2, "SN"), (5, &value)], true)?;
    }
    both(
        "OBX",
        &[(2, "ED"), (5, "^multipart^^A^synthetic"), (6, "")],
        true,
    )?;
    both(
        "OBX",
        &[
            (2, "RP"),
            (
                5,
                &format!(
                    "pointer^&{}&URI^{}^{}",
                    "u".repeat(199),
                    "t".repeat(9),
                    "s".repeat(19)
                ),
            ),
            (6, ""),
        ],
        true,
    )?;
    for (number, value, component) in [
        (3, "id^^LN", 2),
        (16, "operator12345678", 1),
        (19, "20230229120000", 1),
        (18, "model^maker^uid^BAD~serial^maker", 4),
    ] {
        let message = message(&fields(
            "OBX",
            ControlDirection::Receive,
            &[(number, value)],
        )?)?;
        assert!(
            matches!(Profile::receive().validate_segment(message.segments().nth(1).ok_or("fixture")?,ControlDirection::Receive),Err(Error::Component{segment:"OBX",field,component: actual}) if field==number&&actual==component)
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c2_c5_c7_ignored_nested_components_and_remaining_lengths() -> TestResult {
    for (number, value) in [(2, "custom^^99LAB"), (3, "custom^^99LAB")] {
        check(
            "INV",
            &[(number, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
        check(
            "INV",
            &[(
                number,
                &format!("{}^{}^99LAB", "i".repeat(20), "x".repeat(199)),
            )],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
        assert!(
            check(
                "INV",
                &[(number, &format!("id^{}^99LAB", "x".repeat(200)))],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    check(
        "NTE",
        &[(4, "note^^99LAB^ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "NTE",
            &[(4, "note^^99LAB^ignored")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    for number in [1, 2, 3, 4] {
        check(
            "INV",
            &[(number, "id^^99LAB^ignored")],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    let equipment = format!("model^maker{}~serial^maker", "^".repeat(416));
    check(
        "OBX",
        &[(18, &equipment)],
        Profile::receive(),
        ControlDirection::Send,
    )?;
    assert!(
        check(
            "OBX",
            &[(18, &equipment.replacen('~', "^~", 1))],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c5_work_builder_validates_and_keeps_truncatable_notes() -> TestResult {
    let text = "x".repeat(300);
    let notes = [text.as_bytes()];
    let work_notes = law::WorkNotes {
        order: &notes,
        ..law::WorkNotes::default()
    };
    let awos = law::Awos::new(
        b"awos-1",
        b"sample&Local",
        b"SER^^HL70487",
        b"P",
        b"tube",
        TEST.as_bytes(),
        law::WorkAction::New,
    )?
    .with_notes(&work_notes);
    let address = law::Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = analyte_hl7::Response {
        control_id: law::control_id(b"w1")?,
        timestamp: b"20260101120000+0000",
    };
    let output = law::work_order(
        &[awos],
        None,
        address,
        response,
        law::Capabilities::default(),
        options(),
    )?;
    assert_eq!(
        output
            .segments()
            .find(|s| s.id() == b"NTE")
            .and_then(|s| s.field(3))
            .map(analyte_hl7::Field::raw),
        Some(text.as_bytes())
    );
    let invalid = [b"one~two".as_slice()];
    let bad_notes = law::WorkNotes {
        order: &invalid,
        ..law::WorkNotes::default()
    };
    assert!(
        law::work_order(
            &[awos.with_notes(&bad_notes)],
            None,
            address,
            response,
            law::Capabilities::default(),
            options()
        )
        .is_err()
    );
    Ok(())
}
