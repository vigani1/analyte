//! Synthetic LAW query regression cases; no analyzer or app is contacted.
use analyte_hl7::{
    Message, Options,
    law::{Error, Query, Scope},
};
use proptest::prelude::*;

fn query(body: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|20260101120000+0000||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\r{body}\rRCP|I||R^Real Time^HL70394\r").as_bytes(), utf8_options())
}

#[test]
fn law_tf2b_3_27_4_container_preserves_leading_zero_and_raw()
-> Result<(), Box<dyn std::error::Error>> {
    let message = query("QPD|WOS^Work Order Step^IHELAW|tag|000Ab")?;
    let view = Query::new(&message)?;
    let Scope::Container(id) = view.scope() else {
        return Err("wrong scope".into());
    };
    assert_eq!(id.raw(), b"000Ab");
    assert_eq!(view.tag(), b"tag");
    assert_eq!(view.message().raw(), message.raw());
    Ok(())
}

#[test]
fn law_tf2b_3_27_4_query_forms_and_conditional_fields() -> Result<(), Box<dyn std::error::Error>> {
    for (body, expected) in [
        ("QPD|WOS_ALL^Work Order Step All^IHELAW|tag", "all"),
        (
            "QPD|WOS_BY_RACK^Work Order Step by rack^IHELAW|tag||rack|01",
            "rack",
        ),
        (
            "QPD|WOS_BY_TRAY^Work Order Step by tray^IHELAW|tag||||tray|1^2",
            "tray",
        ),
        (
            "QPD|WOS_BY_ISOLATE^Work Order Step by isolate^IHELAW|tag|child||||||parent",
            "isolate",
        ),
        (
            "QPD|WOS_INFORMATIONAL_RACK^Query^IHELAW|tag|tube|rack|2",
            "informational",
        ),
    ] {
        let message = query(body)?;
        let name = match Query::new(&message)?.scope() {
            Scope::AllPending => "all",
            Scope::Rack { .. } => "rack",
            Scope::Tray { .. } => "tray",
            Scope::Isolate { .. } => "isolate",
            Scope::InformationalRack { .. } => "informational",
            Scope::Container(_) => "container",
        };
        assert_eq!(name, expected);
    }
    Ok(())
}

#[test]
fn law_tf2b_3_27_4_forbidden_missing_null_and_repeated_parameters_retain_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    for body in [
        "QPD|WOS_ALL^Work Order Step All^IHELAW|tag|forbidden",
        "QPD|WOS^Work Order Step^IHELAW|tag",
        "QPD|WOS^Work Order Step^IHELAW|tag|\"\"",
        "QPD|WOS^Work Order Step^IHELAW|tag|a~b",
        "QPD|WOS_BY_RACK^Rack^IHELAW|tag||rack|x",
    ] {
        let message = query(body)?;
        let original = message.serialize();
        assert!(matches!(Query::new(&message), Err(Error::Content { .. })));
        assert_eq!(message.raw(), original);
    }
    Ok(())
}

proptest! {
    #[test]
    fn law_tf2b_3_27_4_bounded_container_strings(id in "[0-9A-Za-z]{1,50}") {
        let message = query(&format!("QPD|WOS^Work Order Step^IHELAW|tag|{id}"))?;
        let parsed = Query::new(&message)?;
        prop_assert!(matches!(parsed.scope(), Scope::Container(_)));
        if let Scope::Container(identity) = parsed.scope() {
            prop_assert_eq!(identity.raw(), id.as_bytes());
        }
    }
}

fn response_metadata() -> Result<analyte_hl7::Response<'static>, analyte_hl7::Error> {
    Ok(analyte_hl7::Response {
        control_id: analyte_hl7::ControlId::new(b"rsp-1")?,
        timestamp: b"20260101120000+0000",
    })
}

#[test]
fn law_tf2b_3_27_4_rsp_correlates_and_separates_empty_work_from_failed_lookup()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        AckError, Field, Responsibility,
        law::{CacheAnswer, QueryFollowup, query_response},
    };
    let source = query("QPD|WOS^Work Order Step^IHELAW|tag|000Ab")?;
    let request = Query::new(&source)?;
    for (answer, expected, status) in [
        (
            CacheAnswer::WorkExists,
            QueryFollowup::Work,
            b"OK".as_slice(),
        ),
        (
            CacheAnswer::NoWork,
            QueryFollowup::Negative,
            b"OK".as_slice(),
        ),
        (
            CacheAnswer::Unavailable(AckError {
                segment: b"QPD",
                occurrence: 1,
                field: 3,
                code: b"207",
                application_code: b"CACHE_UNAVAILABLE",
            }),
            QueryFollowup::None,
            b"AE".as_slice(),
        ),
    ] {
        let result = query_response(
            request,
            answer,
            response_metadata()?,
            Responsibility::StoredAndInterpreted,
            utf8_options(),
        )?;
        assert_eq!(result.followup(), expected);
        let segments: Vec<_> = result.message().segments().collect();
        let msh = segments.first().ok_or("missing MSH")?;
        assert_eq!(
            msh.field(9).map(Field::raw),
            Some(b"RSP^K11^RSP_K11".as_slice())
        );
        assert_eq!(msh.field(3).map(Field::raw), Some(b"M".as_slice()));
        assert_eq!(msh.field(5).map(Field::raw), Some(b"A".as_slice()));
        assert_eq!(msh.field(10).map(Field::raw), Some(b"rsp-1".as_slice()));
        assert_eq!(msh.field(15).map(Field::raw), Some(b"".as_slice()));
        assert_eq!(msh.field(16).map(Field::raw), Some(b"".as_slice()));
        assert_eq!(
            msh.field(21).map(Field::raw),
            Some(b"LAB-27^IHE".as_slice())
        );
        let msa = segments
            .iter()
            .find(|s| s.id() == b"MSA")
            .ok_or("missing MSA")?;
        assert_eq!(msa.field(2).map(Field::raw), Some(b"q1".as_slice()));
        let qak = segments
            .iter()
            .find(|s| s.id() == b"QAK")
            .ok_or("missing QAK")?;
        assert_eq!(qak.field(1).map(Field::raw), Some(b"tag".as_slice()));
        assert_eq!(qak.field(2).map(Field::raw), Some(status));
        assert_eq!(
            qak.field(3).map(Field::raw),
            Some(b"WOS^Work Order Step^IHELAW".as_slice())
        );
        assert_eq!(
            segments.last().map(|s| s.raw()),
            source.segments().nth(1).map(analyte_hl7::Segment::raw)
        );
        let error = segments.iter().find(|s| s.id() == b"ERR");
        if expected == QueryFollowup::None {
            assert_eq!(
                error.and_then(|s| s.field(2)).map(Field::raw),
                Some(b"QPD^1^3".as_slice())
            );
        } else {
            assert!(error.is_none());
        }
    }
    Ok(())
}

#[test]
fn law_tf2b_b5_rsp_requires_storage_and_bounds_output() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        Responsibility,
        law::{CacheAnswer, query_response},
    };
    let source = query("QPD|WOS^Work Order Step^IHELAW|tag|000Ab")?;
    let request = Query::new(&source)?;
    for responsibility in [Responsibility::NotStored, Responsibility::Stored] {
        assert!(matches!(
            query_response(
                request,
                CacheAnswer::WorkExists,
                response_metadata()?,
                responsibility,
                utf8_options()
            ),
            Err(Error::Syntax(analyte_hl7::Error::NotStored))
        ));
    }
    let mut options = utf8_options();
    options.limits.message_bytes = 64;
    assert!(matches!(
        query_response(
            request,
            CacheAnswer::NoWork,
            response_metadata()?,
            Responsibility::StoredAndInterpreted,
            options
        ),
        Err(Error::Syntax(analyte_hl7::Error::Limit { .. }))
    ));
    Ok(())
}

fn address() -> analyte_hl7::law::Address<'static> {
    analyte_hl7::law::Address {
        application: b"M",
        facility: b"MF",
        peer_application: b"A",
        peer_facility: b"AF",
    }
}

fn utf8_options() -> Options {
    Options {
        charset: analyte_hl7::Charset::Utf8,
        ..Options::default()
    }
}

#[test]
fn law_tf2b_3_28_4_work_unique_awos_replicates_and_authorized_options()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        Field,
        law::{Awos, Capabilities, Instructions, WorkAction, work_order},
    };
    let a = Awos::new(
        b"awos-1",
        b"sample-01&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"000tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let b = Awos::new(
        b"awos-2",
        b"sample-01&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"000tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let message = work_order(
        &[a, b],
        None,
        address(),
        response_metadata()?,
        work_capabilities(),
        utf8_options(),
    )?;
    let ids: Vec<_> = message
        .segments()
        .filter(|s| s.id() == b"OBR")
        .filter_map(|s| s.field(2))
        .map(Field::raw)
        .collect();
    assert_eq!(ids, [b"awos-1".as_slice(), b"awos-2"]);
    assert!(!message.segments().any(|s| s.id() == b"TCD"));
    assert!(
        work_order(
            &[a, a],
            None,
            address(),
            response_metadata()?,
            work_capabilities(),
            utf8_options()
        )
        .is_err()
    );
    let instructed = a.with_instructions(Instructions {
        dilution: Some(b"^1^:^5"),
        automatic_repeat: Some(false),
        reflex: Some(true),
        priority: Some(b"S^Stat^HL70485"),
        ..Instructions::default()
    });
    assert!(
        work_order(
            &[instructed],
            None,
            address(),
            response_metadata()?,
            work_capabilities(),
            utf8_options()
        )
        .is_err()
    );
    let enabled = Capabilities {
        dilution: true,
        repeat_control: true,
        priority: true,
        ..work_capabilities()
    };
    let message = work_order(
        &[instructed],
        None,
        address(),
        response_metadata()?,
        enabled,
        utf8_options(),
    )?;
    let tcd = message
        .segments()
        .find(|s| s.id() == b"TCD")
        .ok_or("missing TCD")?;
    assert_eq!(tcd.field(2).map(Field::raw), Some(b"^1^:^5".as_slice()));
    assert_eq!(tcd.field(6).map(Field::raw), Some(b"N".as_slice()));
    assert_eq!(tcd.field(7).map(Field::raw), Some(b"Y".as_slice()));
    Ok(())
}

#[test]
fn law_tf2b_3_28_4_negative_lab28_for_genuine_no_work() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{Field, law::negative_work};
    for (body, container) in [
        (
            "QPD|WOS^Work Order Step^IHELAW|tag|000tube",
            b"000tube".as_slice(),
        ),
        ("QPD|WOS_ALL^Work Order Step All^IHELAW|tag", b"\"\""),
    ] {
        let request = query(body)?;
        let result = negative_work(
            Query::new(&request)?,
            address(),
            response_metadata()?,
            utf8_options(),
        )?;
        assert_eq!(
            result
                .segments()
                .map(analyte_hl7::Segment::id)
                .collect::<Vec<_>>(),
            [b"MSH", b"SPM", b"SAC", b"ORC"]
        );
        let spm = result.segments().find(|s| s.id() == b"SPM").ok_or("SPM")?;
        assert_eq!(spm.field(4).map(Field::raw), Some(b"\"\"".as_slice()));
        assert_eq!(spm.field(11).map(Field::raw), Some(b"U^^IHELAW".as_slice()));
        let sac = result.segments().find(|s| s.id() == b"SAC").ok_or("SAC")?;
        assert_eq!(sac.field(3).map(Field::raw), Some(container));
        let orc = result.segments().find(|s| s.id() == b"ORC").ok_or("ORC")?;
        assert_eq!(orc.field(1).map(Field::raw), Some(b"DC".as_slice()));
        assert_eq!(
            orc.field(9).map(Field::raw),
            Some(b"20260101120000".as_slice())
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        AckCode,
        law::{Awos, AwosOutcome, WorkAction, WorkResponse, work_order},
    };
    let work = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let sent = work_order(
        &[work],
        None,
        address(),
        response_metadata()?,
        work_capabilities(),
        utf8_options(),
    )?;
    let cancelled = Awos::new(
        b"awos-1",
        b"sample&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"tube",
        b"test^Test^99LAB",
        WorkAction::Cancel,
    )?;
    let cancellation = work_order(
        &[cancelled],
        None,
        address(),
        response_metadata()?,
        work_capabilities(),
        utf8_options(),
    )?;
    for (control, status, expected) in [
        ("OK", "SC", AwosOutcome::Accepted),
        ("UA", "CA", AwosOutcome::UnableToAccept),
        ("CR", "CA", AwosOutcome::CancelAccepted),
        ("UC", "IP", AwosOutcome::UnableToCancel),
    ] {
        let incoming = format!(
            "MSH|^~\\&|A|AF|M|MF|20260101120000+0000||ORL^O34^ORL_O42|orl1|P|2.5.1||||||UNICODE UTF-8|||LAB-28^IHE\rMSA|AA|rsp-1\rSPM|1|||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||tube\rORC|{control}|awos-1|||{status}\r"
        );
        let message = Message::parse(incoming.as_bytes(), utf8_options())?;
        let request = if matches!(control, "CR" | "UC") {
            &cancellation
        } else {
            &sent
        };
        let parsed = WorkResponse::new(&message, request)?;
        let wrong_container = Message::parse(
            incoming
                .replace("SAC|||tube", "SAC|||different-tube")
                .as_bytes(),
            utf8_options(),
        )?;
        assert!(matches!(
            WorkResponse::new(&wrong_container, request),
            Err(Error::Inconsistent {
                segment: "SAC",
                field: 3
            })
        ));
        if matches!(control, "CR" | "UC") {
            assert!(WorkResponse::new(&message, &sent).is_err());
        }
        assert_eq!(parsed.acceptance(), AckCode::Aa);
        assert_eq!(
            parsed.decisions().first().map(|d| d.outcome),
            Some(expected)
        );
        assert_eq!(
            parsed.decisions().first().map(|d| d.id.raw()),
            Some(b"awos-1".as_slice())
        );
        assert_eq!(parsed.message().raw(), incoming.as_bytes());
    }
    Ok(())
}

fn results_message(awos: &str, extra: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|result-1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||000tube\rOBR||{awos}||test^Test^99LAB\rORC|SC||||CM\r{extra}").as_bytes(), utf8_options())
}

fn observation(code: &str, sub_id: &str, kind: &str, value: &str, status: &str) -> String {
    let mut fields = vec![""; 29];
    for (index, value) in [
        (0, "1"),
        (1, kind),
        (2, code),
        (3, sub_id),
        (4, value),
        (
            5,
            if matches!(kind, "NM" | "SN") {
                "mg/L^mg/L^UCUM"
            } else {
                ""
            },
        ),
        (6, "0-2"),
        (7, "N^Normal^HL70078"),
        (10, status),
        (15, "operator^Synthetic"),
        (17, "instrument^99LAB~module^99LAB"),
        (18, "20260101120000"),
        (28, "RSLT"),
    ] {
        if let Some(field) = fields.get_mut(index) {
            *field = value;
        }
    }
    format!("OBX|{}\r", fields.join("|"))
}

#[test]
fn law_tf2b_3_29_4_results_preserve_identity_values_status_equipment_and_scoped_notes()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        Field,
        law::{Parent, ResultStatus, Results, Value},
    };
    let mut body = observation("test^Test^99LAB", "1^1^1", "SN", "<^0.1", "C");
    body.push_str("TCD|test^Test^99LAB|^1^:^5\rNTE|1|Z|Synthetic observation note\r");
    let message = results_message("awos-1", &body)?;
    let results = Results::new(&message)?;
    let result = results
        .observations()
        .first()
        .ok_or("missing observation")?;
    assert_eq!(
        result.awos().map(analyte_hl7::law::Identity::raw),
        Some(b"awos-1".as_slice())
    );
    assert_eq!(result.code(), b"test^Test^99LAB");
    assert_eq!(result.sub_id(), b"1^1^1");
    assert_eq!(result.status(), ResultStatus::Corrected);
    assert!(matches!(
        result.values().first(),
        Some(Value::StructuredNumber(b"<^0.1"))
    ));
    assert_eq!(
        result.container().field(3).map(Field::raw),
        Some(b"000tube".as_slice())
    );
    assert_eq!(
        result.specimen().field(2).map(Field::raw),
        Some(b"sample-01&99LAB".as_slice())
    );
    assert_eq!(
        result.units().map(Field::raw),
        Some(b"mg/L^mg/L^UCUM".as_slice())
    );
    assert_eq!(result.range().map(Field::raw), Some(b"0-2".as_slice()));
    assert_eq!(
        result.flags().map(Field::raw),
        Some(b"N^Normal^HL70078".as_slice())
    );
    assert_eq!(result.equipment().map(|f| f.repetitions().count()), Some(2));
    assert!(
        results
            .attached()
            .iter()
            .any(|a| matches!(a.parent, Parent::Observation(_)) && a.segment.id() == b"NTE")
    );
    assert_eq!(results.message().raw(), message.raw());
    Ok(())
}

#[test]
fn law_tf2b_b7_6_inconsistent_tcd_rejects_whole_message_and_retains_original()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::law::Results;
    let mut body = observation("test^Test^99LAB", "1", "NM", "1.20", "F");
    body.push_str(
        &observation("other^Other^99LAB", "2", "NM", "2.30", "F").replacen("OBX|1|", "OBX|2|", 1),
    );
    body.push_str("TCD|mismatch^Mismatch^99LAB\r");
    let message = results_message("awos-1", &body)?;
    let original = message.serialize();
    assert!(matches!(
        Results::new(&message),
        Err(Error::Inconsistent {
            segment: "TCD",
            field: 1
        })
    ));
    assert_eq!(message.raw(), original);
    Ok(())
}

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
    assert!(
        analyte_hl7::acknowledgment(
            &message,
            response_metadata()?,
            AckCode::Ae,
            Responsibility::Stored,
            Some(unknown),
            utf8_options()
        )
        .is_err()
    );
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
            field: 1
        })
    );
    let failure = Failure::QueryRejected {
        location: ErrorLocation {
            segment: "QPD",
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

fn work_capabilities() -> analyte_hl7::law::Capabilities {
    analyte_hl7::law::Capabilities {
        specimen: true,
        ..analyte_hl7::law::Capabilities::default()
    }
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
    capabilities.patient = true;
    capabilities.container = true;
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

fn validate_profile_body(body: &str) -> Result<(), Box<dyn std::error::Error>> {
    let message = query(body)?;
    let segment = message
        .segments()
        .nth(1)
        .ok_or("missing synthetic segment")?;
    analyte_hl7::law::validate_segment(segment)?;
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
    let message = query("QPD|WOS_ALL^Synthetic^IHELAW|tag")?;
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

#[test]
fn law_tf2b_c1_component_failures_report_precise_locations()
-> Result<(), Box<dyn std::error::Error>> {
    for (body, field, component) in [
        ("ERR||OBX^x|100^^HL70357|E", 2, 2),
        ("ERR|||999^^HL70357|E", 3, 1),
        ("ERR|||100^^OTHER|E", 3, 3),
        ("ERR|||100^^HL70357|E|LOCAL^^OTHER", 5, 3),
    ] {
        let message = query(body)?;
        let segment = message
            .segments()
            .nth(1)
            .ok_or("missing synthetic segment")?;
        assert_eq!(
            analyte_hl7::law::validate_segment(segment),
            Err(Error::Component {
                segment: "ERR",
                field,
                component,
            })
        );
    }
    Ok(())
}
