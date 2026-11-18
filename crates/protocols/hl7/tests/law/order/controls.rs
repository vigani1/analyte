//! Tests LAW order, timing and test-detail segments.
use super::*;

// IHE PaLM TF-2b §C.8
#[test]
fn order_control_and_status_are_closed_sets() -> TestResult {
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

// IHE PaLM TF-2b §C.8, ORC-1
#[test]
fn orc1_control_codes_by_direction_and_transaction() -> TestResult {
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

// IHE PaLM TF-2b §C.8, ORC-2, ORC-5
#[test]
fn orc2_awos_and_orc5_status_by_direction() -> TestResult {
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

// IHE PaLM TF-2b §C.8, ORC-4, ORC-8
#[test]
fn orc4_group_and_orc8_reflex_parents() -> TestResult {
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

// IHE PaLM TF-2b §C.8, ORC-9, ORC-27
#[test]
fn orc9_event_and_orc27_available_timestamps() -> TestResult {
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

// IHE PaLM TF-2b §C.8, ORC-21
#[test]
fn orc21_facility_name_kept_untruncated() -> TestResult {
    for value in ["", "Facility", &"\u{e9}".repeat(300)] {
        both("ORC", &[(21, value)], true)?;
    }
    for value in ["\"\"", "^type", "name&nested", "one~two"] {
        both("ORC", &[(21, value)], false)?;
    }
    manager("ORC", &[(21, "Name^ignored")], false)?;
    Ok(())
}

// IHE PaLM TF-2b §C.6, OBR-2, OBR-3
#[test]
fn obr2_obr3_id_limits_and_null_awos() -> TestResult {
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

// IHE PaLM TF-2b §C.6, §C.13, OBR-4, TCD-1
#[test]
fn obr4_tcd1_test_codes_and_coding_systems() -> TestResult {
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

// IHE PaLM TF-2b §C.6, OBR-11, OBR-49
#[test]
fn obr11_reflex_and_obr49_result_handling() -> TestResult {
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

// IHE PaLM TF-2b §C.6, OBR-16
#[test]
fn obr16_provider_components_and_authority() -> TestResult {
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
