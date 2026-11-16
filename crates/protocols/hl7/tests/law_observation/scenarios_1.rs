//! Tests law tf2b c7 obx 1 si and 4 og lengths nulls and manager re; law tf2b c7 obx 3 cwe alternate predicates and namespaces.
use super::*;

#[test]
fn law_tf2b_c7_obx_1_si_and_4_og_lengths_nulls_and_manager_re() -> TestResult {
    for value in ["1", "9999"] {
        both("OBX", &[(1, value)], true)?;
    }
    for value in ["", "0", "-1", "1.0", "10000", "\"\"", "1~2", "1^2"] {
        both("OBX", &[(1, value)], false)?;
    }
    for value in ["1", "12345678901234567890", "1^99999^99999"] {
        both("OBX", &[(4, value)], true)?;
    }
    for value in [
        "0",
        "-1",
        "word",
        "1^0",
        "1^1.5",
        "1^100000",
        "1^1^100000",
        "\"\"",
        "1~2",
    ] {
        both("OBX", &[(4, value)], false)?;
    }
    check(
        "OBX",
        &[(4, "")],
        Profile::receive(),
        ControlDirection::Send,
    )?;
    assert!(
        check(
            "OBX",
            &[(4, "")],
            Profile::receive(),
            ControlDirection::Receive
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_3_cwe_alternate_predicates_and_namespaces() -> TestResult {
    for system in ["LN", "JC10", "99LAB"] {
        both("OBX", &[(3, &format!("id^Synthetic^{system}"))], true)?;
    }
    for code in ["S_IMAGE", "S_GRAPH", "S_RAW", "S_OTHER"] {
        both(
            "OBX",
            &[(3, &format!("{TEST}^{code}^Synthetic^IHELAW"))],
            true,
        )?;
    }
    both(
        "OBX",
        &[(3, &format!("{TEST}^alternate^Text^LN^^^^second^Text^JC10"))],
        true,
    )?;
    both(
        "OBX",
        &[(
            3,
            &format!("{}^{}^99LAB", "x".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    for value in [
        "",
        "\"\"",
        "id^^LN",
        "id^Text^OTHER",
        "id^Text^99AB",
        "id^Text^99A!B",
        "id&bad^Text^LN",
        "id^Text^LN~id^Text^LN",
        "id^Text^LN^alt^^LN",
        "id^Text^LN^alt^Text^OTHER",
        "id^Text^LN^S_BAD^Text^IHELAW",
        "id^Text^LN^^^^^^^alt^^LN",
    ] {
        both("OBX", &[(3, value)], false)?;
    }
    for value in [
        format!("{}^Text^LN", "x".repeat(21)),
        format!("id^{}^LN", "x".repeat(200)),
        format!("{TEST}^{}^Text^LN", "x".repeat(21)),
        format!("{TEST}^alt^{}^LN", "x".repeat(200)),
        format!("{TEST}^^^^^^^{}^Text^LN", "x".repeat(21)),
    ] {
        both("OBX", &[(3, &value)], false)?;
    }
    // Absent alternate IDs make their text and namespaces X on receive.
    check(
        "OBX",
        &[(3, &format!("{TEST}^^ignored^ignored"))],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(3, &format!("{TEST}^^ignored^ignored"))],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_2_5_null_and_supplemental_repetition_predicates() -> TestResult {
    both("OBX", &[(2, ""), (5, "\"\""), (6, "")], true)?;
    for value in ["", "\"\"~\"\""] {
        both("OBX", &[(5, value)], false)?;
    }
    for kind in ["", "FT", "CWE", "DT", "ZZ", "NM~NM"] {
        both("OBX", &[(2, kind)], false)?;
    }
    check(
        "OBX",
        &[(2, "ignored"), (5, "\"\""), (6, "ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(2, "NM"), (5, "\"\""), (6, "")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    both("OBX", &[(5, "1~2")], false)?;
    for code in ["S_RAW", "S_OTHER"] {
        both(
            "OBX",
            &[(3, &format!("{TEST}^{code}^Synthetic^IHELAW")), (5, "1~2")],
            true,
        )?;
    }
    for code in [
        format!("{TEST}^S_RAW^Synthetic^99LAB"),
        format!("{TEST}^S_IMAGE^Synthetic^IHELAW"),
    ] {
        both("OBX", &[(3, &code), (5, "1~2")], false)?;
    }
    both(
        "OBX",
        &[(3, &format!("{TEST}^S_RAW^Synthetic^IHELAW")), (5, "1~")],
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_5_nm_sn_byte_exact_and_invalid_numeric_tokens() -> TestResult {
    for value in ["+0001.2000", "-.01", "1234567890123456"] {
        both("OBX", &[(5, value)], true)?;
    }
    for value in [
        "<0.1",
        "1,234",
        "positive",
        "ERR42",
        "NaN",
        "1e3",
        "12345678901234567",
    ] {
        both("OBX", &[(5, value)], false)?;
    }
    for value in [
        "<^0.1",
        ">=^+0001.2000",
        "=^1^:^2",
        "<>^1^.^2",
        "<^123456789012345",
    ] {
        both("OBX", &[(2, "SN"), (5, value)], true)?;
    }
    for value in [
        "^1",
        "bad^1",
        "<^1,234",
        "<^1234567890123456",
        "<^1^x^2",
        "<^1^:^1234567890123456",
        "<^1^:^word",
        "<^1&bad",
    ] {
        both("OBX", &[(2, "SN"), (5, value)], false)?;
    }
    let message = result_message(&[(2, "SN"), (5, "<^0.1")], "")?;
    let results = law::Results::new(&message)?;
    assert_eq!(
        results
            .observations()
            .first()
            .and_then(|o| o.values().first())
            .map(|v| v.raw()),
        Some(b"<^0.1".as_slice())
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_5_st_tx_and_rule5_original_values_units() -> TestResult {
    for value in ["<0.1", "1,234", "positive", "ERR42", " 001.20 "] {
        let message = result_message(&[(2, "ST"), (5, value), (6, "")], "")?;
        let results = law::Results::new(&message)?;
        assert_eq!(
            results
                .observations()
                .first()
                .and_then(|o| o.values().first())
                .map(|v| v.raw()),
            Some(value.as_bytes())
        );
        assert_eq!(message.raw(), message.encode());
    }
    for (kind, max) in [("ST", 200), ("TX", 65536)] {
        both(
            "OBX",
            &[(2, kind), (5, &"\u{e9}".repeat(max)), (6, "")],
            true,
        )?;
        both(
            "OBX",
            &[
                (2, kind),
                (5, &"x".repeat(max.checked_add(1).ok_or("fixture")?)),
                (6, ""),
            ],
            false,
        )?;
        both(
            "OBX",
            &[(2, kind), (5, "unescaped^component"), (6, "")],
            false,
        )?;
    }
    let message = result_message(&[(5, "+0001.2000"), (6, "mg/L^original unit^UCUM")], "")?;
    let results = law::Results::new(&message)?;
    let result = results.observations().first().ok_or("fixture")?;
    assert_eq!(
        result.values().first().map(|v| v.raw()),
        Some(b"+0001.2000".as_slice())
    );
    assert_eq!(
        result.units().map(analyte_hl7::Field::raw),
        Some(b"mg/L^original unit^UCUM".as_slice())
    );
    let bad = result_message(&[(5, "1,234")], "")?;
    let original = bad.encode();
    assert!(law::Results::new(&bad).is_err());
    assert_eq!(bad.raw(), original);
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_5_ce_ei_na_components_and_lengths() -> TestResult {
    both(
        "OBX",
        &[(2, "CE"), (5, "error^Synthetic failure^99LAB"), (6, "")],
        true,
    )?;
    both(
        "OBX",
        &[
            (2, "CE"),
            (
                5,
                &format!(
                    "{}^{}^{}",
                    "x".repeat(20),
                    "\u{e9}".repeat(199),
                    "s".repeat(12)
                ),
            ),
            (6, ""),
        ],
        true,
    )?;
    for value in [
        "code",
        "^Text^system",
        "code^^system",
        "code^Text^",
        "code&bad^Text^system",
    ] {
        both("OBX", &[(2, "CE"), (5, value), (6, "")], false)?;
    }
    for value in [
        format!("{}^Text^s", "x".repeat(21)),
        format!("id^{}^s", "x".repeat(200)),
        format!("id^Text^{}", "x".repeat(13)),
    ] {
        both("OBX", &[(2, "CE"), (5, &value), (6, "")], false)?;
    }
    both(
        "OBX",
        &[(2, "EI"), (5, &"\u{e9}".repeat(199)), (6, "")],
        true,
    )?;
    both("OBX", &[(2, "EI"), (5, &"x".repeat(200)), (6, "")], false)?;
    both(
        "OBX",
        &[(2, "NA"), (5, "1^^-2.00^1234567890123456"), (6, "")],
        true,
    )?;
    for value in ["^1", "1^word", "1^12345678901234567", "1^2&bad"] {
        both("OBX", &[(2, "NA"), (5, value), (6, "")], false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_5_ed_hd_encoding_and_payload_limits() -> TestResult {
    for value in [
        "source^IM^JPEG^Base64^YQ==",
        "^IM^^A^synthetic",
        "^IM^JPEG^Hex^0A1b",
        "source^TEXT^XML^A^synthetic",
    ] {
        both("OBX", &[(2, "ED"), (5, value), (6, "")], true)?;
    }
    for value in [
        "^IM^JPEG^^data",
        "^IM^JPEG^OTHER^data",
        "^BAD^^A^text",
        "^IM^BAD^A^text",
        "^IM^^Hex^abc",
        "^IM^^Base64^a",
        "^IM^^Base64^ab=c",
        "^IM^^Base64^====",
        "^IM^^A^",
        "^IM^^A^\u{e9}",
    ] {
        both("OBX", &[(2, "ED"), (5, value), (6, "")], false)?;
    }
    for (value, valid) in [
        (format!("{}^IM^^A^text", "s".repeat(20)), true),
        (format!("{}^IM^^A^text", "s".repeat(21)), false),
        (format!("^IM^Z{}^A^text", "s".repeat(17)), true),
        (format!("^IM^Z{}^A^text", "s".repeat(18)), false),
    ] {
        both("OBX", &[(2, "ED"), (5, &value), (6, "")], valid)?;
    }
    let value = format!("^IM^^A^{}", "x".repeat(99992));
    both("OBX", &[(2, "ED"), (5, &value), (6, "")], true)?;
    both(
        "OBX",
        &[(2, "ED"), (5, &format!("{value}x")), (6, "")],
        false,
    )?;
    check(
        "OBX",
        &[(2, "ED"), (5, "source&ignored^IM^^A^data"), (6, "")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(2, "ED"), (5, "source&ignored^IM^^A^data"), (6, "")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_5_rp_nested_hd_limits_and_no_pointer_fetch() -> TestResult {
    for value in [
        "https://synthetic.invalid/result^^IM^JPEG",
        "pointer^Local^^vendor",
        "pointer^&1.2.3&ISO^^",
    ] {
        both("OBX", &[(2, "RP"), (5, value), (6, "")], true)?;
    }
    both("OBX", &[(2, "RP"), (5, &"x".repeat(999)), (6, "")], true)?;
    both("OBX", &[(2, "RP"), (5, &"x".repeat(1000)), (6, "")], false)?;
    for value in ["^Local", "pointer^&oid", "pointer^&oid&BAD"] {
        both("OBX", &[(2, "RP"), (5, value), (6, "")], false)?;
    }
    for value in [
        format!("pointer^{}", "x".repeat(21)),
        format!("pointer^&{}&URI", "x".repeat(200)),
        format!("pointer^^{}", "x".repeat(10)),
        format!("pointer^^^{}", "x".repeat(20)),
    ] {
        both("OBX", &[(2, "RP"), (5, &value), (6, "")], false)?;
    }
    let bindings = [law::AuthorityBinding::new(b"Local", b"1.2.3", b"ISO")?];
    let profile = Profile::receive().with_authorities(&bindings);
    for direction in [ControlDirection::Send, ControlDirection::Receive] {
        check(
            "OBX",
            &[(2, "RP"), (5, "pointer^Local&1.2.3&ISO"), (6, "")],
            profile,
            direction,
        )?;
        assert!(
            check(
                "OBX",
                &[(2, "RP"), (5, "pointer^Other&1.2.3&ISO"), (6, "")],
                profile,
                direction
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_6_units_identifier_namespace_predicate_and_null() -> TestResult {
    for value in ["\"\"", "^unit as supplied", "mg/L^unit as supplied^UCUM"] {
        both("OBX", &[(6, value)], true)?;
    }
    for value in [
        "",
        "id^^UCUM",
        "id^Text^OTHER",
        "id^Text",
        "^\"\"",
        "id^Text^UCUM~id^Text^UCUM",
    ] {
        both("OBX", &[(6, value)], false)?;
    }
    both(
        "OBX",
        &[(
            6,
            &format!("{}^{}^UCUM", "x".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    for value in [
        format!("{}^Text^UCUM", "x".repeat(21)),
        format!("id^{}^UCUM", "x".repeat(200)),
    ] {
        both("OBX", &[(6, &value)], false)?;
    }
    check(
        "OBX",
        &[(6, "^Text^ignored")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(6, "^Text^ignored")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    check(
        "OBX",
        &[(2, "ST"), (5, "positive"), (6, "ignored~units")],
        Profile::receive(),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(2, "ST"), (5, "positive"), (6, "ignored")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_7_range_and_8_repeated_cwe_or_null() -> TestResult {
    both("OBX", &[(7, &"\u{e9}".repeat(70))], true)?;
    for value in ["\"\"", "range~range", "a^b", &"x".repeat(71)] {
        both("OBX", &[(7, value)], false)?;
    }
    for value in ["\"\"", "N^^HL70078", "POS^Synthetic^HL70078~custom^^99LAB"] {
        both("OBX", &[(8, value)], true)?;
    }
    both(
        "OBX",
        &[(
            8,
            &format!("{}^{}^99LAB", "i".repeat(20), "\u{e9}".repeat(199)),
        )],
        true,
    )?;
    for value in [
        "",
        "N",
        "N^^OTHER",
        "N^\"\"^HL70078",
        "N^^HL70078~",
        "XX^^HL70078",
        "\"\"~N^^HL70078",
    ] {
        both("OBX", &[(8, value)], false)?;
    }
    for value in [
        format!("{}^^99LAB", "x".repeat(21)),
        format!("N^{}^HL70078", "x".repeat(200)),
    ] {
        both("OBX", &[(8, &value)], false)?;
    }
    // The complete required namespace cannot fit C.7-1's stale ID length of five.
    both("OBX", &[(8, "N^^HL70078")], true)?;
    Ok(())
}

#[test]
fn law_tf2b_c7_obx_9_mass_spec_probability_exact_range_option_and_manager_x() -> TestResult {
    for value in ["", "0", "1", ".5", "+1.00", "-0.00", "00001"] {
        check(
            "OBX",
            &[(9, value)],
            Profile::receive(),
            ControlDirection::Receive,
        )?;
    }
    for value in [
        "\"\"", "-0.01", "1.001", "2", "1,234", "0.1234", ".5~.5", "0^x",
    ] {
        assert!(
            check(
                "OBX",
                &[(9, value)],
                Profile::receive(),
                ControlDirection::Receive
            )
            .is_err()
        );
    }
    check(
        "OBX",
        &[(9, "ignored~values")],
        Profile::receive().with_mass_spectrometry(false),
        ControlDirection::Receive,
    )?;
    assert!(
        check(
            "OBX",
            &[(9, ".5")],
            Profile::receive(),
            ControlDirection::Send
        )
        .is_err()
    );
    Ok(())
}
