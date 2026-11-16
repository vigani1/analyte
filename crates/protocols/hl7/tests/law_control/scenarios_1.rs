//! Tests LAW MSH, MSA and ERR segment validation.
use super::*;

#[test]
fn law_tf2b_c4_msh_1_2_recommended_delimiters() -> TestResult {
    both("query", &[], "", true)?;
    let good = wire("query", &[], "")?;
    for bytes in [
        good.replace('|', "#"),
        good.replace('^', "$"),
        good.replace('~', "%"),
        good.replace('\\', "!"),
        good.replace('&', "?"),
    ] {
        let message = law::parse_message(bytes.as_bytes(), options())?;
        for direction in [ControlDirection::Receive, ControlDirection::Send] {
            assert!(law::validate_control(&message, direction).is_err());
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_3_4_5_6_re_hd_namespace_usage_length_type_cardinality() -> TestResult {
    for field in [3, 4, 5, 6] {
        for value in [
            "",
            "Any laboratory",
            "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}",
            "A\\S\\B",
        ] {
            both("query", &[(field, value)], "", true)?;
        }
        for value in ["\"\"", "123456789012345678901", "a~b", "a&b"] {
            both("query", &[(field, value)], "", false)?;
        }
        let missing_namespace = check(
            "query",
            &[(field, "^ignored")],
            "",
            ControlDirection::Receive,
        );
        assert_eq!(missing_namespace.is_ok(), field == 6);
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_7_m_ts_precision_calendar_timezone_and_singleton() -> TestResult {
    for value in ["20240229235959-2359", "20260101000000+0000"] {
        both("query", &[(7, value)], "", true)?;
    }
    for value in [
        "",
        "\"\"",
        "20260229120000+0000",
        "00000101120000+0000",
        "20260001120000+0000",
        "20260100120000+0000",
        "20260101240000+0000",
        "20260101126000+0000",
        "20260101120060+0000",
        "20260101120000+2400",
        "20260101120000+0060",
        "20260101120000",
        "202601011200+0000",
        "20260101120000.1+0000",
        "20260101120000+0000~20260101120000+0000",
    ] {
        both("query", &[(7, value)], "", false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_9_m_msg_components_tables_lengths_and_cardinality() -> TestResult {
    for kind in ["query", "work", "results", "rsp", "orl", "ack"] {
        let body = if matches!(kind, "rsp" | "orl" | "ack") {
            "MSA|AA|000Ab\r"
        } else {
            ""
        };
        both(kind, &[], body, true)?;
    }
    for value in [
        "",
        "\"\"",
        "QBP",
        "QBP^^QBP_Q11",
        "^Q11^QBP_Q11",
        "QBP^Q11",
        "QBPQ^Q11^QBP_Q11",
        "QBP^Q111^QBP_Q11",
        "QBP^Q11^QBP_Q111",
        "QBP^Q11^WRONG",
        "XXX^Q11^QBP_Q11",
        "QBP^XXX^QBP_Q11",
        "QBP^Q11^QBP_Q11~QBP^Q11^QBP_Q11",
        "QBP&x^Q11^QBP_Q11",
    ] {
        both("query", &[(9, value)], "", false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_10_m_st_and_c3_msa_2_m_st_length_null_cardinality() -> TestResult {
    for field in [10, 0] {
        for (value, valid) in [
            ("", false),
            ("\"\"", false),
            ("Ab^Cd", false),
            ("Ab&Cd", false),
            ("Ab~Cd", false),
            ("000Ab", true),
            ("with space", true),
            ("identifier\\F\\suffix", true),
            ("\u{0085}", false),
        ] {
            if field == 10 {
                both("query", &[(10, value)], "", valid)?;
            } else {
                both("ack", &[], &format!("MSA|AA|{value}\r"), valid)?;
            }
        }
        for size in [50, 51] {
            let value = "\u{e9}".repeat(size);
            if field == 10 {
                both("query", &[(10, &value)], "", size == 50)?;
            } else {
                both("ack", &[], &format!("MSA|AA|{value}\r"), size == 50)?;
            }
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_11_m_pt_production_only() -> TestResult {
    both("query", &[(11, "P")], "", true)?;
    for value in ["", "\"\"", "T", "D", "PP", "P~P", "P&x"] {
        both("query", &[(11, value)], "", false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_12_m_vid_compatible_25_versions_and_base_policy_unchanged() -> TestResult {
    for version in ["2.5", "2.5.1", "2.5.2"] {
        both("query", &[(12, version)], "", true)?;
        let bytes = wire("query", &[(12, version)], "")?;
        assert_eq!(
            Message::parse(bytes.as_bytes(), options()).is_ok(),
            version == "2.5.1"
        );
    }
    for version in ["", "\"\"", "2.3.1", "2.6", "2.5.1~2.5.1", "2.5&bad"] {
        both("query", &[(12, version)], "", false)?;
    }
    // C.4's primitive field length overrides the underlying VID default.
    both(
        "query",
        &[(12, &format!("2.5{}", "1".repeat(57)))],
        "",
        true,
    )?;
    both(
        "query",
        &[(12, &format!("2.5{}", "1".repeat(58)))],
        "",
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_15_16_c_m_x_predicate_is_trigger_versus_response() -> TestResult {
    for kind in ["query", "work", "results"] {
        both(kind, &[], "", true)?;
        for field in [15, 16] {
            for value in ["", "\"\"", "ER", "SU", "AL~AL", "AL&bad", "LONG"] {
                both(kind, &[(field, value)], "", false)?;
            }
        }
        both(kind, &[(15, "AL")], "", false)?;
        both(kind, &[(16, "NE")], "", false)?;
    }
    for kind in ["rsp", "orl", "ack"] {
        both(kind, &[], "MSA|AA|000Ab\r", true)?;
        for field in [15, 16] {
            check(
                kind,
                &[(field, "ignored")],
                "MSA|AA|000Ab\r",
                ControlDirection::Receive,
            )?;
            assert!(
                check(
                    kind,
                    &[(field, "ignored")],
                    "MSA|AA|000Ab\r",
                    ControlDirection::Send
                )
                .is_err()
            );
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_18_m_utf8_singleton() -> TestResult {
    both("query", &[(18, "UNICODE UTF-8")], "", true)?;
    for value in [
        "",
        "\"\"",
        "ASCII",
        "UNICODE UTF-8~UNICODE UTF-8",
        "UNICODE UTF-8^bad",
    ] {
        both("query", &[(18, value)], "", false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_21_m_ei_required_components_lengths_first_transaction() -> TestResult {
    both("query", &[(21, "LAB-27^IHE~local^Lab")], "", true)?;
    let id = "\u{e9}".repeat(50);
    let namespace = "N".repeat(20);
    both(
        "query",
        &[(21, &format!("LAB-27^IHE~{id}^{namespace}"))],
        "",
        true,
    )?;
    for value in [
        "",
        "\"\"",
        "LAB-28^IHE",
        "local^Lab~LAB-27^IHE",
        "LAB-27^OTHER",
        "LAB-27",
        "LAB-27^IHE~",
        "LAB-27^IHE~local",
        "LAB-27^IHE~^Lab",
        "LAB-27^IHE~local^\"\"",
        "LAB-27^IHE~local&bad^Lab",
    ] {
        both("query", &[(21, value)], "", false)?;
    }
    both(
        "query",
        &[(21, &format!("LAB-27^IHE~{id}x^Lab"))],
        "",
        false,
    )?;
    both(
        "query",
        &[(21, &format!("LAB-27^IHE~local^{namespace}x"))],
        "",
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c3_msa_1_m_ack_code_subset_and_err_predicate() -> TestResult {
    for code in ["AA", "AE", "AR"] {
        let error = if code == "AA" {
            ""
        } else {
            "ERR|||100^^HL70357|E\r"
        };
        both("ack", &[], &format!("MSA|{code}|000Ab\r{error}"), true)?;
    }
    for code in ["", "\"\"", "CA", "CE", "CR", "AAA", "AA~AA", "AA^x", "AA&x"] {
        both("ack", &[], &format!("MSA|{code}|000Ab\r"), false)?;
    }
    for body in [
        "",
        "MSA|AA|000Ab\rMSA|AA|000Ab\r",
        "MSA|AE|000Ab\r",
        "MSA|AR|000Ab\r",
        "MSA|AA|000Ab\rERR|||100^^HL70357|E\r",
        "ERR|||100^^HL70357|E\rMSA|AE|000Ab\r",
    ] {
        both("ack", &[], body, false)?;
    }
    both("query", &[], "MSA|AA|000Ab\r", false)?;
    both("query", &[], "ERR|||100^^HL70357|E\r", false)?;
    Ok(())
}

#[test]
fn law_tf2b_c1_err_2_re_erl_r_re_components_lengths_positions_repetitions() -> TestResult {
    for value in [
        "",
        "OBX^1",
        "OBX^99^99^99^99^99",
        "OBX^1~SPM^2^4",
        "OBX^1^^^1",
    ] {
        both("ack", &[], &error_body(2, value)?, true)?;
    }
    for value in [
        "\"\"", "OBX", "^1", "OBXX^1", "obx^1", "OB^1", "OBX^0", "OBX^-1", "OBX^1.5", "OBX^1^x",
        "OBX^1~", "OBX&x^1",
    ] {
        both("ack", &[], &error_body(2, value)?, false)?;
    }
    for position in 2..=6 {
        for invalid in ["100", "x", "0", "\"\""] {
            let mut components = ["OBX", "1", "1", "1", "1", "1"];
            *components
                .get_mut(position - 1)
                .ok_or("synthetic component")? = invalid;
            both("ack", &[], &error_body(2, &components.join("^"))?, false)?;
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_c1_err_3_m_cwe_subset_components_lengths_and_singleton() -> TestResult {
    for code in [
        "100", "101", "102", "103", "200", "201", "202", "203", "207",
    ] {
        both(
            "ack",
            &[],
            &error_body(3, &format!("{code}^^HL70357"))?,
            true,
        )?;
    }
    both(
        "ack",
        &[],
        &error_body(3, &format!("100^{}^HL70357", "\u{e9}".repeat(199)))?,
        true,
    )?;
    for value in [
        "",
        "\"\"",
        "100",
        "100^^",
        "999^^HL70357",
        "^Text^HL70357",
        "100^^OTHER",
        "100^^HL703577",
        "100&x^^HL70357",
        "100^^HL70357~100^^HL70357",
    ] {
        both("ack", &[], &error_body(3, value)?, false)?;
    }
    both(
        "ack",
        &[],
        &error_body(3, &format!("100^{}^HL70357", "x".repeat(200)))?,
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c1_err_4_m_id_severity_e_only() -> TestResult {
    both("ack", &[], &error_body(4, "E")?, true)?;
    for value in ["", "\"\"", "W", "I", "EE", "EEE", "E~E", "E^x", "E&x"] {
        both("ack", &[], &error_body(4, value)?, false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_c1_err_5_re_cwe_r_o_r_components_vendor_namespace_lengths() -> TestResult {
    for value in ["", "local^^99ABC", "local^Description^99a1Z"] {
        both("ack", &[], &error_body(5, value)?, true)?;
    }
    both(
        "ack",
        &[],
        &error_body(
            5,
            &format!("{}^{}^99LAB", "\u{e9}".repeat(20), "\u{e9}".repeat(199)),
        )?,
        true,
    )?;
    for value in [
        "\"\"",
        "local",
        "^text^99LAB",
        "local^^OTHER",
        "local^^99AB",
        "local^^99ABCD",
        "local^^99A!C",
        "local^^99LAB~local^^99LAB",
        "local&x^^99LAB",
    ] {
        both("ack", &[], &error_body(5, value)?, false)?;
    }
    both(
        "ack",
        &[],
        &error_body(5, &format!("{}^^99LAB", "x".repeat(21)))?,
        false,
    )?;
    both(
        "ack",
        &[],
        &error_body(5, &format!("local^{}^99LAB", "x".repeat(200)))?,
        false,
    )?;
    Ok(())
}

#[test]
fn law_tf2b_c1_err_8_re_tx_250_hash_preserves_long_text_and_singleton() -> TestResult {
    for value in [
        "",
        "Synthetic detail",
        "escaped \\S\\ and \\F\\",
        &"\u{e9}".repeat(251),
    ] {
        both("ack", &[], &error_body(8, value)?, true)?;
    }
    for value in ["\"\"", "a~b", "a^b", "a&b", "\u{0085}"] {
        both("ack", &[], &error_body(8, value)?, false)?;
    }
    Ok(())
}

#[test]
fn law_tf2b_2_2_1_2_c1_c3_c4_x_fields_and_components_receive_ignore_send_omit() -> TestResult {
    for field in [8, 13, 14, 17, 19, 20, 22, 23, 24] {
        check(
            "query",
            &[(field, "ignored^data&here~too")],
            "",
            ControlDirection::Receive,
        )?;
        assert!(check("query", &[(field, "ignored")], "", ControlDirection::Send).is_err());
    }
    for (field, good, extra) in [
        (3, "A", "A^ignored"),
        (4, "A", "A^ignored"),
        (5, "A", "A^ignored"),
        (6, "A", "A^ignored"),
        (7, "20260101120000+0000", "20260101120000+0000^ignored"),
        (9, "QBP^Q11^QBP_Q11", "QBP^Q11^QBP_Q11^ignored"),
        (11, "P", "P^ignored"),
        (12, "2.5.1", "2.5.1^ignored^ignored"),
        (21, "LAB-27^IHE", "LAB-27^IHE^ignored^ignored"),
    ] {
        both("query", &[(field, good)], "", true)?;
        check("query", &[(field, extra)], "", ControlDirection::Receive)?;
        assert!(check("query", &[(field, extra)], "", ControlDirection::Send).is_err());
    }
    for field in [3, 4, 5, 6] {
        let body = format!("MSA|AA|000Ab{}ignored\r", "|".repeat(field - 2));
        check("ack", &[], &body, ControlDirection::Receive)?;
        assert!(check("ack", &[], &body, ControlDirection::Send).is_err());
    }
    for field in [1, 6, 7, 9, 10, 11, 12] {
        let body = error_body(field, "ignored")?;
        check("ack", &[], &body, ControlDirection::Receive)?;
        assert!(check("ack", &[], &body, ControlDirection::Send).is_err());
    }
    for (field, value) in [
        (2, "OBX^1^5^1^1^1^ignored"),
        (3, "100^^HL70357^ignored"),
        (5, "local^^99LAB^ignored"),
    ] {
        let body = error_body(field, value)?;
        check("ack", &[], &body, ControlDirection::Receive)?;
        assert!(check("ack", &[], &body, ControlDirection::Send).is_err());
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_c3_b5_receive_query_reply_reversal_correlation_ignored_content() -> TestResult {
    use analyte_hl7::{Field, Responsibility};
    let source_id = "\u{e9}".repeat(50);
    let message = query_with(&[
        (3, "Different analyzer^ignored"),
        (4, "Other facility^ignored"),
        (5, "Manager^ignored"),
        (6, "Host^ignored"),
        (10, &source_id),
        (11, "P^ignored"),
        (12, "2.5^ignored"),
        (20, "ignored"),
        (21, "LAB-27^IHE^ignored"),
    ])?;
    let query = law::receive_query(&message)?;
    let reply = law::query_response(
        query,
        law::CacheAnswer::NoWork,
        response(b"new-id")?,
        Responsibility::StoredAndInterpreted,
        options(),
    )?;
    law::validate_control(reply.message(), ControlDirection::Send)?;
    let msh = reply.message().segments().next().ok_or("missing MSH")?;
    for (number, expected) in [
        (3, b"Manager".as_slice()),
        (4, b"Host"),
        (5, b"Different analyzer"),
        (6, b"Other facility"),
        (11, b"P"),
        (12, b"2.5"),
        (15, b""),
        (16, b""),
        (20, b""),
    ] {
        assert_eq!(
            msh.field(number).map_or(b"".as_slice(), Field::raw),
            expected
        );
    }
    let msa = reply
        .message()
        .segments()
        .find(|segment| segment.id() == b"MSA")
        .ok_or("missing MSA")?;
    assert_eq!(msa.field(2).map(Field::raw), Some(source_id.as_bytes()));
    for (field, invalid) in [
        (3, "123456789012345678901"),
        (7, "20260101123000"),
        (11, "T"),
        (15, "AL"),
        (16, "NE"),
        (21, "LAB-28^IHE"),
    ] {
        assert!(law::receive_query(&query_with(&[(field, invalid)])?).is_err());
    }
    Ok(())
}
