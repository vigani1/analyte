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
