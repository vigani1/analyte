//! Tests POCT reviewer XML message output.
mod peer_messages;
use analyte_poct::{
    AckKind, ApplicationError, Body, ControlId, EscapeReason, Header, Message, MessageError,
    Nullable, Outbound, TerminationReason, Timestamp, encode,
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
        let built = encode(&header, &body, peer_messages::limits())?;
        let parsed = Message::parse(built.document().raw(), peer_messages::limits())?;
        assert_eq!(parsed.header(), &header);
        match (&body, parsed.body()) {
            (Outbound::RequestObservations, Body::Request(Nullable::Value(code))) => {
                assert_eq!(code.value, "ROBS");
            }
            (Outbound::Terminate(_), Body::Terminate(Nullable::Value(code))) => {
                assert_eq!(code.value, "NRM");
            }
            (Outbound::Accept(_), Body::Acknowledgement { kind, reference: r }) => {
                assert_eq!(*kind, AckKind::Accept);
                assert_eq!(r, &reference);
            }
            (Outbound::ApplicationError { .. }, Body::Acknowledgement { kind, reference: r }) => {
                assert_eq!(*kind, AckKind::Error);
                assert!(
                    std::str::from_utf8(built.document().raw())
                        .expect("encoded XML")
                        .contains("<ACK.error_detail_cd V=\"202\"/>")
                );
                assert_eq!(r, &reference);
            }
            (
                Outbound::Escape { .. },
                Body::Escape {
                    reference: r,
                    reason,
                },
            ) => {
                assert_eq!(r, &reference);
                assert_eq!(reason, "CNC");
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
    let mut limits = peer_messages::limits();
    limits.message_bytes = 10;
    assert!(encode(&header, &Outbound::RequestObservations, limits).is_err());
    header.created = Timestamp::parse("2026-06-09T08:00:00")?;
    assert!(
        encode(
            &header,
            &Outbound::RequestObservations,
            peer_messages::limits()
        )
        .is_err()
    );
    // Exercise the shared peer fixtures in each integration-test module, so the
    // compiler can also check their declarations without dead-code exceptions.
    for raw in [
        peer_messages::hello("h"),
        peer_messages::status("s", "0"),
        peer_messages::observation("o", true, "<OBS.value NULL=\"UNK\"/>"),
    ] {
        Message::parse(raw.as_bytes(), peer_messages::limits())?;
    }
    Ok(())
}

#[test]
fn poct01_b_5_7_builder_honors_exact_device_receive_size() -> Result<(), MessageError> {
    let header = Header {
        control_id: ControlId::new("synthetic-&-id")?,
        created: Timestamp::parse("2026-03-12T09:41:02+01:00")?,
    };
    let built = encode(
        &header,
        &Outbound::RequestObservations,
        peer_messages::limits(),
    )?;
    let mut limits = peer_messages::limits();
    limits.message_bytes = built.document().raw().len();
    assert_eq!(
        encode(&header, &Outbound::RequestObservations, limits)?
            .document()
            .raw(),
        built.document().raw()
    );
    limits.message_bytes = limits
        .message_bytes
        .checked_sub(1)
        .ok_or(MessageError::Structure)?;
    assert!(encode(&header, &Outbound::RequestObservations, limits).is_err());
    Ok(())
}

// POCT01-A2 Appendix B 5.3 and 6.2: independently inspect the error detail attribute.
#[test]
fn poct01_b_6_2_error_ack_detail_matches_classification() -> Result<(), MessageError> {
    let header = Header {
        control_id: ControlId::new("reply")?,
        created: Timestamp::parse("2026-06-09T08:00:00+02:00")?,
    };
    for (error, code) in [
        (ApplicationError::ObjectSequence, "100"),
        (ApplicationError::RequiredField, "101"),
        (ApplicationError::DataType, "102"),
        (ApplicationError::TableValue, "103"),
        (ApplicationError::UnsupportedValue, "200"),
        (ApplicationError::UnsupportedVersion, "201"),
        (ApplicationError::Internal, "202"),
    ] {
        let built = encode(
            &header,
            &Outbound::ApplicationError {
                reference: ControlId::new("incoming")?,
                error,
            },
            peer_messages::limits(),
        )?;
        let raw = std::str::from_utf8(built.document().raw()).expect("encoder writes UTF-8");
        assert!(raw.contains(&format!("<ACK.error_detail_cd V=\"{code}\"/>")));
        assert!(
            matches!(built.body(), Body::Acknowledgement { kind: AckKind::Error, reference } if reference.as_str() == "incoming")
        );
    }
    Ok(())
}
