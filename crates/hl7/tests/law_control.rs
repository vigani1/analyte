//! Synthetic control-profile cases from `PaLM` TF-2b Rev 11 C.1/C.3/C.4,
//! 2.2.1-2.2.3 and HL7 2.5.1 chapter 2A. No external peer is contacted.
use analyte_hl7::{
    Charset, Message, Options,
    law::{self, ControlDirection},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}

fn wire(
    kind: &str,
    changes: &[(usize, &str)],
    body: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let (message_type, profile, accept, application) = match kind {
        "query" => ("QBP^Q11^QBP_Q11", "LAB-27^IHE", "NE", "AL"),
        "work" => ("OML^O33^OML_O33", "LAB-28^IHE", "NE", "AL"),
        "results" => ("OUL^R22^OUL_R22", "LAB-29^IHE", "NE", "AL"),
        "rsp" => ("RSP^K11^RSP_K11", "LAB-27^IHE", "", ""),
        "orl" => ("ORL^O34^ORL_O42", "LAB-28^IHE", "", ""),
        "ack" => ("ACK^R22^ACK", "LAB-29^IHE", "", ""),
        _ => return Err("unknown synthetic message kind".into()),
    };
    let mut fields = vec![""; 25];
    for (number, value) in [
        (2, "^~\\&"),
        (3, "Analyzer"),
        (4, "Lab"),
        (5, "Manager"),
        (6, "Lab"),
        (7, "20260228123059+0130"),
        (9, message_type),
        (10, "000Ab"),
        (11, "P"),
        (12, "2.5.1"),
        (15, accept),
        (16, application),
        (18, "UNICODE UTF-8"),
        (21, profile),
    ]
    .into_iter()
    .chain(changes.iter().copied())
    {
        *fields
            .get_mut(number)
            .ok_or("synthetic field out of range")? = value;
    }
    Ok(format!(
        "MSH|{}\r{body}",
        fields.get(2..).ok_or("synthetic header")?.join("|")
    ))
}

fn check(
    kind: &str,
    changes: &[(usize, &str)],
    body: &str,
    direction: ControlDirection,
) -> Result<Message, Box<dyn std::error::Error>> {
    let bytes = wire(kind, changes, body)?;
    let parsed = law::parse_message(bytes.as_bytes(), options())?;
    law::validate_control(&parsed, direction)?;
    assert_eq!(parsed.raw(), bytes.as_bytes());
    Ok(parsed)
}

fn both(kind: &str, changes: &[(usize, &str)], body: &str, valid: bool) -> TestResult {
    for direction in [ControlDirection::Receive, ControlDirection::Send] {
        assert_eq!(
            check(kind, changes, body, direction).is_ok(),
            valid,
            "kind={kind}, changes={changes:?}, direction={direction:?}, body={body}"
        );
    }
    Ok(())
}

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

fn error_body(field: usize, value: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut fields = vec![""; 13];
    *fields.get_mut(2).ok_or("synthetic location")? = "OBX^1^5^1^1^1";
    *fields.get_mut(3).ok_or("synthetic code")? = "100^^HL70357";
    *fields.get_mut(4).ok_or("synthetic severity")? = "E";
    *fields.get_mut(field).ok_or("synthetic field")? = value;
    Ok(format!(
        "MSA|AE|000Ab\rERR|{}\r",
        fields.get(1..).ok_or("synthetic ERR")?.join("|")
    ))
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

fn response<'a>(id: &'a [u8]) -> Result<analyte_hl7::Response<'a>, analyte_hl7::Error> {
    Ok(analyte_hl7::Response {
        control_id: law::control_id(id)?,
        timestamp: b"20260101123000+0000",
    })
}

fn query_with(changes: &[(usize, &str)]) -> Result<Message, Box<dyn std::error::Error>> {
    let body = "QPD|WOS^Synthetic^IHELAW|tag|000tube\rRCP|I||R^Real Time^HL70394\r";
    let bytes = wire("query", changes, body)?;
    Ok(law::parse_message(bytes.as_bytes(), options())?)
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

#[test]
fn law_tf2b_c1_query_builder_checks_generated_erl_code_and_mandatory_err() -> TestResult {
    use analyte_hl7::{AckError, Responsibility};
    let message = query_with(&[])?;
    let query = law::Query::new(&message)?;
    let good = AckError {
        segment: b"QPD",
        occurrence: 1,
        field: 3,
        code: b"207",
        application_code: b"synthetic-detail",
    };
    let reply = law::query_response(
        query,
        law::CacheAnswer::Unavailable(good),
        response(b"reply")?,
        Responsibility::Stored,
        options(),
    )?;
    law::validate_control(reply.message(), ControlDirection::Send)?;
    for error in [
        AckError {
            occurrence: 100,
            ..good
        },
        AckError { field: 100, ..good },
        AckError {
            code: b"999",
            ..good
        },
        AckError {
            segment: b"bad",
            ..good
        },
    ] {
        assert!(
            law::query_response(
                query,
                law::CacheAnswer::Unavailable(error),
                response(b"reply")?,
                Responsibility::Stored,
                options()
            )
            .is_err()
        );
    }
    // Each invalid response timestamp is rejected before any message is returned.
    let mut metadata = response(b"reply")?;
    metadata.timestamp = b"20260230120000+0000";
    assert!(
        law::query_response(
            query,
            law::CacheAnswer::NoWork,
            metadata,
            Responsibility::StoredAndInterpreted,
            options()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c4_work_builder_checks_all_four_hd_fields_and_response_id() -> TestResult {
    use law::{Address, Awos, Capabilities, WorkAction};
    let awos = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^Synthetic^HL70487",
        b"P",
        b"000tube",
        b"test^Synthetic^99LAB",
        WorkAction::New,
    )?;
    let base = Address {
        application: b"Manager",
        facility: b"Host",
        peer_application: b"Analyzer",
        peer_facility: b"Lab",
    };
    let capabilities = Capabilities {
        specimen: true,
        ..Capabilities::default()
    };
    let good = law::work_order(
        &[awos],
        None,
        base,
        response(b"new-id")?,
        capabilities,
        options(),
    )?;
    law::validate_control(&good, ControlDirection::Send)?;
    for address in [
        Address {
            application: b"123456789012345678901",
            ..base
        },
        Address {
            facility: b"123456789012345678901",
            ..base
        },
        Address {
            peer_application: b"123456789012345678901",
            ..base
        },
        Address {
            peer_facility: b"123456789012345678901",
            ..base
        },
        Address {
            application: b"Name^unsupported",
            ..base
        },
    ] {
        assert!(
            law::work_order(
                &[awos],
                None,
                address,
                response(b"new-id")?,
                capabilities,
                options()
            )
            .is_err()
        );
    }
    let unicode = "\u{e9}".repeat(50);
    let good = law::work_order(
        &[awos],
        None,
        base,
        response(unicode.as_bytes())?,
        capabilities,
        options(),
    );
    assert!(good.is_ok());
    assert!(law::control_id("\u{e9}".repeat(51).as_bytes()).is_err());
    Ok(())
}

#[test]
fn law_tf2b_c1_c3_c4_ack_builder_and_receive_correlation_and_err_profiles() -> TestResult {
    use analyte_hl7::{AckCode, AckError, Responsibility};
    let body = "SPM|1||\"\"|SER^Synthetic^HL70487|||||||P^Patient^HL70369\rSAC|||000tube\rORC|SC||||IP\rOBR||awos-1||test^Synthetic^99LAB\r";
    let source = wire("results", &[], body)?;
    let message = law::parse_message(source.as_bytes(), options())?;
    let detail = AckError {
        segment: b"OBX",
        occurrence: 1,
        field: 5,
        code: b"102",
        application_code: b"synthetic",
    };
    let ack = law::result_acknowledgment(
        &message,
        response(b"reply")?,
        AckCode::Ae,
        Responsibility::Stored,
        Some(detail),
        options(),
    )?
    .ok_or("missing ACK")?;
    law::validate_control(&ack, ControlDirection::Send)?;
    assert_eq!(law::receive_acknowledgment(&ack, &message)?, AckCode::Ae);
    for (needle, replacement) in [
        ("MSA|AE|000Ab", "MSA|AE|other-id"),
        ("102^^HL70357", "999^^HL70357"),
        ("|E|", "|W|"),
        ("OBX^1^5", "OBX^100^5"),
        ("ACK^R22^ACK", "ACK^O33^ACK"),
    ] {
        let text = std::str::from_utf8(ack.raw())?;
        assert!(text.contains(needle));
        let invalid = law::parse_message(text.replace(needle, replacement).as_bytes(), options())?;
        assert!(law::receive_acknowledgment(&invalid, &message).is_err());
    }
    let invalid = AckError {
        occurrence: 100,
        ..detail
    };
    assert!(
        law::result_acknowledgment(
            &message,
            response(b"reply")?,
            AckCode::Ae,
            Responsibility::Stored,
            Some(invalid),
            options()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_c4_c1_hl7_251_2_7_malformed_escapes_do_not_bypass_profile_validation() -> TestResult {
    let bytes = wire("query", &[(3, "bad\\")], "")?;
    let permissive = Options {
        escapes: analyte_hl7::EscapePolicy::Preserve,
        ..options()
    };
    let parsed = law::parse_message(bytes.as_bytes(), permissive)?;
    for direction in [ControlDirection::Receive, ControlDirection::Send] {
        assert!(law::validate_control(&parsed, direction).is_err());
    }
    assert!(law::control_id(b"bad\\").is_err());
    let escaped = "\\F\\".repeat(50);
    law::control_id(escaped.as_bytes())?;
    assert!(law::control_id(format!("{escaped}x").as_bytes()).is_err());
    Ok(())
}

#[test]
fn law_tf2b_c4_msh_single_header_and_configured_wire_repetition_bounds() -> TestResult {
    let bytes = wire("query", &[(21, "LAB-27^IHE~local^Lab")], "")?;
    let mut bounded = options();
    bounded.limits.message_bytes = bytes.len();
    bounded.limits.repetitions = 2;
    let message = law::parse_message(bytes.as_bytes(), bounded)?;
    law::validate_control(&message, ControlDirection::Receive)?;
    bounded.limits.repetitions = 1;
    assert!(law::parse_message(bytes.as_bytes(), bounded).is_err());
    bounded.limits.repetitions = 2;
    bounded.limits.message_bytes = bytes.len().checked_sub(1).ok_or("empty fixture")?;
    assert!(law::parse_message(bytes.as_bytes(), bounded).is_err());
    let doubled = format!("{bytes}{bytes}");
    // The shared syntax boundary rejects an embedded MSH before profile validation.
    assert!(matches!(
        law::parse_message(doubled.as_bytes(), options()),
        Err(analyte_hl7::Error::Segment(2))
    ));
    Ok(())
}

#[test]
fn law_tf2b_c4_c1_composite_field_lengths_include_empty_output_positions() -> TestResult {
    for (field, value, maximum) in [
        (3, "A", 227),
        (4, "A", 227),
        (5, "A", 227),
        (6, "A", 227),
        (7, "20260101120000+0000", 26),
        (9, "QBP^Q11^QBP_Q11", 15),
        (11, "P", 3),
        (12, "2.5.1", 60),
        (21, "LAB-27^IHE", 427),
    ] {
        let padded = format!("{value}{}", "^".repeat(maximum - value.len()));
        check("query", &[(field, &padded)], "", ControlDirection::Send)?;
        let oversized = format!("{padded}^");
        assert!(check("query", &[(field, &oversized)], "", ControlDirection::Send).is_err());
        check(
            "query",
            &[(field, &oversized)],
            "",
            ControlDirection::Receive,
        )?;
    }
    for (field, value, maximum) in [
        (2, "OBX^1", 18),
        (3, "100^^HL70357", 705),
        (5, "local^^99LAB", 705),
    ] {
        let padded = format!("{value}{}", "^".repeat(maximum - value.len()));
        check(
            "ack",
            &[],
            &error_body(field, &padded)?,
            ControlDirection::Send,
        )?;
        let oversized = format!("{padded}^");
        assert!(
            check(
                "ack",
                &[],
                &error_body(field, &oversized)?,
                ControlDirection::Send
            )
            .is_err()
        );
        check(
            "ack",
            &[],
            &error_body(field, &oversized)?,
            ControlDirection::Receive,
        )?;
    }
    Ok(())
}
