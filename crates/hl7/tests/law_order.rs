//! Synthetic order/timing/test-detail cases from TF-2b C.6/C.8/C.13/C.14.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection, Error, OrderControl, OrderStatus, Profile},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const TEST: &str = "test^Synthetic^99LAB";
fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}
fn message(
    id: &str,
    direction: ControlDirection,
    changes: &[(usize, &str)],
) -> Result<Message, Box<dyn std::error::Error>> {
    let mut fields = [""; 52];
    let defaults = match id {
        "ORC" => match direction {
            ControlDirection::Send => vec![(1, "NW"), (9, "20260101120000")],
            ControlDirection::Receive => vec![(1, "SC"), (5, "IP")],
        },
        "OBR" => vec![(2, "awos-1"), (4, TEST)],
        "TCD" => vec![(1, TEST)],
        "TQ1" => vec![(9, "R^^HL70485")],
        _ => return Err("fixture segment".into()),
    };
    for (number, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(number).ok_or("fixture field")? = value;
    }
    let wire = format!(
        "MSH|^~\\&|M|L|A|L|20260101120000+0000||OML^O33^OML_O33|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-28^IHE\r{id}|{}\r",
        fields.get(1..).ok_or("fixture")?.join("|")
    );
    Ok(law::parse_message(wire.as_bytes(), options())?)
}
fn check(
    id: &str,
    changes: &[(usize, &str)],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> TestResult {
    let message = message(id, direction, changes)?;
    let original = message.serialize();
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
fn manager(id: &str, changes: &[(usize, &str)], valid: bool) -> TestResult {
    assert_eq!(
        check(id, changes, Profile::receive(), ControlDirection::Send).is_ok(),
        valid,
        "{id} {changes:?}"
    );
    check(id, changes, Profile::receive(), ControlDirection::Receive)
}

#[test]
fn law_tf2b_c8_order_control_and_order_status_are_closed_enums() -> TestResult {
    for value in [
        OrderControl::New,
        OrderControl::Accepted,
        OrderControl::UnableToAccept,
        OrderControl::Cancel,
        OrderControl::Cancelled,
        OrderControl::UnableToCancel,
        OrderControl::Discontinue,
        OrderControl::StatusChange,
    ] {
        assert_eq!(OrderControl::parse(value.as_bytes())?, value);
    }
    for value in [
        OrderStatus::Scheduled,
        OrderStatus::InProgress,
        OrderStatus::Complete,
        OrderStatus::Cancelled,
    ] {
        assert_eq!(OrderStatus::parse(value.as_bytes())?, value);
    }
    for value in [b"".as_slice(), b"\"\"", b"XX", b"NW~NW"] {
        assert!(OrderControl::parse(value).is_err());
    }
    for value in [b"".as_slice(), b"\"\"", b"XX", b"IP~IP"] {
        assert!(OrderStatus::parse(value).is_err());
    }
    Ok(())
}

#[test]
fn law_tf2b_c8_orc_1_m_control_direction_and_transaction_predicates() -> TestResult {
    for value in ["NW", "CA"] {
        check(
            "ORC",
            &[(1, value)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
    }
    check("ORC", &[], Profile::receive(), ControlDirection::Receive)?;
    for value in [
        "", "\"\"", "XX", "NWW", "NW~NW", "NW^bad", "NW&bad", "OK", "UA", "CR", "UC", "SC", "DC",
    ] {
        assert!(
            check(
                "ORC",
                &[(1, value)],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    for value in ["NW", "CA", "DC", "OK", "UA", "CR", "UC", "XX"] {
        assert!(
            check(
                "ORC",
                &[(1, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    for value in ["OK", "UA", "CR", "UC"] {
        check(
            "ORC",
            &[(1, value), (2, "awos-1")],
            Profile::receive_work_response(),
            ControlDirection::Receive,
        )?;
    }
    assert!(
        check(
            "ORC",
            &[(1, "SC"), (2, "awos-1")],
            Profile::receive_work_response(),
            ControlDirection::Receive
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c8_orc_2_conditional_ei_5_m_status_and_manager_x() -> TestResult {
    for field in [2, 5] {
        for value in ["", "\"\"", "bad~bad", "bad&bad"] {
            assert!(
                check(
                    "ORC",
                    &[(1, "OK"), (2, "awos-1"), (field, value)],
                    Profile::receive_work_response(),
                    ControlDirection::Receive
                )
                .is_err()
            );
        }
    }
    for value in ["SC", "IP", "CM", "CA"] {
        check(
            "ORC",
            &[(5, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    assert!(
        check(
            "ORC",
            &[(5, "XX")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    let fifty = "\u{e9}".repeat(50);
    check(
        "ORC",
        &[(1, "OK"), (2, &fifty)],
        Profile::receive_work_response(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "ORC",
            &[(1, "OK"), (2, &format!("{fifty}x"))],
            Profile::receive_work_response(),
            ControlDirection::Receive
        )
        .is_err()
    );
    for field in [2, 5] {
        assert!(
            check(
                "ORC",
                &[(field, "ignored")],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    // ORC-2 is X in OUL; its unsupported content does not overwrite AWOS identity.
    check(
        "ORC",
        &[(2, "ignored~content")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c8_orc_4_re_eip_and_8_reflex_parents_repeated() -> TestResult {
    for (field, direction) in [
        (4, ControlDirection::Send),
        (4, ControlDirection::Receive),
        (8, ControlDirection::Receive),
    ] {
        for value in ["", "group", "000Ab"] {
            check("ORC", &[(field, value)], Profile::receive(), direction)?;
        }
        check(
            "ORC",
            &[(field, &"\u{e9}".repeat(50))],
            Profile::receive(),
            direction,
        )?;
        for value in ["\"\"", "&namespace", "^filler"] {
            assert!(check("ORC", &[(field, value)], Profile::receive(), direction).is_err());
        }
        assert!(
            check(
                "ORC",
                &[(field, &"x".repeat(51))],
                Profile::receive(),
                direction
            )
            .is_err()
        );
    }
    both("ORC", &[(4, "one~two")], false)?;
    check(
        "ORC",
        &[(8, "one~two")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "ORC",
            &[(8, "one~")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    assert!(
        check(
            "ORC",
            &[(8, "parent")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "ORC",
        &[(8, "ignored~")],
        Profile::receive().with_analyzer_reflex(false),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "ORC",
            &[(4, "group&namespace")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "ORC",
        &[(4, "group&namespace^filler")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c8_orc_9_m_event_timestamp_and_27_re_available_timestamp() -> TestResult {
    for value in ["20240229120000", "20260101235959"] {
        manager("ORC", &[(9, value)], true)?;
        check(
            "ORC",
            &[(27, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    for value in [
        "",
        "\"\"",
        "20230229120000",
        "20260101240000",
        "20260101126000",
        "20260101120060",
        "20260101",
        "20260101120000+0000",
        "20260101120000~20260101120000",
        "20260101120000&bad",
    ] {
        manager("ORC", &[(9, value)], false)?;
    }
    for value in ["", "20240229", "20240229120000"] {
        check(
            "ORC",
            &[(27, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    for value in [
        "\"\"",
        "20230229",
        "2024010112",
        "20240101120000+0000",
        "20240101~20240101",
        "20240101&bad",
    ] {
        assert!(
            check(
                "ORC",
                &[(27, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    assert!(
        check(
            "ORC",
            &[(27, "20240101")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c8_orc_21_re_xon_required_name_hash_50_preserved() -> TestResult {
    for value in ["", "Facility", &"\u{e9}".repeat(300)] {
        both("ORC", &[(21, value)], true)?;
    }
    for value in ["\"\"", "^type", "name&nested", "one~two"] {
        both("ORC", &[(21, value)], false)?;
    }
    manager("ORC", &[(21, "Name^ignored")], false)?;
    Ok(())
}

#[test]
fn law_tf2b_c6_obr_2_m_3_re_an_ei_id_limits_and_null_awos() -> TestResult {
    both("OBR", &[], true)?;
    for value in ["", "awos-1"] {
        check(
            "OBR",
            &[(3, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    for field in [2, 3] {
        let long = "\u{e9}".repeat(50);
        check(
            "OBR",
            &[(field, &long)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
        assert!(
            check(
                "OBR",
                &[(field, &format!("{long}x"))],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
        for value in ["one~two", "one&bad"] {
            assert!(
                check(
                    "OBR",
                    &[(field, value)],
                    Profile::receive(),
                    ControlDirection::Receive
                )
                .is_err()
            );
        }
    }
    assert!(
        check(
            "OBR",
            &[(2, "")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    check(
        "OBR",
        &[(2, "\"\"")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBR",
            &[(2, "\"\"")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    assert!(
        check(
            "OBR",
            &[(3, "filler")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    assert!(
        check(
            "OBR",
            &[(3, "\"\"")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c6_c13_obr_4_tcd_1_required_test_components_and_coding_systems() -> TestResult {
    for (id, field) in [("OBR", 4), ("TCD", 1)] {
        for system in ["LN", "JC10", "99LAB"] {
            both(id, &[(field, &format!("id^Synthetic^{system}"))], true)?;
        }
        both(
            id,
            &[(
                field,
                &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
            )],
            true,
        )?;
        for value in [
            "",
            "\"\"",
            "id",
            "^Text^99LAB",
            "id^^99LAB",
            "id^Text^OTHER",
            "id^Text^99AB",
            "id^Text^99A!B",
            "id^Text^99LAB~id^Text^99LAB",
            "id&bad^Text^99LAB",
        ] {
            both(id, &[(field, value)], false)?;
        }
        both(
            id,
            &[(field, &format!("{}^Text^99LAB", "x".repeat(21)))],
            false,
        )?;
        both(
            id,
            &[(field, &format!("id^{}^99LAB", "x".repeat(200)))],
            false,
        )?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c6_obr_11_reflex_g_and_49_result_handling_re() -> TestResult {
    check(
        "OBR",
        &[(11, "G"), (49, "RE")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    for value in ["\"\"", "X", "GG", "G~G", "G^x", "G&x"] {
        assert!(
            check(
                "OBR",
                &[(11, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    assert!(
        check(
            "OBR",
            &[(11, "G")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "OBR",
        &[(11, "ignored~values")],
        Profile::receive().with_analyzer_reflex(false),
        ControlDirection::Receive,
    )?;
    // Follow C.6.1's AM RE column despite the contradictory following prose.
    for value in ["", "RE"] {
        both("OBR", &[(49, value)], true)?;
    }
    for value in ["\"\"", "XX", "REE", "RE~RE", "RE^x", "RE&x"] {
        both("OBR", &[(49, value)], false)?;
    }
    check(
        "OBR",
        &[(49, "ignored~")],
        Profile::receive().with_external_results(false),
        ControlDirection::Receive,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c6_obr_16_re_xcn_nested_fn_hd_and_hash_names() -> TestResult {
    for value in [
        "",
        "provider",
        "provider^Family^Given^Middle^Jr^^^^Local",
        "provider^^^^^^^^&1.2.3&ISO",
    ] {
        manager("OBR", &[(16, value)], true)?;
    }
    for value in [
        "\"\"",
        "^Family",
        "provider&bad",
        "one~two",
        "provider^^^^^^^^&oid",
        "provider^^^^^^^^&oid&BAD",
        "provider^Family&bad",
        "provider^^Given&bad",
    ] {
        manager("OBR", &[(16, value)], false)?;
    }
    manager("OBR", &[(16, &"p".repeat(15))], true)?;
    manager("OBR", &[(16, &"p".repeat(16))], false)?;
    for position in [2, 3, 4, 5] {
        let long = "\u{e9}".repeat(300);
        let mut parts = [""; 9];
        *parts.first_mut().ok_or("fixture")? = "provider";
        *parts.get_mut(position - 1).ok_or("fixture")? = &long;
        manager("OBR", &[(16, &parts.join("^"))], true)?;
    }
    manager(
        "OBR",
        &[(16, &format!("provider^^^^^^^^{}", "n".repeat(21)))],
        false,
    )?;
    manager(
        "OBR",
        &[(16, &format!("provider^^^^^^^^&{}&URI", "u".repeat(200)))],
        false,
    )?;
    let bindings = [law::AuthorityBinding::new(b"Local", b"1.2.3", b"ISO")?];
    let profile = Profile::receive().with_authorities(&bindings);
    check(
        "OBR",
        &[(16, "provider^^^^^^^^Local&1.2.3&ISO")],
        profile,
        ControlDirection::Send,
    )?;
    assert!(
        check(
            "OBR",
            &[(16, "provider^^^^^^^^Local&1.2.4&ISO")],
            profile,
            ControlDirection::Send
        )
        .is_err()
    );
    let unknown = message(
        "OBR",
        ControlDirection::Send,
        &[(16, "provider^^^^^^^^Local&1.2.3&ISO")],
    )?;
    assert!(matches!(
        Profile::receive().validate_segment(
            unknown.segments().nth(1).ok_or("fixture")?,
            ControlDirection::Send
        ),
        Err(Error::Authority {
            segment: "OBR",
            field: 16,
            component: 9
        })
    ));
    Ok(())
}

#[test]
fn law_tf2b_c14_tq1_9_r_priority_cwe_subset_and_option() -> TestResult {
    for value in ["R^^HL70485", "S^Synthetic^HL70485", "local^^99LAB"] {
        both("TQ1", &[(9, value)], true)?;
    }
    for value in [
        "",
        "\"\"",
        "P^^HL70485",
        "id",
        "^Text^99LAB",
        "id^^OTHER",
        "id^^99ABCD",
        "id^^99A!B",
        "id^^99LAB~other^^99LAB",
        "id&x^^99LAB",
    ] {
        both("TQ1", &[(9, value)], false)?;
    }
    both(
        "TQ1",
        &[(
            9,
            &format!("{}^{}^99LAB", "i".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    both("TQ1", &[(9, &format!("{}^^99LAB", "i".repeat(21)))], false)?;
    both(
        "TQ1",
        &[(9, &format!("id^{}^99LAB", "x".repeat(200)))],
        false,
    )?;
    check(
        "TQ1",
        &[(9, "ignored")],
        Profile::receive().with_priority(false),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "TQ1",
            &[],
            Profile::receive().with_priority(false),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c13_tcd_2_3_5_sn_ratio_predicates_and_direction() -> TestResult {
    for field in [2, 3, 5] {
        for value in [
            "",
            "^1^:^5.50",
            "^1^:^.5",
            "^1^:^+5",
            "^1^:^123456789012345",
        ] {
            check(
                "TCD",
                &[(field, value)],
                Profile::receive(),
                ControlDirection::Send,
            )?;
        }
        for value in [
            "\"\"",
            "<^1^:^5",
            "^2^:^5",
            "^1^/^5",
            "^1^:^",
            "^1^:^0",
            "^1^:^-1",
            "^1^:^1e3",
            "^1^:^1.2.3",
            "^1^:^1234567890123456",
            "^1^:^5~^1^:^5",
            "^1^:^5&bad",
        ] {
            assert!(
                check(
                    "TCD",
                    &[(field, value)],
                    Profile::receive(),
                    ControlDirection::Send
                )
                .is_err()
            );
            if field != 2 {
                check(
                    "TCD",
                    &[(field, value)],
                    Profile::receive(),
                    ControlDirection::Receive,
                )?;
            }
        }
    }
    for value in ["^1^+", "^1^-"] {
        check(
            "TCD",
            &[(2, value)],
            Profile::receive(),
            ControlDirection::Send,
        )?;
        assert!(
            check(
                "TCD",
                &[(2, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    for value in ["^1^+^5", "^1^-^5", "^1^+^\"\""] {
        assert!(
            check(
                "TCD",
                &[(2, value)],
                Profile::receive(),
                ControlDirection::Send
            )
            .is_err()
        );
    }
    check(
        "TCD",
        &[(2, "^1^:^5.50")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    for value in ["^1^:^0", "^1^:^NaN", "^1^:^5~^1^:^5"] {
        assert!(
            check(
                "TCD",
                &[(2, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c13_tcd_6_7_r_permissions_8_r_repeat_status() -> TestResult {
    let permissions = Profile::receive().with_repeat_options(false, true);
    for first in ["Y", "N"] {
        for second in ["Y", "N"] {
            check(
                "TCD",
                &[(6, first), (7, second)],
                permissions,
                ControlDirection::Send,
            )?;
        }
    }
    for field in [6, 7] {
        for value in ["", "\"\"", "X", "YY", "Y~Y", "Y^bad", "Y&bad"] {
            assert!(
                check(
                    "TCD",
                    &[(6, "Y"), (7, "N"), (field, value)],
                    permissions,
                    ControlDirection::Send
                )
                .is_err()
            );
            check(
                "TCD",
                &[(field, value)],
                permissions,
                ControlDirection::Receive,
            )?;
        }
    }
    let repeat = Profile::receive().with_repeat_options(true, false);
    for code in ["O", "R", "D", "F"] {
        check(
            "TCD",
            &[(8, &format!("{code}^Synthetic^HL70389"))],
            repeat,
            ControlDirection::Send,
        )?;
    }
    for value in [
        "",
        "\"\"",
        "X^Text^HL70389",
        "O^^HL70389",
        "O^Text^99LAB",
        "O^Text^HL70389~O^Text^HL70389",
        "O&bad^Text^HL70389",
    ] {
        assert!(check("TCD", &[(8, value)], repeat, ControlDirection::Send).is_err());
        check("TCD", &[(8, value)], repeat, ControlDirection::Receive)?;
    }
    assert!(
        check(
            "TCD",
            &[(6, "Y"), (7, "N"), (8, "O^Text^HL70389")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c13_tcd_9_re_cq_ucum_units_and_10_pool_size() -> TestResult {
    for value in [
        "",
        "1^mL&&UCUM",
        "-1.25^mL&Description&UCUM",
        "1234567890123456^mL&&UCUM",
    ] {
        manager("TCD", &[(9, value)], true)?;
    }
    for value in [
        "\"\"",
        "1",
        "1^mL",
        "1^&Text&UCUM",
        "1^mL&&OTHER",
        "x^mL&&UCUM",
        "1e3^mL&&UCUM",
        "12345678901234567^mL&&UCUM",
        "1^mL&&UCUM~1^mL&&UCUM",
    ] {
        manager("TCD", &[(9, value)], false)?;
    }
    manager(
        "TCD",
        &[(
            9,
            &format!("1^{}&{}&UCUM", "u".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    manager("TCD", &[(9, &format!("1^{}&&UCUM", "u".repeat(21)))], false)?;
    manager(
        "TCD",
        &[(9, &format!("1^mL&{}&UCUM", "d".repeat(200)))],
        false,
    )?;
    let pool = Profile::receive().with_pooling_analyzer(true);
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        for value in ["1", "0002", "9999999999999999"] {
            check("TCD", &[(10, value)], pool, direction)?;
        }
        for value in [
            "",
            "\"\"",
            "0",
            "-1",
            "1.5",
            "10000000000000000",
            "2~2",
            "2^bad",
            "2&bad",
        ] {
            assert!(check("TCD", &[(10, value)], pool, direction).is_err());
        }
    }
    assert!(
        check(
            "TCD",
            &[(10, "2")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "TCD",
        &[(10, "ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c13_tcd_11_re_cwe_dilution_option_and_either_factor_or_type() -> TestResult {
    for value in ["", "D1^^99LAB", "D1^Description^99LAB"] {
        both("TCD", &[(11, value)], true)?;
    }
    for value in [
        "\"\"",
        "D1",
        "^Text^99LAB",
        "D1^^OTHER",
        "D1^^99A!B",
        "D1^^99LAB~D1^^99LAB",
        "D1&bad^^99LAB",
    ] {
        both("TCD", &[(11, value)], false)?;
    }
    both(
        "TCD",
        &[(
            11,
            &format!("{}^{}^99LAB", "d".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    both("TCD", &[(11, &format!("{}^^99LAB", "d".repeat(21)))], false)?;
    both(
        "TCD",
        &[(11, &format!("D1^{}^99LAB", "d".repeat(200)))],
        false,
    )?;
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        assert!(
            check(
                "TCD",
                &[],
                Profile::receive().with_dilution(true),
                direction
            )
            .is_err()
        );
        check(
            "TCD",
            &[(2, "^1^:^5")],
            Profile::receive().with_dilution(true),
            direction,
        )?;
        check(
            "TCD",
            &[(11, "D1^^99LAB")],
            Profile::receive().with_dilution(true),
            direction,
        )?;
        check(
            "TCD",
            &[(2, "ignored"), (11, "ignored")],
            Profile::receive().with_dilution(false),
            ControlDirection::Receive,
        )?;
    }
    assert!(
        check(
            "TCD",
            &[(2, "^1^:^5")],
            Profile::receive().with_dilution(false),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c13_tcd_2_11_consistency_requires_test_specific_host_definition() -> TestResult {
    let bindings = [law::DilutionBinding::new(
        TEST.as_bytes(),
        b"D1^^99LAB",
        b"^1^:^5.00",
    )?];
    let profile = Profile::receive().with_dilution_bindings(&bindings);
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        check(
            "TCD",
            &[(2, "^1^:^5.00"), (11, "D1^^99LAB")],
            profile,
            direction,
        )?;
        assert!(
            check(
                "TCD",
                &[(2, "^1^:^6"), (11, "D1^^99LAB")],
                profile,
                direction
            )
            .is_err()
        );
        assert!(
            check(
                "TCD",
                &[(2, "^1^:^5.00"), (11, "D2^^99LAB")],
                profile,
                direction
            )
            .is_err()
        );
        assert!(
            check(
                "TCD",
                &[(2, "^1^:^5.00"), (11, "D1^^99LAB")],
                Profile::receive(),
                direction
            )
            .is_err()
        );
    }
    assert!(law::DilutionBinding::new(TEST.as_bytes(), b"D1^^99LAB", b"^1^:^0").is_err());
    let conflicting = [
        bindings.first().copied().ok_or("fixture")?,
        law::DilutionBinding::new(TEST.as_bytes(), b"D1^^99LAB", b"^1^:^6")?,
    ];
    assert!(
        check(
            "TCD",
            &[(2, "^1^:^5.00"), (11, "D1^^99LAB")],
            Profile::receive().with_dilution_bindings(&conflicting),
            ControlDirection::Receive
        )
        .is_err()
    );
    Ok(())
}

fn result_wire(detail: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut fields = [""; 29];
    for (position, value) in [
        (1_usize, "1"),
        (2, "NM"),
        (3, TEST),
        (4, "1"),
        (5, "1.20"),
        (6, "mg/L^mg/L^UCUM"),
        (8, "N^Synthetic^HL70078"),
        (11, "F"),
        (16, "operator^Synthetic"),
        (18, "instrument^99LAB~module^99LAB"),
        (19, "20260101120000"),
        (29, "RSLT"),
    ] {
        *fields
            .get_mut(position.checked_sub(1).ok_or("fixture position")?)
            .ok_or("fixture")? = value;
    }
    Ok(format!(
        "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|r1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample&Local||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||{TEST}\rORC|SC||||CM\rOBX|{}\r{detail}",
        fields.join("|")
    ))
}

#[test]
fn law_tf2b_c13_safety_rule_6_reported_dilution_keeps_original_and_explicit_adjustment()
-> TestResult {
    use law::{DilutionMeaning, ResultAdjustment, Value};
    let source = result_wire(&format!("TCD|{TEST}|^1^:^05.5000\r"))?;
    let message = law::parse_message(source.as_bytes(), options())?;
    for adjustment in [
        ResultAdjustment::Unknown,
        ResultAdjustment::AlreadyApplied,
        ResultAdjustment::NotApplied,
    ] {
        let results = law::Results::with_profile(
            &message,
            Profile::receive().with_result_adjustment(adjustment),
        )?;
        let observation = results.observations().first().ok_or("fixture")?;
        let factor = observation
            .test_detail()
            .ok_or("TCD")?
            .auto_dilution()
            .ok_or("factor")?;
        assert_eq!(factor.raw(), b"^1^:^05.5000");
        assert_eq!(
            factor.meaning(),
            DilutionMeaning::ReportedForRun { adjustment }
        );
        assert_eq!(
            factor.requires_reconciliation(),
            adjustment == ResultAdjustment::Unknown
        );
        assert!(matches!(
            observation.values().first(),
            Some(Value::Number(b"1.20"))
        ));
        assert_eq!(results.message().raw(), source.as_bytes());
    }
    let default = law::Results::new(&message)?;
    assert!(
        default
            .observations()
            .first()
            .ok_or("fixture")?
            .test_detail()
            .ok_or("TCD")?
            .auto_dilution()
            .ok_or("factor")?
            .requires_reconciliation()
    );
    let ignored = law::Results::with_profile(&message, Profile::receive().with_dilution(false))?;
    let detail = ignored
        .observations()
        .first()
        .ok_or("fixture")?
        .test_detail()
        .ok_or("TCD")?;
    assert!(detail.auto_dilution().is_none());
    assert_eq!(
        detail.segment().field(2).map(analyte_hl7::Field::raw),
        Some(b"^1^:^05.5000".as_slice())
    );
    Ok(())
}

#[test]
fn law_tf2b_c13_safety_rule_6_requests_and_diluent_are_never_reported_adjustments() -> TestResult {
    use law::{DilutionMeaning, TestDetail};
    let message = message(
        "TCD",
        ControlDirection::Send,
        &[(2, "^1^:^5.50"), (3, "^1^:^7.00"), (5, "^1^:^2.25")],
    )?;
    let detail = TestDetail::new(
        message.segments().nth(1).ok_or("fixture")?,
        Profile::receive(),
        ControlDirection::Send,
    )?;
    for (dilution, spelling, meaning) in [
        (
            detail.auto_dilution(),
            b"^1^:^5.50".as_slice(),
            DilutionMeaning::RequestedForRun,
        ),
        (
            detail.rerun_dilution(),
            b"^1^:^7.00",
            DilutionMeaning::RequestedForRerun,
        ),
        (
            detail.prediluent_content(),
            b"^1^:^2.25",
            DilutionMeaning::PrediluentContent,
        ),
    ] {
        let dilution = dilution.ok_or("factor")?;
        assert_eq!(dilution.raw(), spelling);
        assert_eq!(dilution.meaning(), meaning);
        assert!(!dilution.requires_reconciliation());
    }
    let received = TestDetail::new(
        message.segments().nth(1).ok_or("fixture")?,
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(received.rerun_dilution().is_none());
    assert!(received.prediluent_content().is_none());
    Ok(())
}

#[test]
fn law_tf2b_c13_c14_selected_options_require_details_and_priority_in_received_results() -> TestResult
{
    let empty = law::parse_message(result_wire("")?.as_bytes(), options())?;
    assert!(law::Results::with_profile(&empty, Profile::receive().with_dilution(true)).is_err());
    let present = law::parse_message(
        result_wire(&format!("TCD|{TEST}|^1^:^5\r"))?.as_bytes(),
        options(),
    )?;
    law::Results::with_profile(&present, Profile::receive().with_dilution(true))?;
    assert!(law::Results::with_profile(&present, Profile::receive().with_priority(true)).is_err());
    let source = std::str::from_utf8(present.raw())?
        .replace("ORC|SC||||CM\r", "ORC|SC||||CM\rTQ1|||||||||S^^HL70485\r");
    let priority = law::parse_message(source.as_bytes(), options())?;
    law::Results::with_profile(
        &priority,
        Profile::receive().with_priority(true).with_dilution(true),
    )?;
    let inconsistent = law::parse_message(
        source
            .replace(&format!("TCD|{TEST}"), "TCD|other^Synthetic^99LAB")
            .as_bytes(),
        options(),
    )?;
    let failure = law::Results::with_profile(&inconsistent, Profile::receive())
        .err()
        .ok_or("accepted inconsistent TCD")?;
    assert!(
        matches!(failure,Error::AtSegment {occurrence:1,source} if matches!(*source,Error::Inconsistent {segment:"TCD",field:1}))
    );
    Ok(())
}

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
    use law::{Address, Awos, Capabilities, Instructions, OrderDetails, WorkAction};
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
        dilution: true,
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
    let invalid_group = OrderDetails {
        group: Some(b"group&namespace"),
        ..OrderDetails::default()
    };
    assert!(
        law::work_order(
            &[work.with_order_details(&invalid_group)],
            None,
            address,
            response,
            capabilities,
            options()
        )
        .is_err()
    );
    let permission_caps = Capabilities {
        repeat_control: true,
        ..Capabilities::default()
    };
    assert!(law::work_order(&[awos], None, address, response, permission_caps, options()).is_err());
    let permissions = awos.with_instructions(Instructions {
        automatic_repeat: Some(false),
        reflex: Some(false),
        ..Instructions::default()
    });
    law::work_order(
        &[permissions],
        None,
        address,
        response,
        permission_caps,
        options(),
    )?;
    let repeat_caps = Capabilities {
        manager_repeat: true,
        ..Capabilities::default()
    };
    assert!(law::work_order(&[awos], None, address, response, repeat_caps, options()).is_err());
    law::work_order(
        &[awos.with_instructions(Instructions {
            repeat: Some(b"O^Synthetic^HL70389"),
            ..Instructions::default()
        })],
        None,
        address,
        response,
        repeat_caps,
        options(),
    )?;
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
