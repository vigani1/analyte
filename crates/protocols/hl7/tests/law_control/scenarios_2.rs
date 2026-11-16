//! Tests law tf2b c1 query builder checks generated erl code and mandatory err; law tf2b c4 work builder checks all four hd fields and response id.
use super::*;

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
        content: law::PeerContent {
            specimen: true,
            ..Capabilities::default().content
        },
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
