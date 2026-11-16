//! Tests law tf2b c12 spm 1 m si length type and message sequence; law tf2b c12 spm 2 3 eip ei identity namespace predicate lengths and repetitions.
use super::*;

#[test]
fn law_tf2b_c12_spm_1_m_si_length_type_and_message_sequence() -> TestResult {
    for value in ["1", "0001", "9999"] {
        both("SPM", &[(1, value)], true)?;
    }
    for value in [
        "", "\"\"", "0", "0000", "10000", "-1", "1.0", "1~1", "1^x", "1&x",
    ] {
        both("SPM", &[(1, value)], false)?;
    }
    let base = message("SPM", &[])?;
    let text = std::str::from_utf8(base.raw())?;
    let first = base.segments().nth(1).ok_or("fixture")?.raw();
    let first = std::str::from_utf8(first)?;
    for (second, valid) in [("2", true), ("0002", true), ("1", false), ("3", false)] {
        let wire = format!(
            "{text}{}\r",
            first.replacen("SPM|1|", &format!("SPM|{second}|"), 1)
        );
        let parsed = law::parse_message(wire.as_bytes(), options())?;
        for direction in [ControlDirection::Send, ControlDirection::Receive] {
            assert_eq!(
                Profile::receive()
                    .validate_message(&parsed, direction)
                    .is_ok(),
                valid
            );
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_c12_spm_2_3_eip_ei_identity_namespace_predicate_lengths_and_repetitions() -> TestResult
{
    for field in [2, 3] {
        for value in ["", "id&Local", "id&&1.2.3&ISO", "id&Local&1.2.3&ISO"] {
            both("SPM", &[(field, value)], true)?;
        }
        both(
            "SPM",
            &[(
                field,
                &format!("{}&{}", "\u{e9}".repeat(50), "n".repeat(20)),
            )],
            true,
        )?;
        for value in [
            "\"\"",
            "id",
            "&Local",
            "id&&1.2.3",
            "id&&&ISO",
            "id&&1.2.3&BAD",
            "id&\"\"",
            "id&local&\"\"",
            "id&local&&\"\"",
        ] {
            both("SPM", &[(field, value)], false)?;
        }
        both(
            "SPM",
            &[(field, &format!("{}&Local", "x".repeat(51)))],
            false,
        )?;
        both("SPM", &[(field, &format!("id&{}", "x".repeat(21)))], false)?;
        // The enclosing 80-character field remains a hard limit.
        both(
            "SPM",
            &[(field, &format!("id&&{}&URI", "u".repeat(72)))],
            true,
        )?;
        both(
            "SPM",
            &[(field, &format!("id&&{}&URI", "u".repeat(73)))],
            false,
        )?;
        both("SPM", &[(field, "id&Local~other&Local")], field == 3)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c12_spm_4_11_m_cwe_role_type_null_predicates_and_namespaces() -> TestResult {
    both("SPM", &[], true)?;
    both(
        "SPM",
        &[(4, "local^Synthetic^99LAB"), (11, "local^^99LAB")],
        true,
    )?;
    both("SPM", &[(11, "Q^^HL70369"), (4, "\"\"")], true)?;
    both("SPM", &[(11, "Q^^HL70369")], false)?;
    assert!(
        check(
            "SPM",
            &[(4, "\"\"")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SPM",
        &[(4, "\"\"")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    for field in [4, 11] {
        for value in [
            "",
            "bad",
            "^Text^99LAB",
            "id^^OTHER",
            "id^^99AB",
            "id^^99A!B",
            "id^^99LAB~id^^99LAB",
            "id&bad^^99LAB",
            "\"\"~\"\"",
        ] {
            both("SPM", &[(field, value)], false)?;
        }
        both(
            "SPM",
            &[(
                field,
                &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
            )],
            true,
        )?;
        both(
            "SPM",
            &[(field, &format!("{}^^99LAB", "x".repeat(21)))],
            false,
        )?;
        both(
            "SPM",
            &[(field, &format!("id^{}^99LAB", "x".repeat(200)))],
            false,
        )?;
    }
    both("SPM", &[(11, "U^^HL70369"), (4, "\"\"")], false)?;
    assert!(
        check(
            "SPM",
            &[(11, "U^^IHELAW"), (4, "\"\"")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c12_spm_7_8_9_16_27_option_cwe_and_analyzer_x() -> TestResult {
    for (field, standard) in [
        (7, "HL70488"),
        (8, "99LAB"),
        (9, "99LAB"),
        (16, "HL70489"),
        (27, "99LAB"),
    ] {
        send_only("SPM", &[(field, "")], true)?;
        send_only("SPM", &[(field, &format!("id^Synthetic^{standard}"))], true)?;
        send_only(
            "SPM",
            &[(
                field,
                &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
            )],
            true,
        )?;
        for value in [
            "\"\"",
            "id",
            "^Text^99LAB",
            "id^^OTHER",
            "id^^99ABCD",
            "id^^99A!B",
            "id^^99LAB~other^^99LAB",
            "id&bad^^99LAB",
        ] {
            send_only("SPM", &[(field, value)], false)?;
        }
        send_only(
            "SPM",
            &[(field, &format!("{}^^99LAB", "x".repeat(21)))],
            false,
        )?;
        send_only(
            "SPM",
            &[(field, &format!("id^{}^99LAB", "x".repeat(200)))],
            false,
        )?;
        assert!(
            check(
                "SPM",
                &[(field, "id^^99LAB")],
                Profile::for_peer(Capabilities::default()),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c12_spm_17_18_dr_ts_required_start_seconds_calendar_and_x() -> TestResult {
    for field in [17, 18] {
        for value in ["", "20240229235959"] {
            send_only("SPM", &[(field, value)], true)?;
        }
        for value in [
            "\"\"",
            "20230229120000",
            "20240101240000",
            "20240101126000",
            "20240101120060",
            "20240101",
            "20240101120000+0000",
            "20240101120000~20240101120000",
            "20240101120000^end",
            "20240101120000&precision",
        ] {
            send_only("SPM", &[(field, value)], false)?;
        }
        assert!(
            check(
                "SPM",
                &[(field, "20240101120000")],
                Profile::for_peer(Capabilities::default()),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c12_spm_3_13_pooling_outside_r_x_count_and_role() -> TestResult {
    let selected = Profile::receive().with_pooling_outside(true);
    let good = [
        (3, "first&Local~second&Local"),
        (11, "L^^HL70369"),
        (13, "2"),
    ];
    check("SPM", &good, selected, ControlDirection::Send)?;
    for changes in [
        vec![(11, "L^^HL70369"), (13, "2")],
        vec![(3, "parent&Local"), (11, "P^^HL70369"), (13, "1")],
        vec![(3, "parent&Local"), (11, "L^^HL70369")],
    ] {
        assert!(check("SPM", &changes, selected, ControlDirection::Send).is_err());
    }
    for value in [
        "", "\"\"", "0", "-1", "1.5", "1000000", "1~1", "1^bad", "1&bad",
    ] {
        let mut changes = good.to_vec();
        changes.push((13, value));
        assert!(check("SPM", &changes, selected, ControlDirection::Send).is_err());
    }
    check(
        "SPM",
        &[(11, "L^^HL70369"), (13, "2")],
        selected,
        ControlDirection::Receive,
    )?;
    // LAB-29 does not echo SPM-3 when this option is selected (2.2.4.17).
    check(
        "SPM",
        &[(3, "ignored"), (11, "L^^HL70369"), (13, "2")],
        selected,
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "SPM",
            &[(13, "2")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    assert!(
        check(
            "SPM",
            &[(11, "L^^HL70369")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_3_4_conditional_ei_component_lengths_nulls_and_cardinality() -> TestResult {
    both("SAC", &[], true)?;
    both("SAC", &[(4, "parent")], true)?;
    for field in [3, 4, 10, 13] {
        let mut changes = vec![(field, "id")];
        if matches!(field, 10 | 13) {
            changes.extend([(3, ""), (4, "parent")]);
        }
        if field == 10 {
            changes.push((11, "1"));
        }
        if field == 13 {
            changes.push((14, "1"));
        }
        for direction in [ControlDirection::Send, ControlDirection::Receive] {
            check("SAC", &changes, Profile::receive(), direction)?;
        }
        for value in ["\"\"", "id~other", "id&bad"] {
            let mut invalid = changes.clone();
            invalid.push((field, value));
            if field == 3 && value == "\"\"" {
                assert!(
                    check("SAC", &invalid, Profile::receive(), ControlDirection::Send).is_err()
                );
                check(
                    "SAC",
                    &invalid,
                    Profile::receive(),
                    ControlDirection::Receive,
                )?;
            } else {
                both("SAC", &invalid, false)?;
            }
        }
        let mut exact = changes.clone();
        let long = "\u{e9}".repeat(50);
        exact.push((field, &long));
        both("SAC", &exact, true)?;
        let too_long = "x".repeat(51);
        exact.push((field, &too_long));
        both("SAC", &exact, false)?;
    }
    both("SAC", &[(3, ""), (4, "")], false)?;
    assert!(
        check(
            "SAC",
            &[(3, ""), (4, "parent")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SAC",
        &[(3, ""), (4, "parent")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    both(
        "SAC",
        &[(3, ""), (4, "parent"), (10, "rack"), (11, "1")],
        true,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_10_11_13_14_carrier_tray_predicates_na_axes_and_limits() -> TestResult {
    for (identity, position) in [(10, 11), (13, 14)] {
        for value in ["1", "0001", "+1", "0", "1^2.5^3", "1^^3"] {
            both(
                "SAC",
                &[
                    (3, ""),
                    (4, "parent"),
                    (identity, "rack"),
                    (position, value),
                ],
                true,
            )?;
        }
        for value in ["\"\"", "1.5", "x", "1~2", "1&2", "1^x", "1^\"\""] {
            both(
                "SAC",
                &[
                    (3, ""),
                    (4, "parent"),
                    (identity, "rack"),
                    (position, value),
                ],
                false,
            )?;
        }
        validate_carrier_axis_limits(identity, position)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_15_conditional_re_location_cwe_repetitions() -> TestResult {
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (10, "rack"),
            (11, "1"),
            (15, "location^^99LAB~other^Text^99LAB"),
        ],
        true,
    )?;
    for value in [
        "\"\"",
        "id",
        "^Text^99LAB",
        "id^^OTHER",
        "id^^99A!B",
        "id^^99LAB~",
        "id&bad^^99LAB",
    ] {
        both(
            "SAC",
            &[(3, ""), (4, "parent"), (10, "rack"), (11, "1"), (15, value)],
            false,
        )?;
    }
    assert!(
        check(
            "SAC",
            &[(15, "location^^99LAB")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SAC",
        &[(15, "location^^99LAB")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (10, "rack"),
            (11, "1"),
            (
                15,
                &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
            ),
        ],
        true,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (10, "rack"),
            (11, "1"),
            (15, &format!("{}^^99LAB", "x".repeat(21))),
        ],
        false,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (10, "rack"),
            (11, "1"),
            (15, &format!("id^{}^99LAB", "x".repeat(200))),
        ],
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_9_re_container_option_ce_namespace() -> TestResult {
    for value in ["", "local^^HL70378", "local^Synthetic^HL70378"] {
        send_only("SAC", &[(9, value)], true)?;
    }
    for value in [
        "\"\"",
        "local",
        "^Text^HL70378",
        "local^^99LAB",
        "local^^OTHER",
        "local^^HL70378~local^^HL70378",
        "local&bad^^HL70378",
    ] {
        send_only("SAC", &[(9, value)], false)?;
    }
    send_only(
        "SAC",
        &[(
            9,
            &format!("{}^{}^HL70378", "x".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    send_only(
        "SAC",
        &[(9, &format!("{}^^HL70378", "x".repeat(21)))],
        false,
    )?;
    send_only(
        "SAC",
        &[(9, &format!("id^{}^HL70378", "x".repeat(200)))],
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_21_22_re_nm_spelling_length_and_singletons() -> TestResult {
    for field in [21, 22] {
        for value in [
            "",
            "0",
            "1.25",
            "+1",
            "-1",
            ".5",
            "1.",
            "12345678901234567890",
        ] {
            send_only("SAC", &[(field, value)], true)?;
        }
        for value in [
            "\"\"",
            "+",
            ".",
            "1..2",
            "1e3",
            "NaN",
            "<1",
            "1,2",
            "123456789012345678901",
            "1~2",
            "1^2",
            "1&2",
        ] {
            send_only("SAC", &[(field, value)], false)?;
        }
    }
    Ok(())
}
