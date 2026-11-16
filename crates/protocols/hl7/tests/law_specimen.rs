//! Synthetic TF-2b C.11/C.12, B.1 and 2.2.4 specimen/container cases.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, Capabilities, ControlDirection, Profile},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}
fn message(id: &str, changes: &[(usize, &str)]) -> Result<Message, Box<dyn std::error::Error>> {
    let mut fields = [""; 36];
    let defaults = if id == "SPM" {
        vec![
            (1, "1"),
            (2, "sample&99LAB"),
            (4, "SER^^HL70487"),
            (11, "P^^HL70369"),
        ]
    } else {
        vec![(3, "tube")]
    };
    for (number, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(number).ok_or("fixture")? = value;
    }
    let bytes = format!(
        "MSH|^~\\&|M|L|A|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\r{id}|{}\r",
        fields.get(1..).ok_or("fixture")?.join("|")
    );
    Ok(law::parse_message(bytes.as_bytes(), options())?)
}
fn check(
    id: &str,
    changes: &[(usize, &str)],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> TestResult {
    let message = message(id, changes)?;
    let original = message.encode();
    profile.validate_segment(message.segments().nth(1).ok_or("fixture")?, direction)?;
    assert_eq!(message.raw(), original);
    Ok(())
}
fn both(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert_eq!(
            check(id, changes, Profile::receive(), direction).is_ok(),
            valid,
            "{id} {changes:?} {direction:?}"
        );
    }
    Ok(())
}
fn send_only(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    assert_eq!(
        check(id, changes, Profile::receive(), ControlDirection::Send).is_ok(),
        valid,
        "{id} {changes:?}"
    );
    check(id, changes, Profile::receive(), ControlDirection::Receive)
}

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

#[test]
fn law_tf2b_c11_sac_24_re_ce_units_identifier_system_predicate() -> TestResult {
    for value in ["", "mL^Milliliters^UCUM", "^Vendor unit"] {
        send_only("SAC", &[(24, value)], true)?;
    }
    for value in [
        "\"\"",
        "mL",
        "mL^^UCUM",
        "mL^Units",
        "mL^Units^OTHER",
        "^Units^UCUM",
        "mL^Units^UCUM~mL^Units^UCUM",
        "mL&bad^Units^UCUM",
    ] {
        send_only("SAC", &[(24, value)], false)?;
    }
    send_only(
        "SAC",
        &[(
            24,
            &format!("{}^{}^UCUM", "x".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    send_only(
        "SAC",
        &[(24, &format!("{}^Units^UCUM", "x".repeat(21)))],
        false,
    )?;
    send_only(
        "SAC",
        &[(24, &format!("mL^{}^UCUM", "x".repeat(200)))],
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c11_sac_29_re_sn_exact_ratio_and_positive_denominator() -> TestResult {
    for value in [
        "",
        "^1^:^1",
        "^1^:^5.5",
        "^1^:^.5",
        "^1^:^+5",
        "^1^:^123456789012345",
    ] {
        send_only("SAC", &[(29, value)], true)?;
    }
    for value in [
        "\"\"",
        "<^1^:^5",
        "^2^:^5",
        "^1^/^5",
        "^1^:^",
        "^1^:^0",
        "^1^:^-1",
        "^1^:^NaN",
        "^1^:^1e3",
        "^1^:^1.2.3",
        "^1^:^1234567890123456",
        "^1^:^5~^1^:^5",
        "^1^:^5&bad",
    ] {
        send_only("SAC", &[(29, value)], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_2_2_x_fields_composites_and_option_resolution() -> TestResult {
    for (id, supported) in [
        ("SPM", vec![1, 2, 3, 4, 7, 8, 9, 11, 13, 16, 17, 18, 27]),
        ("SAC", vec![3, 4, 9, 10, 11, 13, 14, 15, 21, 22, 24, 29]),
    ] {
        for number in 1..=35 {
            if supported.contains(&number) {
                continue;
            }
            check(
                id,
                &[(number, "ignored^data&too~here")],
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
    for (id, field, value) in [
        ("SPM", 2, "id&Local&oid&ISO&ignored^filler"),
        ("SPM", 3, "id&Local^filler"),
        ("SPM", 4, "SER^^HL70487^ignored"),
        ("SPM", 11, "P^^HL70369^ignored"),
        ("SAC", 3, "tube^namespace"),
        ("SAC", 4, "parent^namespace"),
        ("SAC", 10, "rack^namespace"),
        ("SAC", 13, "tray^namespace"),
    ] {
        let mut changes = vec![(field, value)];
        if field == 10 && id == "SAC" {
            changes.push((11, "1"));
        }
        if field == 13 && id == "SAC" {
            changes.push((14, "1"));
        }
        check(id, &changes, Profile::receive(), ControlDirection::Receive)?;
        assert!(check(id, &changes, Profile::receive(), ControlDirection::Send).is_err());
    }
    let disabled = Profile::for_peer(Capabilities::default());
    for (id, field, value) in [
        ("SPM", 2, "id&Local"),
        ("SPM", 3, "id&Local"),
        ("SPM", 7, "id^^99LAB"),
        ("SPM", 8, "id^^99LAB"),
        ("SPM", 9, "id^^99LAB"),
        ("SPM", 16, "id^^99LAB"),
        ("SPM", 17, "20240101120000"),
        ("SPM", 18, "20240101120000"),
        ("SPM", 27, "id^^99LAB"),
        ("SAC", 9, "id^^HL70378"),
        ("SAC", 21, "1"),
        ("SAC", 22, "1"),
        ("SAC", 24, "^Units"),
        ("SAC", 29, "^1^:^5"),
    ] {
        check(id, &[(field, value)], disabled, ControlDirection::Receive)?;
        assert!(check(id, &[(field, value)], disabled, ControlDirection::Send).is_err());
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_composite_field_ceiling_including_empty_positions() -> TestResult {
    for (id, number, value, maximum) in [
        ("SPM", 2, "id&Local", 80),
        ("SPM", 3, "id&Local", 80),
        ("SPM", 4, "SER^^HL70487", 250),
        ("SPM", 7, "id^^HL70488", 250),
        ("SPM", 8, "id^^99LAB", 250),
        ("SPM", 9, "id^^99LAB", 250),
        ("SPM", 11, "P^^HL70369", 250),
        ("SPM", 16, "id^^HL70489", 250),
        ("SPM", 17, "20240101120000", 26),
        ("SPM", 18, "20240101120000", 26),
        ("SPM", 27, "id^^99LAB", 250),
        ("SAC", 3, "tube", 80),
        ("SAC", 4, "parent", 80),
        ("SAC", 9, "id^^HL70378", 250),
        ("SAC", 24, "mL^Units^UCUM", 250),
    ] {
        let exact = format!("{value}{}", "^".repeat(maximum - value.len()));
        check(
            id,
            &[(number, &exact)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
        assert!(
            check(
                id,
                &[(number, &format!("{exact}^"))],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_work_builder_validates_container_details_and_option_fields() -> TestResult {
    use analyte_hl7::Response;
    use law::{Address, Awos, ContainerDetails, SpecimenDetails, WorkAction};
    let awos = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^^HL70487",
        b"P",
        b"",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?;
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = Response {
        control_id: law::control_id(b"out-1")?,
        timestamp: b"20260101120000+0000",
    };
    let capabilities = Capabilities {
        content: law::PeerContent {
            specimen: true,
            container: true,
            ..Capabilities::default().content
        },
        ..Capabilities::default()
    };
    // C.11: fail before a caller can reserve work using this render result.
    assert!(matches!(
        law::work_order(&[awos], None, address, response, capabilities, options()),
        Err(law::Error::Content {
            segment: "SAC",
            field: 4
        })
    ));
    let no_parent = ContainerDetails {
        carrier: Some(b"rack"),
        carrier_position: Some(b"1"),
        ..ContainerDetails::default()
    };
    assert!(matches!(
        law::work_order(
            &[awos.with_container_details(&no_parent)],
            None,
            address,
            response,
            capabilities,
            options()
        ),
        Err(law::Error::Content {
            segment: "SAC",
            field: 4
        })
    ));
    let specimen = SpecimenDetails {
        parents: Some(b"parent&Local"),
        collection_method: Some(b"id^^HL70488"),
        source_site: Some(b"id^^99LAB"),
        source_modifier: Some(b"id^^99LAB"),
        risk: Some(b"id^^HL70489"),
        collection_time: Some(b"20240101120000"),
        receipt_time: Some(b"20240101130000"),
        container_type: Some(b"id^^99LAB"),
    };
    let container = ContainerDetails {
        parent: Some(b"parent"),
        carrier_type: Some(b"id^^HL70378"),
        carrier: Some(b"rack"),
        carrier_position: Some(b"1^2"),
        tray: Some(b"tray"),
        tray_position: Some(b"1^2"),
        location: Some(b"location^^99LAB"),
        container_volume: Some(b"2.5"),
        specimen_volume: Some(b"1.2"),
        volume_units: Some(b"mL^Milliliters^UCUM"),
        dilution: Some(b"^1^:^5.5"),
    };
    let work = awos
        .with_specimen_details(&specimen)
        .with_container_details(&container);
    law::work_order(&[work], None, address, response, capabilities, options())?;
    validate_specimen_detail_cases(awos, address, response, capabilities, specimen)?;
    validate_container_detail_cases(awos, address, response, capabilities, container)?;
    Ok(())
}

#[test]
fn law_tf2b_2_2_4_2_c11_c12_negative_query_echoes_each_scope_without_invented_ids() -> TestResult {
    use analyte_hl7::{Field, Response};
    use law::{Address, Query};
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = Response {
        control_id: law::control_id(b"out-1")?,
        timestamp: b"20260101120000+0000",
    };
    for body in [
        "QPD|WOS^Synthetic^IHELAW|tag|000tube",
        "QPD|WOS_ALL^Synthetic^IHELAW|tag",
        "QPD|WOS_BY_RACK^Synthetic^IHELAW|tag||rack|1",
        "QPD|WOS_BY_TRAY^Synthetic^IHELAW|tag||||tray|1^2",
        "QPD|WOS_BY_ISOLATE^Synthetic^IHELAW|tag|child||||||parent",
        "QPD|WOS_INFORMATIONAL_RACK^Synthetic^IHELAW|tag|000tube|rack|2",
    ] {
        let wire = format!(
            "MSH|^~\\&|A|L|M|L|20260101120000+0000||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\r{body}\rRCP|I||R^Real Time^HL70394\r"
        );
        let source = law::parse_message(wire.as_bytes(), options())?;
        let query = Query::new(&source)?;
        let output = law::negative_work(query, address, response, options())?;
        Profile::for_peer(Capabilities::default())
            .validate_message(&output, ControlDirection::Send)?;
        let spm = output
            .segments()
            .find(|segment| segment.id() == b"SPM")
            .ok_or("fixture")?;
        assert_eq!(spm.field(4).map(Field::raw), Some(b"\"\"".as_slice()));
        let malformed = std::str::from_utf8(output.raw())?.replace("ORC|DC", "ORC|NW");
        let parsed = law::parse_message(malformed.as_bytes(), options())?;
        assert!(
            Profile::receive()
                .validate_message(&parsed, ControlDirection::Send)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_results_receive_preserves_x_and_rejects_bad_supported_identity() -> TestResult {
    let wire = "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|r1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample&Local||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||test^Synthetic^99LAB\rORC|SC||||IP\r";
    let good = law::parse_message(wire.as_bytes(), options())?;
    law::Results::new(&good)?;
    for (needle, value) in [
        ("sample&Local", "unqualified"),
        ("SER^^HL70487", "SER^^OTHER"),
        ("SAC|||tube", "SAC|||"),
        ("SPM|1|", "SPM|2|"),
    ] {
        let invalid = law::parse_message(wire.replace(needle, value).as_bytes(), options())?;
        let bytes = invalid.encode();
        assert!(law::Results::new(&invalid).is_err());
        assert_eq!(invalid.raw(), bytes);
    }
    let appended = wire.replace(
        "SAC|||tube",
        "SAC|||tube||||||ignored||||||||||||not-numeric",
    );
    let input = law::parse_message(appended.as_bytes(), options())?;
    law::Results::new(&input)?;
    assert_eq!(input.raw(), appended.as_bytes());
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_remaining_location_lengths_and_pool_receive_predicates() -> TestResult {
    for (field, value, size, position) in [
        (10, "rack", 80, 11),
        (13, "tray", 80, 14),
        (15, "location^^99LAB", 250, 11),
    ] {
        let exact = format!("{value}{}", "^".repeat(size - value.len()));
        let mut changes = if field == 15 {
            vec![(10, "rack"), (11, "1"), (field, exact.as_str())]
        } else {
            vec![(field, exact.as_str()), (position, "1")]
        };
        changes.extend([(3, ""), (4, "parent")]);
        check("SAC", &changes, Profile::receive(), ControlDirection::Send)?;
        let oversized = format!("{exact}^");
        let mut invalid = changes.clone();
        invalid.push((field, &oversized));
        assert!(check("SAC", &invalid, Profile::receive(), ControlDirection::Send).is_err());
    }
    let pool = Profile::receive().with_pooling_outside(true);
    for count in ["", "\"\"", "0", "1.5", "1000000", "2~2"] {
        assert!(
            check(
                "SPM",
                &[(11, "L^^HL70369"), (13, count)],
                pool,
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    check(
        "SPM",
        &[(11, "L^^HL70369"), (13, "999999")],
        pool,
        ControlDirection::Receive,
    )?;
    let no_patients = "MSH|^~\\&|M|L|A|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\rPID|||id||Family^^^^^^L\rSPM|1|sample&Local|parent&Local|SER^^HL70487|||||||L^^HL70369||1\r";
    let message = law::parse_message(no_patients.as_bytes(), options())?;
    assert!(
        pool.validate_message(&message, ControlDirection::Send)
            .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c12_role_predicates_require_the_standard_coding_system() -> TestResult {
    for code in ["P", "Q", "U", "L"] {
        both("SPM", &[(11, &format!("{code}^^99LAB"))], true)?;
    }
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert!(
            check(
                "SPM",
                &[(11, "U^^IHELAW"), (4, "\"\"")],
                Profile::receive(),
                direction
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c11_c12_explicit_profile_cannot_enlarge_peer_send_capabilities() -> TestResult {
    use analyte_hl7::Response;
    use law::{Address, Awos, ContainerDetails, WorkAction};
    let details = ContainerDetails {
        container_volume: Some(b"1.5"),
        ..ContainerDetails::default()
    };
    let awos = Awos::new(
        b"a1",
        b"sample&Local",
        b"SER^^HL70487",
        b"P",
        b"tube",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?
    .with_container_details(&details);
    let address = Address {
        application: b"M",
        facility: b"L",
        peer_application: b"A",
        peer_facility: b"L",
    };
    let response = Response {
        control_id: law::control_id(b"m1")?,
        timestamp: b"20260101120000+0000",
    };
    assert!(
        law::work_order_with_profile(
            &[awos],
            None,
            address,
            response,
            Capabilities::default(),
            options(),
            Profile::receive()
        )
        .is_err()
    );
    let supported = Capabilities {
        content: law::PeerContent {
            container: true,
            ..Capabilities::default().content
        },
        ..Capabilities::default()
    };
    law::work_order_with_profile(
        &[awos],
        None,
        address,
        response,
        supported,
        options(),
        Profile::receive(),
    )?;
    Ok(())
}
#[test]
fn law_tf2b_c11_am_carrier_tray_only_without_sac3_with_sac4() -> TestResult {
    // Original failing synthetic SAC bytes from the Gazelle export, C.11 M/X.
    let failing = law::parse_message(b"MSH|^~\\&|ANALYTE|ANALYTE-TEST|SYN-ANALYZER|ANALYTE-TEST|20260101120000+0000||OML^O33^OML_O33|SYN-OML1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE~LAW_SPECIMEN^IHE\rSPM|1|SYN-0001&ANALYTE-TEST||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||SYN-0001|||||||SYN-RACK1|1||||||||||||||||||\rORC|NW||||||||20260101120000||||||||||||\rOBR||SYN-AWOS1||SYN-TEST^Synthetic test^99SYN|||||||||||||||||||||||||||||||||||||||||||||\r", options())?;
    assert_eq!(
        Profile::for_peer(Capabilities {
            content: law::PeerContent {
                specimen: true,
                ..Capabilities::default().content
            },
            ..Capabilities::default()
        })
        .validate_message(&failing, ControlDirection::Send),
        Err(law::Error::AtSegment {
            occurrence: 1,
            source: Box::new(law::Error::Content {
                segment: "SAC",
                field: 10
            })
        })
    );
    for (identity, position) in [(10, 11), (13, 14)] {
        send_only("SAC", &[(identity, "rack"), (position, "1")], false)?;
        both(
            "SAC",
            &[(3, ""), (4, "parent"), (identity, "rack"), (position, "1")],
            true,
        )?;
        assert!(
            check(
                "SAC",
                &[(3, ""), (identity, "rack"), (position, "1")],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    Ok(())
}

fn validate_carrier_axis_limits(identity: usize, position: usize) -> TestResult {
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, "1234567890123456"),
        ],
        true,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, "12345678901234567"),
        ],
        false,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, "1^12345678901234567"),
        ],
        false,
    )?;
    assert!(
        check(
            "SAC",
            &[(3, ""), (4, "parent"), (identity, "rack")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SAC",
        &[(3, ""), (4, "parent"), (identity, "rack")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "SAC",
            &[(position, "1")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "SAC",
        &[(position, "1")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    let exact = format!(
        "{}^{}^{}^{}^{}",
        "1".repeat(16),
        "2".repeat(16),
        "3".repeat(16),
        "4".repeat(16),
        "5".repeat(12)
    );
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, &exact),
        ],
        true,
    )?;
    both(
        "SAC",
        &[
            (3, ""),
            (4, "parent"),
            (identity, "rack"),
            (position, &format!("{exact}1")),
        ],
        false,
    )?;
    Ok(())
}

fn validate_specimen_detail_cases(
    awos: law::Awos<'_>,
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: Capabilities,
    specimen: law::SpecimenDetails<'_>,
) -> TestResult {
    use law::SpecimenDetails;
    for invalid in [
        SpecimenDetails {
            parents: Some(b"unqualified"),
            ..specimen
        },
        SpecimenDetails {
            source_modifier: Some(b"id^^99LAB~other^^99LAB"),
            ..specimen
        },
        SpecimenDetails {
            collection_time: Some(b"20230229120000"),
            ..specimen
        },
        SpecimenDetails {
            risk: Some(b"id^^OTHER"),
            ..specimen
        },
    ] {
        assert!(
            law::work_order(
                &[awos.with_specimen_details(&invalid)],
                None,
                address,
                response,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    Ok(())
}

fn validate_container_detail_cases(
    awos: law::Awos<'_>,
    address: law::Address<'_>,
    response: analyte_hl7::Response<'_>,
    capabilities: Capabilities,
    container: law::ContainerDetails<'_>,
) -> TestResult {
    use law::ContainerDetails;
    for invalid in [
        ContainerDetails {
            carrier_position: None,
            ..container
        },
        ContainerDetails {
            tray_position: Some(b"1.5"),
            ..container
        },
        ContainerDetails {
            container_volume: Some(b"1e3"),
            ..container
        },
        ContainerDetails {
            volume_units: Some(b"mL^Units^OTHER"),
            ..container
        },
        ContainerDetails {
            dilution: Some(b"^1^:^0"),
            ..container
        },
    ] {
        assert!(
            law::work_order(
                &[awos.with_container_details(&invalid)],
                None,
                address,
                response,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    // Carrier/parent identity is base information, not gated by LAW_CONTAINER.
    let base_details = ContainerDetails {
        parent: Some(b"parent"),
        carrier: Some(b"rack"),
        carrier_position: Some(b"1"),
        ..ContainerDetails::default()
    };
    law::work_order(
        &[awos.with_container_details(&base_details)],
        None,
        address,
        response,
        Capabilities::default(),
        options(),
    )?;
    assert!(
        law::work_order(
            &[awos.with_container_details(&container)],
            None,
            address,
            response,
            Capabilities::default(),
            options()
        )
        .is_err()
    );
    Ok(())
}
