//! Tests typed POCT Basic Profile messages.
use analyte_fixtures::ConfigEdit as _;
use analyte_fixtures::poct as peer_messages;
use analyte_poct::{Body, DataError, Message, MessageError, Nullable, Timestamp, Value};
use peer_messages::{field, hello, limits, message, observation, status};
use proptest::prelude::*;

// CLSI POCT01-A2 §B.4.4
#[test]
fn each_mandatory_body_has_exact_meaning() -> Result<(), MessageError> {
    for (kind, body) in [
        (
            "EOT.R01",
            format!("<EOT>{}</EOT>", field("EOT.topic_cd", "OBS")),
        ),
        (
            "END.R01",
            format!("<TRM>{}</TRM>", field("TRM.reason_cd", "NRM")),
        ),
        (
            "ACK.R01",
            format!(
                "<ACK>{}{}</ACK>",
                field("ACK.type_cd", "AA"),
                field("ACK.ack_control_id", "original")
            ),
        ),
        (
            "ACK.R01",
            format!(
                "<ACK>{}{}</ACK>",
                field("ACK.type_cd", "AE"),
                field("ACK.ack_control_id", "original")
            ),
        ),
        (
            "ESC.R01",
            format!(
                "<ESC>{}{}</ESC>",
                field("ESC.esc_control_id", "original"),
                field("ESC.detail_cd", "CNC")
            ),
        ),
    ] {
        let raw = message(kind, "000A", &body);
        let parsed = Message::parse(raw.as_bytes(), limits())?;
        match parsed.body() {
            Body::EndTopic(Nullable::Value(code)) => assert_eq!(code.value, "OBS"),
            Body::Terminate(Nullable::Value(code)) => assert_eq!(code.value, "NRM"),
            Body::Acknowledgement { kind, reference } => {
                assert_eq!(reference.as_str(), "original");
                assert_eq!(
                    *kind,
                    if body.contains("AE") {
                        analyte_poct::AckKind::Error
                    } else {
                        analyte_poct::AckKind::Accept
                    }
                );
            }
            Body::Escape { reference, reason } => {
                assert_eq!(reference.as_str(), "original");
                assert_eq!(reason, "CNC");
            }
            Body::Hello { .. }
            | Body::Status { .. }
            | Body::PatientObservations(_)
            | Body::NonPatientObservations(_)
            | Body::EndTopic(_)
            | Body::Terminate(_)
            | Body::Request(_)
            | Body::KeepAlive => return Err(MessageError::Structure),
        }
        assert_eq!(parsed.header().control_id.as_str(), "000A");
        assert_eq!(parsed.document().raw(), raw.as_bytes());
    }
    assert!(matches!(
        Message::parse(hello("h").as_bytes(), limits())?.body(),
        Body::Hello { .. }
    ));
    assert!(matches!(
        Message::parse(status("s", "2").as_bytes(), limits())?.body(),
        Body::Status { .. }
    ));
    Ok(())
}

// CLSI POCT01-A2 §B.6.10
#[test]
fn patient_and_nonpatient_values_are_explicit() -> Result<(), MessageError> {
    for patient in [true, false] {
        let raw = observation(
            "o",
            patient,
            "<OBS.value V=\"01.2300\" U=\"synthetic-unit\"/>",
        );
        let parsed = Message::parse(raw.as_bytes(), limits())?;
        let services = match parsed.body() {
            Body::PatientObservations(s) => {
                assert!(patient);
                s
            }
            Body::NonPatientObservations(s) => {
                assert!(!patient);
                s
            }
            Body::Hello { .. }
            | Body::Status { .. }
            | Body::EndTopic(_)
            | Body::Terminate(_)
            | Body::Acknowledgement { .. }
            | Body::Escape { .. }
            | Body::KeepAlive
            | Body::Request(_) => return Err(MessageError::Structure),
        };
        let result = services
            .first()
            .and_then(|s| s.observations.first())
            .ok_or(MessageError::Structure)?;
        assert!(
            matches!(&result.value,Value::Quantity(Nullable::Value(q)) if q.value == "01.2300" && q.unit == "synthetic-unit")
        );
        assert_eq!(result.interpretation.as_deref(), Some("<"));
    }
    Ok(())
}

// CLSI POCT01-A2 §B.8
#[test]
fn null_quantity_and_coded_values_are_validated() -> Result<(), MessageError> {
    for value in [
        "<OBS.value NULL=\"UNK\"/>",
        "<OBS.qualitative_value V=\"positive\" SN=\"synthetic\"/>",
    ] {
        Message::parse(observation("o", true, value).as_bytes(), limits())?;
    }
    for value in [
        "<OBS.value V=\"1\"/>",
        "<OBS.value V=\"1\" U=\"u\" NULL=\"UNK\"/>",
        "<OBS.value NULL=\"invalid\"/>",
        "<OBS.value value=\"1\" unit=\"u\"/>",
    ] {
        assert!(Message::parse(observation("o", true, value).as_bytes(), limits()).is_err());
    }
    Ok(())
}

// CLSI POCT01-A2 §B.6.12
#[test]
fn required_structure_and_order_are_enforced() {
    let raw = hello("h");
    let missing = raw.rewrite(&field("DEV.device_id", "A1-B2-C3-D4-E5-F6-07-08"), "");
    assert!(matches!(
        Message::parse(missing.as_bytes(), limits()),
        Err(MessageError::Missing("DEV.device_id"))
    ));
    let missing = raw.rewrite(&field("HDR.control_id", "h"), "");
    assert!(matches!(
        Message::parse(missing.as_bytes(), limits()),
        Err(MessageError::Missing("HDR.control_id"))
    ));
    for raw in [
        raw.rewrite("POCT01", "later-version"),
        raw.rewrite("</DEV>", "<unknown/></DEV>"),
        raw.rewrite("</DEV>", "<DEV.device_id V=\"duplicate\"/></DEV>"),
    ] {
        assert!(Message::parse(raw.as_bytes(), limits()).is_err());
    }
    let raw = observation("o", true, "");
    assert!(matches!(
        Message::parse(raw.as_bytes(), limits()),
        Err(MessageError::ObservationValue)
    ));
    let raw =
        observation("o", true, "<OBS.value V=\"1\" U=\"u\"/>").rewrite("V=\"OBS\"", "V=\"LQC\"");
    assert!(matches!(
        Message::parse(raw.as_bytes(), limits()),
        Err(MessageError::Role)
    ));
}

// CLSI POCT01-A2 §B.8.11
#[test]
fn time_keeps_offset_and_precision() -> Result<(), DataError> {
    let time = Timestamp::parse("2024-02-29T12:34:56.000120-03:30")?;
    assert_eq!(time.original(), "2024-02-29T12:34:56.000120-03:30");
    assert_eq!(time.offset_seconds(), Some(-12600));
    assert_eq!(Timestamp::parse("2024-02")?.offset_seconds(), None);
    for raw in [
        "2023-02-29",
        "2024-13",
        "2024-01-01T25:00",
        "2024-01-01T00:00:00+01:99",
        "2024-01-01T00:00:00.",
    ] {
        assert!(Timestamp::parse(raw).is_err());
    }
    Ok(())
}

proptest! {
    // CLSI POCT01-A2 §B.8.10
    #[test]
    fn quantity_spelling_survives(value in "[0-9]{1,6}\\.[0-9]{1,8}") {
        let raw = observation("o",true,&format!("<OBS.value V=\"{value}\" U=\"unit\"/>"));
        let parsed = Message::parse(raw.as_bytes(),limits())?;
        prop_assert_eq!(parsed.document().raw(),raw.as_bytes());
    }
}

// CLSI POCT01-A2 §B.12
#[test]
fn auxiliary_metadata_and_encapsulation_stay_local() -> Result<(), MessageError> {
    let raw = hello("h").rewrite("</DCP>","<DCP.vendor_specific ENC=\"B64\" COMPN=\"GZ\" MT=\"application/octet-stream\"><REF V=\"https://invalid.example/never-fetch\"/><THUMBNAIL ENC=\"TXT\">synthetic</THUMBNAIL></DCP.vendor_specific></DCP>");
    Message::parse(raw.as_bytes(), limits())?;
    let raw = observation("o",true,"<OBS.value V=\"1.230\" U=\"unit\" V-T=\"REAL\" U-T=\"CS\" PROB=\"0.9\"><NOTE V=\"synthetic-note\"/><CONFID V=\"synthetic-confidence\" SN=\"synthetic-system\"/></OBS.value>").rewrite("<PT.patient_id V=\"000xyz\"/>","<PT.patient_id V=\"000xyz\" xml:lang=\"en\"/><PT.name V=\"Synthetic Person\"><GIV T=\"PNXP\" V=\"Synthetic\" QUAL=\"RE\"/><FAM V=\"Person\"/></PT.name>");
    Message::parse(raw.as_bytes(), limits())?;
    for corrupted in [
        raw.rewrite("V-T=\"REAL\"", "V-T=\"ST\""),
        raw.rewrite("U-T=\"CS\"", "U-T=\"REAL\""),
        raw.rewrite("T=\"PNXP\"", "T=\"ST\""),
    ] {
        assert!(Message::parse(corrupted.as_bytes(), limits()).is_err());
    }
    Ok(())
}

// CLSI POCT01-A2 §B.12
#[test]
fn intervals_and_value_choice_use_poct_forms() -> Result<(), MessageError> {
    let raw = observation("o", true, "<OBS.value V=\"1.230\" U=\"unit\"/>").rewrite(
        "</OBS>",
        "<OBS.normal_lo-hi_limit V=\"[0.1;+inf[\" U=\"unit\"/></OBS>",
    );
    Message::parse(raw.as_bytes(), limits())?;
    for bad in [
        raw.rewrite("[0.1;+inf[", "[missing]"),
        raw.rewrite("</OBS>", "<OBS.qualitative_value V=\"positive\"/></OBS>"),
    ] {
        assert!(Message::parse(bad.as_bytes(), limits()).is_err());
    }
    Ok(())
}

proptest! {
    // CLSI POCT01-A2; project rule
    #[test]
    fn typed_parser_bounds_arbitrary_bytes(raw in prop::collection::vec(any::<u8>(),0..2048)) {
        let _ = Message::parse(&raw,limits());
    }
}

// CLSI POCT01-A2 §B.12
#[test]
fn probability_checked_without_binary_rounding() {
    let raw = observation(
        "o",
        true,
        "<OBS.value V=\"1.230\" U=\"unit\" PROB=\"1.00000000000000000000000000000000001\"/>",
    );
    assert!(Message::parse(raw.as_bytes(), limits()).is_err());
}

// CLSI POCT01-A2 §B.12
#[test]
fn probability_accepts_signed_zero_and_endpoints() -> Result<(), MessageError> {
    for probability in ["-0.00", "+0", "+1.00", "0.999999999999999999999999"] {
        let raw = observation(
            "o",
            true,
            &format!("<OBS.value V=\"1.230\" U=\"unit\" PROB=\"{probability}\"/>"),
        );
        Message::parse(raw.as_bytes(), limits())?;
    }
    Ok(())
}

// POCT01-A2 B 4.1.2 p.78; only a unique, bounded Hello header authorizes a reference.
// CLSI POCT01-A2 §B.4.1.2
#[test]
fn error_reference_is_never_guessed() {
    let raw = hello("header-only").rewrite("</DEV>", "</broken>");
    assert_eq!(
        analyte_poct::hello_error_reference(raw.as_bytes(), limits())
            .map(|id| id.as_str().to_owned()),
        Some("header-only".into())
    );
    for raw in [
        raw.rewrite("<HDR.control_id V=\"header-only\"/>", ""),
        raw.rewrite(
            "<HDR.control_id V=\"header-only\"/>",
            "<HDR.control_id V=\"one\"/><HDR.control_id V=\"two\"/>",
        ),
        raw.rewrite(
            "<HDR.control_id V=\"header-only\"/>",
            "<HDR.control_id V=\"one\" NULL=\"UNK\"/>",
        ),
        raw.rewrite("<HDR>", "<DEV><HDR>"),
        raw.rewrite("HEL.R01", "OBS.R01"),
        raw.rewrite(
            "<HDR.control_id V=\"header-only\"/>",
            "<!--<HDR.control_id V=\"forged\"/>-->",
        ),
    ] {
        assert!(analyte_poct::hello_error_reference(raw.as_bytes(), limits()).is_none());
    }
    let mut small = limits();
    small.message_bytes = 10;
    assert!(analyte_poct::hello_error_reference(raw.as_bytes(), small).is_none());
}
