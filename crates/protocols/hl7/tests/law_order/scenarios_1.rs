//! Tests LAW order, timing and test-detail segments.
use super::*;

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
    // ADR 0016 retains explicit null distinctly from an absent estimate (TF-2b C.8-9).
    for value in ["", "\"\"", "20240229", "20240229120000"] {
        check(
            "ORC",
            &[(27, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    for value in [
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
