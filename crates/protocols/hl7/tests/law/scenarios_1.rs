//! Tests LAW message validation and host exchanges.
use super::*;

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
        let original = message.encode();
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
        (
            CacheAnswer::Rejected(AckError {
                segment: b"QPD",
                occurrence: 1,
                field: 3,
                code: b"207",
                application_code: b"unsupported-query",
            }),
            QueryFollowup::None,
            b"AR".as_slice(),
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
        instructions: analyte_hl7::law::PeerInstructions {
            dilution: true,
            priority: true,
            repeat: analyte_hl7::law::PeerRepeat {
                repeat_control: true,
                ..work_capabilities().instructions.repeat
            },
        },
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
        ("QPD|WOS^Work Order Step^IHELAW|tag|000tube^LAB", b"000tube"),
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
fn law_tf2b_c4_work_and_negative_builders_keep_fifty_character_control_ids()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        Field, Response,
        law::{Awos, Capabilities, WorkAction},
    };
    let id = [b'a'; 50];
    let response = Response {
        control_id: analyte_hl7::law::control_id(&id)?,
        timestamp: b"20260101120000+0000",
    };
    let awos = Awos::new(
        b"awos-1",
        b"sample&LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let positive = analyte_hl7::law::work_order(
        &[awos],
        None,
        address(),
        response,
        Capabilities::default(),
        utf8_options(),
    )?;
    let request = query("QPD|WOS^Work Order Step^IHELAW|tag|tube^LAB")?;
    let negative = analyte_hl7::law::negative_work(
        Query::new(&request)?,
        address(),
        response,
        utf8_options(),
    )?;
    for message in [positive, negative] {
        assert_eq!(
            message
                .segments()
                .next()
                .and_then(|s| s.field(10))
                .map(Field::raw),
            Some(id.as_slice())
        );
    }
    assert!(analyte_hl7::law::control_id(&[b'a'; 51]).is_err());
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
    let original = message.encode();
    let failure = Results::new(&message)
        .err()
        .ok_or("accepted mismatched TCD")?;
    assert!(
        matches!(failure, Error::AtSegment {occurrence:1,source} if matches!(*source,Error::Inconsistent {segment:"TCD",field:1}))
    );
    assert_eq!(message.raw(), original);
    Ok(())
}
