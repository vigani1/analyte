//! Appendix B sections 5 and 6 reviewer output tests.
mod common;
use analyte_poct::{
    AckKind, ApplicationError, Body, ControlId, EscapeReason, Header, Message, MessageError,
    Outbound, TerminationReason, Timestamp, build,
};

#[test]
fn poct01_b_6_reviewer_builders_round_trip_and_correlate() -> Result<(), MessageError> {
    let header = Header {
        control_id: ControlId::new("new<&\"'")?,
        created: Timestamp::parse("2026-06-09T08:00:00+02:00")?,
    };
    let reference = ControlId::new("old<&\"'")?;
    for body in [
        Outbound::RequestObservations,
        Outbound::Terminate(TerminationReason::Normal),
        Outbound::Accept(reference.clone()),
        Outbound::ApplicationError {
            reference: reference.clone(),
            error: ApplicationError::Internal,
        },
        Outbound::Escape {
            reference: reference.clone(),
            reason: EscapeReason::CannotComplete,
        },
    ] {
        let built = build(&header, &body, common::limits())?;
        let parsed = Message::parse(built.document().raw(), common::limits())?;
        assert_eq!(parsed.header(), &header);
        match (&body, parsed.body()) {
            (Outbound::RequestObservations, Body::Request(_))
            | (Outbound::Terminate(_), Body::Terminate(_)) => {}
            (Outbound::Accept(_), Body::Acknowledgement { kind, reference: r }) => {
                assert_eq!(*kind, AckKind::Accept);
                assert_eq!(r, &reference);
            }
            (Outbound::ApplicationError { .. }, Body::Acknowledgement { kind, reference: r }) => {
                assert_eq!(*kind, AckKind::Error);
                assert_eq!(r, &reference);
            }
            (Outbound::Escape { .. }, Body::Escape { reference: r, .. }) => {
                assert_eq!(r, &reference);
            }
            _ => return Err(MessageError::Structure),
        }
    }
    Ok(())
}

#[test]
fn poct01_project_builder_limits_and_explicit_offset() -> Result<(), MessageError> {
    let mut header = Header {
        control_id: ControlId::new("out")?,
        created: Timestamp::parse("2026-06-09T08:00:00+02:00")?,
    };
    let mut limits = common::limits();
    limits.message_bytes = 10;
    assert!(build(&header, &Outbound::RequestObservations, limits).is_err());
    header.created = Timestamp::parse("2026-06-09T08:00:00")?;
    assert!(build(&header, &Outbound::RequestObservations, common::limits()).is_err());
    // Exercise the shared peer fixtures in each integration-test module, so the
    // compiler can also check their declarations without dead-code exceptions.
    for raw in [
        common::hello("h"),
        common::status("s", "0"),
        common::observation("o", true, "<OBS.value NULL=\"UNK\"/>"),
    ] {
        Message::parse(raw.as_bytes(), common::limits())?;
    }
    Ok(())
}
