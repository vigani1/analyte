//! Tests LAW order, timing and test-detail segments.
use super::*;

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
fn law_tf2b_c13_9_repeat_status_text_matches_code() -> TestResult {
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
    for (code, text) in [
        ("O", "Original, first run"),
        ("R", "Repeated without dilution"),
        ("D", "Repeated with dilution"),
        ("F", "Reflex test"),
    ] {
        check(
            "TCD",
            &[(8, &format!("{code}^{text}^HL70389"))],
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
