//! Original synthetic cases: `PaLM` TF-2b C.2/C.5/C.7, section 2.2 and HL7 2.5.1 2A.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection, Error, Profile, ResultStatus},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const TEST: &str = "test^Synthetic^99LAB";

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}

fn fields(
    id: &str,
    direction: ControlDirection,
    changes: &[(usize, &str)],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut fields = [""; 40];
    let defaults = match id {
        "OBX" => vec![
            (1, "1"),
            (2, "NM"),
            (3, TEST),
            (4, "1"),
            (5, "1.20"),
            (6, "mg/L^original unit^UCUM"),
            (8, "N^^HL70078"),
            (11, "F"),
            (16, "operator"),
            (18, "model^maker~serial^maker"),
            (19, "20260101120000"),
            (29, "RSLT"),
        ],
        "NTE" => vec![
            (1, "1"),
            (
                2,
                if direction == ControlDirection::Send {
                    "A"
                } else {
                    "Z"
                },
            ),
            (3, "Synthetic comment"),
        ],
        "INV" => vec![
            (1, "substance^^99LAB"),
            (2, "NA^^HL70383"),
            (3, "MR^^HL70384"),
        ],
        _ => return Err("fixture segment".into()),
    };
    for (pos, value) in defaults.into_iter().chain(changes.iter().copied()) {
        *fields.get_mut(pos).ok_or("fixture field")? = value;
    }
    Ok(format!(
        "{id}|{}\r",
        fields.get(1..).ok_or("fixture")?.join("|")
    ))
}

fn message(body: &str) -> Result<Message, analyte_hl7::Error> {
    let wire = format!(
        "MSH|^~\\&|A|L|M|L|20260101120000+0000||OUL^R22^OUL_R22|m1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\r{body}"
    );
    law::parse_message(wire.as_bytes(), options())
}

fn check(
    id: &str,
    changes: &[(usize, &str)],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> TestResult {
    let message = message(&fields(id, direction, changes)?)?;
    let original = message.encode();
    let result = profile.validate_segment(message.segments().nth(1).ok_or("fixture")?, direction);
    assert_eq!(message.raw(), original);
    Ok(result?)
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

fn result_message(
    changes: &[(usize, &str)],
    suffix: &str,
) -> Result<Message, Box<dyn std::error::Error>> {
    message(&format!("SPM|1|sample&Local||SER^^HL70487|||||||P^^HL70369\rSAC|||tube\rOBR||awos-1||{TEST}\rORC|SC||||CM\r{}{suffix}",fields("OBX",ControlDirection::Receive,changes)?)).map_err(Into::into)
}

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
