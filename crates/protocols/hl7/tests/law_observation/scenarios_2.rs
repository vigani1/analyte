//! Tests LAW OBX, NTE and INV segment validation.
use super::*;

#[test]
fn law_tf2b_c7_obx_11_closed_status_and_explicit_correction_only() -> TestResult {
    for status in [
        ResultStatus::Final,
        ResultStatus::Preliminary,
        ResultStatus::Corrected,
        ResultStatus::Unverified,
        ResultStatus::Unobtainable,
    ] {
        assert_eq!(ResultStatus::parse(status.as_bytes())?, status);
        let code = std::str::from_utf8(status.as_bytes())?;
        both("OBX", &[(11, code)], true)?;
        let message = result_message(&[(11, code)], "")?;
        let result = law::Results::new(&message)?;
        assert_eq!(
            result.observations().first().map(law::Observation::status),
            Some(status)
        );
    }
    for value in ["", "\"\"", "D", "W", "U", "S", "I", "N", "O", "F~C", "F^C"] {
        both("OBX", &[(11, value)], false)?;
        assert!(ResultStatus::parse(value.as_bytes()).is_err());
    }
    // A changed final value remains F. Only C declares a correction.
    for value in ["1.20", "1.30"] {
        let message = result_message(&[(5, value)], "")?;
        assert_eq!(
            law::Results::new(&message)?
                .observations()
                .first()
                .map(law::Observation::status),
            Some(ResultStatus::Final)
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_14_19_seconds_timestamps_and_direction() -> TestResult {
    for value in ["20240229120000", "20260101120000"] {
        both("OBX", &[(19, value)], true)?;
        check(
            "OBX",
            &[(14, value)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
    }
    for value in [
        "",
        "\"\"",
        "20230229120000",
        "20260101",
        "20260101120000+0000",
        "20260101120000~20260101120000",
    ] {
        both("OBX", &[(19, value)], false)?;
        if !value.is_empty() {
            assert!(
                check(
                    "OBX",
                    &[(14, value)],
                    Profile::receive(),
                    ControlDirection::Send
                )
                .is_err()
            );
        }
        check(
            "OBX",
            &[(14, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    check(
        "OBX",
        &[(19, "20260101120000^S")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(19, "20260101120000^S")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_16_xcn_null_observer_and_repetition_limits() -> TestResult {
    for value in ["operator", "\"\"", "operator~\"\"", &"\u{e9}".repeat(15)] {
        both("OBX", &[(16, value)], true)?;
    }
    for value in ["", "operator~", "id&bad", &"x".repeat(16)] {
        both("OBX", &[(16, value)], false)?;
    }
    check(
        "OBX",
        &[(16, "operator^ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(16, "operator^ignored")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_18_first_second_later_equipment_ei_rules() -> TestResult {
    for value in [
        "model^maker~serial^maker",
        "model^maker^1.2.3^ISO~serial^maker~site",
        "model^maker~serial^maker~^site",
        "model^maker~serial^maker~",
    ] {
        both("OBX", &[(18, value)], true)?;
    }
    for value in [
        "",
        "model^maker",
        "^maker~serial^maker",
        "model~serial^maker",
        "model^maker~^maker",
        "model^maker~serial",
        "model^maker^uid^BAD~serial^maker",
    ] {
        both("OBX", &[(18, value)], false)?;
    }
    both(
        "OBX",
        &[(
            18,
            &format!(
                "{}^{}^{}^ISO~serial^maker",
                "\u{e9}".repeat(50),
                "\u{e9}".repeat(20),
                "u".repeat(199)
            ),
        )],
        true,
    )?;
    for value in [
        format!("{}^maker~serial^maker", "x".repeat(51)),
        format!("model^{}~serial^maker", "x".repeat(21)),
        format!("model^maker^{}^ISO~serial^maker", "x".repeat(200)),
        format!("model^maker~serial^maker~{}", "x".repeat(51)),
    ] {
        both("OBX", &[(18, &value)], false)?;
    }
    check(
        "OBX",
        &[(18, "model^maker~serial^maker^ignored^ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(18, "model^maker~serial^maker^uid^ISO")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_21_available_ei_identity_and_29_type_subset() -> TestResult {
    for value in ["", "instance", &"\u{e9}".repeat(50)] {
        both("OBX", &[(21, value)], true)?;
    }
    for value in ["\"\"", "^namespace", "one~two", "id&bad", &"x".repeat(51)] {
        both("OBX", &[(21, value)], false)?;
    }
    check(
        "OBX",
        &[(21, "instance^ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(21, "instance^namespace")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    for value in ["AOE", "ASC", "RSLT", "SCI"] {
        both("OBX", &[(29, value)], true)?;
    }
    for value in ["", "\"\"", "OTHER", "RSLT~SCI", "RSLT^SCI"] {
        both("OBX", &[(29, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c5_nte_required_singletons_source_and_truncatable_comment() -> TestResult {
    both("NTE", &[], true)?;
    both("NTE", &[(1, "9999"), (3, &"\u{e9}".repeat(300))], true)?;
    for value in ["", "0", "-1", "10000", "\"\"", "1^2"] {
        both("NTE", &[(1, value)], false)?;
    }
    for value in ["", "\"\"", "OTHER", "A~Z"] {
        both("NTE", &[(2, value)], false)?;
    }
    for value in ["", "\"\"", "one~two", "one^two"] {
        both("NTE", &[(3, value)], false)?;
    }
    assert!(
        check(
            "NTE",
            &[(2, "Z")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    assert!(
        check(
            "NTE",
            &[(2, "A")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c5_nte_4_re_ce_components_and_vendor_system() -> TestResult {
    for value in [
        "",
        "note^^99LAB",
        &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
    ] {
        both("NTE", &[(4, value)], true)?;
    }
    for value in [
        "\"\"",
        "code",
        "^Text^99LAB",
        "id^Text^HL70001",
        "id^Text^99AB",
        "one^^99LAB~two^^99LAB",
        &format!("{}^^99LAB", "x".repeat(21)),
        &format!("id^{}^99LAB", "x".repeat(200)),
    ] {
        both("NTE", &[(4, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c5_nte_sequence_is_local_to_consecutive_group() -> TestResult {
    let first = fields("NTE", ControlDirection::Receive, &[])?;
    let second = fields("NTE", ControlDirection::Receive, &[(1, "2")])?;
    let good = result_message(&[], &format!("{first}{second}"))?;
    law::Results::new(&good)?;
    let bad = result_message(&[], &format!("{first}{first}"))?;
    let failure = law::Results::new(&bad)
        .err()
        .ok_or("accepted invalid NTE")?;
    assert!(
        matches!(failure, Error::AtSegment {occurrence:2,source} if matches!(*source,Error::Content {segment:"NTE",field:1}))
    );
    let message = message(&format!(
        "{first}{}{}",
        fields("OBX", ControlDirection::Receive, &[])?,
        first
    ))?;
    Profile::receive().validate_message(&message, ControlDirection::Receive)?;
    Ok(())
}

#[test]
fn law_tf2b_c2_inv_required_ce_and_2_2_3_identifier_lot_conformance() -> TestResult {
    check("INV", &[], Profile::receive(), ControlDirection::Receive)?;
    check(
        "INV",
        &[
            (
                1,
                &format!("{}^{}^99LAB", "\u{e9}".repeat(50), "\u{e9}".repeat(199)),
            ),
            (16, &"\u{e9}".repeat(50)),
        ],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    for field in [1, 2, 3] {
        for value in [
            "",
            "\"\"",
            "code",
            "^Text^99LAB",
            "id^Text^OTHER",
            "id^Text^99AB",
            "one^^99LAB~two^^99LAB",
        ] {
            assert!(
                check(
                    "INV",
                    &[(field, value)],
                    Profile::receive(),
                    ControlDirection::Receive
                )
                .is_err()
            );
        }
    }
    for (field, value) in [
        (1, format!("{}^^99LAB", "x".repeat(51))),
        (1, format!("id^{}^99LAB", "x".repeat(200))),
        (2, format!("{}^^99LAB", "x".repeat(21))),
        (3, format!("{}^^99LAB", "x".repeat(21))),
        (16, "x".repeat(51)),
        (16, String::from("\"\"")),
        (16, String::from("one~two")),
    ] {
        assert!(
            check(
                "INV",
                &[(field, &value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    for value in ["OK^^HL70383", "NA^^HL70383", "custom^^99LAB"] {
        check(
            "INV",
            &[(2, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    for value in ["CO^^HL70384", "MR^^HL70384", "custom^^99LAB"] {
        check(
            "INV",
            &[(3, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    assert!(
        check(
            "INV",
            &[(2, "BAD^^HL70383")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    assert!(
        check(
            "INV",
            &[(3, "BAD^^HL70384")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c2_inv_4_12_re_an_container_expiry_and_option_direction() -> TestResult {
    check(
        "INV",
        &[(4, "package^^99LAB"), (12, "20240229120000")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    check(
        "INV",
        &[(
            4,
            &format!("{}^{}^99LAB", "i".repeat(20), "\u{e9}".repeat(199)),
        )],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    for value in [
        "\"\"",
        "id^^HL70451",
        "^Text^99LAB",
        "one^^99LAB~two^^99LAB",
        &format!("{}^^99LAB", "i".repeat(21)),
        &format!("id^{}^99LAB", "x".repeat(200)),
    ] {
        assert!(
            check(
                "INV",
                &[(4, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    for value in [
        "\"\"",
        "20230229120000",
        "20260101",
        "20260101120000+0000",
        "20260101120000~20260101120000",
    ] {
        assert!(
            check(
                "INV",
                &[(12, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    check(
        "INV",
        &[(12, "20260101120000^S")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(check("INV", &[], Profile::receive(), ControlDirection::Send).is_err());
    check(
        "INV",
        &[(1, "ignored~values")],
        Profile::receive().with_contributing_substances(false),
        ControlDirection::Receive,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c2_control_inventory_co_predicate_and_result_ownership() -> TestResult {
    let inventory = fields("INV", ControlDirection::Receive, &[(3, "CO^^HL70384")])?;
    let result = result_message(&[], &inventory)?;
    law::Results::new(&result)?;
    let base = std::str::from_utf8(result.raw())?;
    let source = base
        .replace(&inventory, "")
        .replace("SER^^HL70487", "\"\"")
        .replace("P^^HL70369", "Q^^HL70369")
        .replace("SAC|||tube\r", &format!("SAC|||tube\r{inventory}"));
    let good = law::parse_message(source.as_bytes(), options())?;
    law::Results::new(&good)?;
    let bad = law::parse_message(
        source.replace("CO^^HL70384", "MR^^HL70384").as_bytes(),
        options(),
    )?;
    let failure = law::Results::new(&bad)
        .err()
        .ok_or("accepted invalid INV")?;
    assert!(
        matches!(failure, Error::AtSegment {occurrence:1,source} if matches!(*source,Error::Content {segment:"INV",field:3}))
    );
    let bad = law::parse_message(
        source.replace("Q^^HL70369", "Q^^99LAB").as_bytes(),
        options(),
    )?;
    assert!(law::Results::new(&bad).is_err());
    Ok(())
}

#[test]
fn law_tf2b_c7_same_primary_identity_requires_og_group_sequence() -> TestResult {
    let second = fields(
        "OBX",
        ControlDirection::Receive,
        &[(1, "2"), (3, "test^Changed description^99LAB"), (4, "1")],
    )?;
    let bad = result_message(&[], &second)?;
    assert!(matches!(
        law::Results::new(&bad),
        Err(Error::Content {
            segment: "OBX",
            field: 4
        })
    ));
    let second = fields("OBX", ControlDirection::Receive, &[(1, "2"), (4, "1^1^2")])?;
    let good = result_message(&[(4, "1^1^1")], &second)?;
    law::Results::new(&good)?;
    let second = fields("OBX", ControlDirection::Receive, &[(1, "3"), (4, "1^1^2")])?;
    assert!(law::Results::new(&result_message(&[(4, "1^1^1")], &second)?).is_err());
    Ok(())
}

// PaLM TF-2b Rev11 C.7, Table C.7-4: complete identities must distinguish OBXs.
#[test]
fn law_tf2b_c7_repeated_observation_requires_unique_complete_og() -> TestResult {
    let second = fields(
        "OBX",
        ControlDirection::Receive,
        &[
            (1, "2"),
            (3, "test^Changed description^99LAB"),
            (4, "1^1^1"),
        ],
    )?;
    let message = result_message(&[(4, "1^1^1")], &second)?;
    let error = law::Results::new(&message).expect_err("duplicate complete identity");
    assert_eq!(
        error,
        Error::AtSegment {
            occurrence: 2,
            source: Box::new(Error::Content {
                segment: "OBX",
                field: 4
            })
        }
    );
    let second = fields("OBX", ControlDirection::Receive, &[(1, "2"), (4, "1^1^2")])?;
    law::Results::new(&result_message(&[(4, "1^1^1")], &second)?)?;
    Ok(())
}
