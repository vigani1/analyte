//! Appendix B sections 5, 6, 8 and 12 typed-message regression tests.
mod common;
use analyte_poct::{Body, DataError, Message, MessageError, Nullable, Timestamp, Value};
use common::{field, hello, limits, message, observation, status};
use proptest::prelude::*;

#[test]
fn poct01_b_6_mandatory_message_bodies_and_control_ids() -> Result<(), MessageError> {
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

#[test]
fn poct01_b_6_10_patient_and_nonpatient_values_are_explicit() -> Result<(), MessageError> {
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

#[test]
fn poct01_b_8_nulls_quantities_qualitative_codes_and_translation() -> Result<(), MessageError> {
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

#[test]
fn poct01_b_6_12_required_structure_and_order_are_enforced() {
    let raw = hello("h");
    let missing = raw.replace(&field("DEV.device_id", "A1-B2-C3-D4-E5-F6-07-08"), "");
    assert!(matches!(
        Message::parse(missing.as_bytes(), limits()),
        Err(MessageError::Missing("DEV.device_id"))
    ));
    let missing = raw.replace(&field("HDR.control_id", "h"), "");
    assert!(matches!(
        Message::parse(missing.as_bytes(), limits()),
        Err(MessageError::Missing("HDR.control_id"))
    ));
    for raw in [
        raw.replace("POCT01", "later-version"),
        raw.replace("</DEV>", "<unknown/></DEV>"),
        raw.replace("</DEV>", "<DEV.device_id V=\"duplicate\"/></DEV>"),
    ] {
        assert!(Message::parse(raw.as_bytes(), limits()).is_err());
    }
    let raw = observation("o", true, "");
    assert!(matches!(
        Message::parse(raw.as_bytes(), limits()),
        Err(MessageError::ObservationValue)
    ));
    let raw =
        observation("o", true, "<OBS.value V=\"1\" U=\"u\"/>").replace("V=\"OBS\"", "V=\"LQC\"");
    assert!(matches!(
        Message::parse(raw.as_bytes(), limits()),
        Err(MessageError::Role)
    ));
}

#[test]
fn poct01_b_8_11_time_keeps_offset_precision_and_null_distinct() -> Result<(), DataError> {
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
    #[test]
    fn poct01_b_8_10_quantity_spelling_survives(value in "[0-9]{1,6}\\.[0-9]{1,8}") {
        let raw = observation("o",true,&format!("<OBS.value V=\"{value}\" U=\"unit\"/>"));
        let parsed = Message::parse(raw.as_bytes(),limits())?;
        prop_assert_eq!(parsed.document().raw(),raw.as_bytes());
    }
}

#[test]
fn poct01_b_12_auxiliary_metadata_and_encapsulation_are_local_only() -> Result<(), MessageError> {
    let raw = hello("h").replace("</DCP>","<DCP.vendor_specific ENC=\"B64\" COMPN=\"GZ\" MT=\"application/octet-stream\"><REF V=\"https://invalid.example/never-fetch\"/><THUMBNAIL ENC=\"TXT\">synthetic</THUMBNAIL></DCP.vendor_specific></DCP>");
    Message::parse(raw.as_bytes(), limits())?;
    let raw = observation("o",true,"<OBS.value V=\"1.230\" U=\"unit\" V-T=\"REAL\" U-T=\"CS\" PROB=\"0.9\"><NOTE V=\"synthetic-note\"/><CONFID V=\"synthetic-confidence\" SN=\"synthetic-system\"/></OBS.value>").replace("<PT.patient_id V=\"000xyz\"/>","<PT.patient_id V=\"000xyz\" xml:lang=\"en\"/><PT.name V=\"Synthetic Person\"><GIV T=\"PNXP\" V=\"Synthetic\" QUAL=\"RE\"/><FAM V=\"Person\"/></PT.name>");
    Message::parse(raw.as_bytes(), limits())?;
    for corrupted in [
        raw.replace("V-T=\"REAL\"", "V-T=\"ST\""),
        raw.replace("U-T=\"CS\"", "U-T=\"REAL\""),
        raw.replace("T=\"PNXP\"", "T=\"ST\""),
    ] {
        assert!(Message::parse(corrupted.as_bytes(), limits()).is_err());
    }
    Ok(())
}

#[test]
fn poct01_b_12_intervals_and_value_choice_are_not_later_hl7_encodings() -> Result<(), MessageError>
{
    let raw = observation("o", true, "<OBS.value V=\"1.230\" U=\"unit\"/>").replace(
        "</OBS>",
        "<OBS.normal_lo-hi_limit V=\"[0.1;+inf[\" U=\"unit\"/></OBS>",
    );
    Message::parse(raw.as_bytes(), limits())?;
    for bad in [
        raw.replace("[0.1;+inf[", "[missing]"),
        raw.replace("</OBS>", "<OBS.qualitative_value V=\"positive\"/></OBS>"),
    ] {
        assert!(Message::parse(bad.as_bytes(), limits()).is_err());
    }
    Ok(())
}

proptest! {
    #[test]
    fn poct01_project_typed_parser_arbitrary_bytes_are_bounded(raw in prop::collection::vec(any::<u8>(),0..2048)) {
        let _ = Message::parse(&raw,limits());
    }
}

#[test]
fn poct01_b_12_probability_is_checked_without_binary_rounding() {
    let raw = observation(
        "o",
        true,
        "<OBS.value V=\"1.230\" U=\"unit\" PROB=\"1.00000000000000000000000000000000001\"/>",
    );
    assert!(Message::parse(raw.as_bytes(), limits()).is_err());
}
