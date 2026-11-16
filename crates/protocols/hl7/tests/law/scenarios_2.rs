//! Tests LAW message validation and host exchanges.
use super::*;

#[test]
fn law_tf2b_3_29_4_unsolicited_null_awos_and_external_reference_are_not_invented()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::{Results, Value};
    let body = observation(
        "plot^Plot^99LAB",
        "1",
        "RP",
        "https://example.invalid/synthetic^app^image^png",
        "F",
    );
    let message = results_message("\"\"", &body)?;
    let parsed = Results::new(&message)?;
    let result = parsed.observations().first().ok_or("missing observation")?;
    assert!(result.awos().is_none());
    assert!(matches!(result.values().first(), Some(Value::Reference(_))));
    Ok(())
}

#[test]
fn law_tf2b_3_29_4_ack_r22_requires_complete_interpretation_and_durability()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{AckCode, Field, Responsibility, law::result_acknowledgment};
    let message = results_message(
        "awos-1",
        &observation("test^Test^99LAB", "1", "NM", "1.20", "F"),
    )?;
    assert!(
        result_acknowledgment(
            &message,
            response_metadata()?,
            AckCode::Aa,
            Responsibility::Stored,
            None,
            utf8_options()
        )
        .is_err()
    );
    let ack = result_acknowledgment(
        &message,
        response_metadata()?,
        AckCode::Aa,
        Responsibility::StoredAndInterpreted,
        None,
        utf8_options(),
    )?
    .ok_or("missing ACK")?;
    let msh = ack.segments().next().ok_or("missing MSH")?;
    assert_eq!(
        msh.field(9).map(Field::raw),
        Some(b"ACK^R22^ACK".as_slice())
    );
    assert_eq!(
        msh.field(21).map(Field::raw),
        Some(b"LAB-29^IHE".as_slice())
    );
    assert_eq!(
        ack.segments()
            .nth(1)
            .and_then(|s| s.field(2))
            .map(Field::raw),
        Some(b"result-1".as_slice())
    );
    Ok(())
}

#[test]
fn law_tf2b_c1_unknown_error_location_is_omitted_never_invented()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{AckCode, AckError, Responsibility, law::result_acknowledgment};
    let message = results_message(
        "awos-1",
        &observation("test^Test^99LAB", "1", "NM", "1.20", "F"),
    )?;
    let unknown = AckError::unlocated(b"207", b"operator-review");
    let ack = result_acknowledgment(
        &message,
        response_metadata()?,
        AckCode::Ae,
        Responsibility::Stored,
        Some(unknown),
        utf8_options(),
    )?
    .ok_or("ACK missing")?;
    let err = ack
        .segments()
        .find(|s| s.id() == b"ERR")
        .ok_or("ERR missing")?;
    assert!(err.field(2).is_none_or(|f| f.raw().is_empty()));
    assert_eq!(
        err.field(3).map(analyte_hl7::Field::raw),
        Some(b"207^^HL70357".as_slice())
    );
    // HL7 2.5.1 section 2.15.5.2 also permits an unvalued generic ERR-2.
    let generic = analyte_hl7::acknowledgment(
        &message,
        response_metadata()?,
        AckCode::Ae,
        Responsibility::Stored,
        Some(unknown),
        utf8_options(),
    )?
    .ok_or("generic ACK missing")?;
    let err = generic
        .segments()
        .find(|s| s.id() == b"ERR")
        .ok_or("ERR missing")?;
    assert!(err.field(2).is_none_or(|f| f.raw().is_empty()));
    let inconsistent = AckError {
        occurrence: 1,
        ..unknown
    };
    assert!(
        result_acknowledgment(
            &message,
            response_metadata()?,
            AckCode::Ae,
            Responsibility::Stored,
            Some(inconsistent),
            utf8_options()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn law_tf2b_2_2_usage_cardinality_length_and_null_rules() -> Result<(), Box<dyn std::error::Error>>
{
    use analyte_hl7::law::{FieldRule, Usage};
    let message = results_message(
        "awos-1",
        &observation("test^Test^99LAB", "1", "NM", "1.20", "F"),
    )?;
    let obx = message.segments().find(|s| s.id() == b"OBX").ok_or("OBX")?;
    for usage in [
        Usage::Mandatory,
        Usage::Required,
        Usage::RequiredIfAvailable,
        Usage::RequiredFromAnalyzer,
    ] {
        FieldRule {
            segment: "OBX",
            field: 5,
            usage,
            minimum: 1,
            maximum: 1,
            length: 4,
        }
        .check(obx)?;
    }
    for (usage, maximum, length) in [
        (Usage::Excluded, 1, 4),
        (Usage::Conditional, 1, 4),
        (Usage::Mandatory, 0, 4),
        (Usage::Mandatory, 1, 3),
    ] {
        assert!(
            FieldRule {
                segment: "OBX",
                field: 5,
                usage,
                minimum: 1,
                maximum,
                length
            }
            .check(obx)
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_appendix_c_result_field_predicates_and_required_fields()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::Results;
    let good = results_message(
        "awos-1",
        &observation("test^Test^99LAB", "1", "NM", "1.20", "F"),
    )?;
    for (needle, replacement) in [
        ("|UNICODE UTF-8|", "||"),
        ("|LAB-29^IHE", "|LAB-28^IHE"),
        ("SPM|1|", "SPM||"),
        ("|1.20|mg/L^mg/L^UCUM|", "|1.20||"),
        ("mg/L^mg/L^UCUM", "mg/L^mg/L^99LAB"),
        ("|F|", "|D|"),
        ("|RSLT\r", "|OTHER\r"),
        ("ORC|SC||||CM\r", ""),
    ] {
        let original = std::str::from_utf8(good.raw())?;
        assert!(original.contains(needle));
        let options = if needle == "|UNICODE UTF-8|" {
            Options::default()
        } else {
            utf8_options()
        };
        let altered = Message::parse(original.replace(needle, replacement).as_bytes(), options)?;
        assert!(
            Results::new(&altered).is_err(),
            "accepted alteration {needle}"
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_c4_extended_control_id_is_scoped_to_law() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        Response, Responsibility,
        law::{CacheAnswer, control_id, query_response},
    };
    let long = b"01234567890123456789012345678901234567890123456789";
    assert!(analyte_hl7::ControlId::new(long).is_err());
    let source = query("QPD|WOS^Work Order Step^IHELAW|tag|000Ab")?;
    let response = query_response(
        Query::new(&source)?,
        CacheAnswer::WorkExists,
        Response {
            control_id: control_id(long)?,
            timestamp: b"20260101120000+0000",
        },
        Responsibility::StoredAndInterpreted,
        utf8_options(),
    )?;
    assert_eq!(
        response
            .message()
            .segments()
            .next()
            .and_then(|s| s.field(10))
            .map(analyte_hl7::Field::raw),
        Some(long.as_slice())
    );
    Ok(())
}

#[test]
fn law_tf2b_b7_trigger_rejection_and_query_processing_errors_have_distinct_actions()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        AckCode,
        law::{ErrorLocation, Failure, receive_query, receive_results},
    };
    let request = query("QPD|WOS^Work Order Step^IHELAW|tag")?;
    let failure = receive_query(&request)
        .err()
        .ok_or("accepted missing container")?;
    assert!(matches!(failure, Failure::MalformedTrigger(_)));
    assert_eq!(failure.acknowledgment_code(), Some(AckCode::Ae));
    assert_eq!(
        failure.location(),
        Some(ErrorLocation {
            segment: "QPD",
            occurrence: None,
            field: 3
        })
    );
    let original = std::str::from_utf8(request.raw())?;
    let wrong = Message::parse(
        original.replace("LAB-27^IHE", "LAB-28^IHE").as_bytes(),
        utf8_options(),
    )?;
    let failure = receive_query(&wrong)
        .err()
        .ok_or("accepted wrong profile")?;
    assert!(matches!(failure, Failure::ContentReject(_)));
    assert_eq!(failure.acknowledgment_code(), Some(AckCode::Ar));
    let mut body = observation("test^Test^99LAB", "1", "NM", "1.20", "F");
    body.push_str("TCD|other^Other^99LAB\r");
    let message = results_message("awos-1", &body)?;
    let failure = receive_results(&message)
        .err()
        .ok_or("accepted inconsistent result")?;
    assert!(matches!(failure, Failure::ContentReject(_)));
    assert_eq!(failure.acknowledgment_code(), Some(AckCode::Ar));
    assert_eq!(
        failure.location(),
        Some(ErrorLocation {
            segment: "TCD",
            occurrence: Some(1),
            field: 1
        })
    );
    let failure = Failure::QueryRejected {
        location: ErrorLocation {
            segment: "QPD",
            occurrence: None,
            field: 3,
        },
    };
    assert_eq!(failure.acknowledgment_code(), None);
    Ok(())
}

#[test]
fn law_tf2b_b7_invalid_ack_and_inconsistent_orl_fail_without_ack_loop()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::{Failure, receive_acknowledgment, receive_work_response};
    let sent = results_message(
        "awos-1",
        &observation("test^Test^99LAB", "1", "NM", "1.20", "F"),
    )?;
    let response = Message::parse(b"MSH|^~\\&|M|MF|A|AF|20260101120000+0000||ACK^R22^ACK|ack1|P|2.5.1||||||UNICODE UTF-8|||LAB-29^IHE\rMSA|AA|wrong-id\r", utf8_options())?;
    let failure = receive_acknowledgment(&response, &sent)
        .err()
        .ok_or("accepted wrong correlation")?;
    assert!(matches!(failure, Failure::InvalidAcknowledgment(_)));
    assert_eq!(failure.acknowledgment_code(), None);
    assert_eq!(
        failure.location().map(|l| (l.segment, l.field)),
        Some(("MSA", 2))
    );
    let failure = receive_work_response(&response, &sent)
        .err()
        .ok_or("accepted wrong response type")?;
    assert!(matches!(failure, Failure::InconsistentOrl(_)));
    assert_eq!(failure.acknowledgment_code(), None);
    Ok(())
}

#[test]
fn law_tf2b_2_2_4_app_supplied_patient_specimen_container_and_order_content()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        Field,
        law::{
            Awos, ContainerDetails, OrderDetails, PatientInfo, SpecimenDetails, Visit, WorkAction,
            work_order,
        },
    };
    let specimen = SpecimenDetails {
        parents: Some(b"parent&99LAB"),
        risk: Some(b"RISK^Synthetic risk^99LAB"),
        collection_time: Some(b"20260101100000"),
        receipt_time: Some(b"20260101110000"),
        ..SpecimenDetails::default()
    };
    let container = ContainerDetails {
        carrier: Some(b"rack-01"),
        carrier_position: Some(b"1"),
        container_volume: Some(b"2.5"),
        specimen_volume: Some(b"1.5"),
        volume_units: Some(b"mL^mL^UCUM"),
        dilution: Some(b"^1^:^2"),
        ..ContainerDetails::default()
    };
    let order = OrderDetails {
        group: Some(b"group-1"),
        facility: Some(b"Synthetic lab"),
        provider: Some(b"provider-1"),
        result_handling: None,
    };
    let awos = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?
    .with_specimen_details(&specimen)
    .with_container_details(&container)
    .with_order_details(&order);
    let patient = PatientInfo {
        notes: &[b"Synthetic patient comment"],
        identifiers: b"synthetic-001",
        name: b"Synthetic^Patient^^^^^L",
        birth: Some(b"20000101"),
        sex: Some(b"U"),
        race: None,
        taxonomy: None,
        visit: Some(Visit {
            class: b"I",
            location: Some(b"^Room"),
        }),
    };
    let mut capabilities = work_capabilities();
    assert!(
        work_order(
            &[awos],
            Some(patient),
            address(),
            response_metadata()?,
            capabilities,
            utf8_options()
        )
        .is_err()
    );
    capabilities.content.patient = true;
    capabilities.content.container = true;
    let message = work_order(
        &[awos],
        Some(patient),
        address(),
        response_metadata()?,
        capabilities,
        utf8_options(),
    )?;
    for (id, field, expected) in [
        (b"PID".as_slice(), 3, b"synthetic-001".as_slice()),
        (b"PV1", 2, b"I"),
        (b"SPM", 3, b"parent&99LAB"),
        (b"SPM", 16, b"RISK^Synthetic risk^99LAB"),
        (b"SAC", 21, b"2.5"),
        (b"SAC", 24, b"mL^mL^UCUM"),
        (b"SAC", 29, b"^1^:^2"),
        (b"ORC", 4, b"group-1"),
        (b"OBR", 16, b"provider-1"),
    ] {
        let segment = message
            .segments()
            .find(|s| s.id() == id)
            .ok_or("missing segment")?;
        assert_eq!(segment.field(field).map(Field::raw), Some(expected));
    }
    Ok(())
}

#[test]
fn law_tf2b_3_29_4_specimen_observations_keep_scope_and_reject_orphan_groups()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::{Results, Value};
    let message = results_message(
        "awos-1",
        &observation("test^Test^99LAB", "1", "NM", "1.20", "F"),
    )?;
    let source = std::str::from_utf8(message.raw())?;
    let specimen_observation = observation(
        "condition^Condition^99LAB",
        "1",
        "ST",
        "synthetic condition",
        "F",
    );
    let with_specimen_observation = source.replace(
        "SAC|||000tube",
        &format!("{specimen_observation}SAC|||000tube"),
    );
    let message = Message::parse(with_specimen_observation.as_bytes(), utf8_options())?;
    let results = Results::new(&message)?;
    assert_eq!(results.observations().len(), 1);
    assert_eq!(results.specimen_observations().len(), 1);
    assert!(matches!(
        results
            .specimen_observations()
            .first()
            .and_then(|o| o.values().first()),
        Some(Value::Text(b"synthetic condition"))
    ));
    let orphan = format!("{source}SPM|2|||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||orphan\r");
    let message = Message::parse(orphan.as_bytes(), utf8_options())?;
    assert!(Results::new(&message).is_err());
    Ok(())
}

#[test]
fn law_tf2b_c7_repetition_predicate_and_structured_numeric_validation()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::Results;
    for (code, kind, value, valid) in [
        ("test^Test^99LAB", "NM", "1~2", false),
        ("raw^Raw^99LAB^S_RAW^Raw^IHELAW", "NM", "1~2", true),
        ("test^Test^99LAB", "NM", "1,234", false),
        ("test^Test^99LAB", "ST", "1,234", true),
        ("test^Test^99LAB", "SN", "<^0.1", true),
        ("test^Test^99LAB", "SN", "nonsense", false),
        ("test^Test^99LAB", "NA", "1^2^3", true),
        ("test^Test^99LAB", "NA", "1^word", false),
    ] {
        let message = results_message("awos-1", &observation(code, "1", kind, value, "F"))?;
        assert_eq!(Results::new(&message).is_ok(), valid, "{kind}: {value}");
    }
    Ok(())
}

#[test]
fn law_tf2b_c12_control_specimen_uses_explicit_null_type() -> Result<(), Box<dyn std::error::Error>>
{
    use analyte_hl7::{
        Field,
        law::{Awos, WorkAction, work_order},
    };
    assert!(
        Awos::new(
            b"qc-awos",
            b"control&99LAB",
            b"SER^Serum^HL70487",
            b"Q",
            b"qc-tube",
            b"test^Test^99LAB",
            WorkAction::New
        )
        .is_err()
    );
    let awos = Awos::new(
        b"qc-awos",
        b"control&99LAB",
        b"\"\"",
        b"Q",
        b"qc-tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let message = work_order(
        &[awos],
        None,
        address(),
        response_metadata()?,
        work_capabilities(),
        utf8_options(),
    )?;
    let spm = message.segments().find(|s| s.id() == b"SPM").ok_or("SPM")?;
    assert_eq!(spm.field(4).map(Field::raw), Some(b"\"\"".as_slice()));
    assert_eq!(
        spm.field(11).map(Field::raw),
        Some(b"Q^^HL70369".as_slice())
    );
    Ok(())
}

#[test]
fn law_tf2b_c1_error_codes_severity_and_available_fields() -> Result<(), Box<dyn std::error::Error>>
{
    for code in [
        "100", "101", "102", "103", "200", "201", "202", "203", "207",
    ] {
        validate_profile_body(&format!(
            "ERR||OBX^1^5|{code}^Synthetic description^HL70357|E"
        ))?;
    }
    validate_profile_body("ERR|||100^^HL70357|E|LOCAL^Synthetic^99LAB|||Synthetic detail")?;
    for body in [
        "ERR|||0^^HL70357|E",
        "ERR|||100^^OTHER|E",
        "ERR|||100^^HL70357|W",
        "ERR|||100^^HL70357|I",
        "ERR|||100^^HL70357|E|LOCAL",
        "ERR||OBX^x|100^^HL70357|E",
        "ERR||OBX^0|100^^HL70357|E",
        "ERR||OBX^1^x|100^^HL70357|E",
        "ERR||OBX|100^^HL70357|E",
    ] {
        assert!(validate_profile_body(body).is_err(), "{body}");
    }
    validate_profile_body(&format!("ERR|||100^^HL70357|E||||{}", "x".repeat(251)))?;
    Ok(())
}

#[test]
fn law_tf2b_c4_namespaces_and_profile_identifier_components()
-> Result<(), Box<dyn std::error::Error>> {
    let message = query("QPD|WOS_ALL^Work Order Step All^IHELAW|tag")?;
    let original = std::str::from_utf8(message.raw())?;
    for replacement in ["", "A", "12345678901234567890"] {
        let bytes = original.replacen("|A|AF|", &format!("|{replacement}|AF|"), 1);
        let parsed = Message::parse(bytes.as_bytes(), utf8_options())?;
        Query::new(&parsed)?;
    }
    for (from, to) in [
        ("|A|AF|", "|123456789012345678901|AF|"),
        ("|A|AF|", "|A~B|AF|"),
        ("LAB-27^IHE", "LAB-27^IHE~vendor"),
        ("LAB-27^IHE", "LAB-27^IHE~vendor^123456789012345678901"),
    ] {
        let bytes = original.replace(from, to);
        let parsed = Message::parse(bytes.as_bytes(), utf8_options())?;
        assert!(Query::new(&parsed).is_err());
        assert_eq!(parsed.raw(), bytes.as_bytes());
    }
    let bytes = original.replace("LAB-27^IHE", "LAB-27^IHE~vendor^LAB");
    let parsed = Message::parse(bytes.as_bytes(), utf8_options())?;
    Query::new(&parsed)?;
    Ok(())
}
