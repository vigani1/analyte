//! Synthetic LAW query regression cases; no analyzer or app is contacted.
use analyte_hl7::{
    Message, Options,
    law::{Error, Query, Scope},
};
use proptest::prelude::*;

fn query(body: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|AF|M|MF|20260101||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL|||||LAB-27^IHE\r{body}\rRCP|I||R^Real Time^HL70394\r").as_bytes(), Options::default())
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
        timestamp: b"202601011200",
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
            Options::default(),
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
                Options::default()
            ),
            Err(Error::Syntax(analyte_hl7::Error::NotStored))
        ));
    }
    let mut options = Options::default();
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
        b"sample-01",
        b"SER^Serum^HL70487",
        b"P",
        b"000tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let b = Awos::new(
        b"awos-2",
        b"sample-01",
        b"SER^Serum^HL70487",
        b"P",
        b"000tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let message = work_order(
        &[a, b],
        address(),
        response_metadata()?,
        Capabilities::default(),
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
            address(),
            response_metadata()?,
            Capabilities::default(),
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
            address(),
            response_metadata()?,
            Capabilities::default(),
            utf8_options()
        )
        .is_err()
    );
    let enabled = Capabilities {
        dilution: true,
        repeat_control: true,
        priority: true,
        ..Capabilities::default()
    };
    let message = work_order(
        &[instructed],
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
        assert_eq!(spm.field(11).map(Field::raw), Some(b"U".as_slice()));
        let sac = result.segments().find(|s| s.id() == b"SAC").ok_or("SAC")?;
        assert_eq!(sac.field(3).map(Field::raw), Some(container));
        let orc = result.segments().find(|s| s.id() == b"ORC").ok_or("ORC")?;
        assert_eq!(orc.field(1).map(Field::raw), Some(b"DC".as_slice()));
        assert_eq!(
            orc.field(9).map(Field::raw),
            Some(b"202601011200".as_slice())
        );
    }
    Ok(())
}

#[test]
fn law_tf2b_3_28_4_orl_outcomes_are_per_request_not_lifecycle_retractions()
-> Result<(), Box<dyn std::error::Error>> {
    use analyte_hl7::{
        AckCode,
        law::{Awos, AwosOutcome, Capabilities, WorkAction, WorkResponse, work_order},
    };
    let work = Awos::new(
        b"awos-1",
        b"sample",
        b"SER^Serum^HL70487",
        b"P",
        b"tube",
        b"test^Test^99LAB",
        WorkAction::New,
    )?;
    let sent = work_order(
        &[work],
        address(),
        response_metadata()?,
        Capabilities::default(),
        utf8_options(),
    )?;
    for (control, status, expected) in [
        ("OK", "SC", AwosOutcome::Accepted),
        ("UA", "CA", AwosOutcome::UnableToAccept),
        ("CR", "CA", AwosOutcome::CancelAccepted),
        ("UC", "IP", AwosOutcome::UnableToCancel),
    ] {
        let incoming = format!(
            "MSH|^~\\&|A|AF|M|MF|20260101||ORL^O34^ORL_O42|orl1|P|2.5.1|||||||||LAB-28^IHE\rMSA|AA|rsp-1\rSPM||||SER\rSAC|||tube\rORC|{control}|awos-1|||{status}\r"
        );
        let message = Message::parse(incoming.as_bytes(), Options::default())?;
        let parsed = WorkResponse::new(&message, &sent)?;
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
