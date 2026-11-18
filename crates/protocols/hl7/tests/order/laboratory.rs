//! Tests HL7 O21 and O22 laboratory orders.
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::order::{
    self, Code, Error, Header, Order, OrderId, OrderMessage, OrderResponse, Outcome, Patient,
    Priority, Processing, SampleField, SampleId, Specimen, Subject, Timestamp, Timing,
};
use analyte_hl7::{AckCode, ControlId, Message, Options, Version, decode_text};

fn header() -> Result<Header<'static>, Error> {
    Ok(Header {
        application: b"HOST",
        facility: b"LAB",
        peer_application: b"MACHINE",
        peer_facility: b"LAB",
        control_id: ControlId::new(b"oml1")?,
        timestamp: Timestamp::new(b"20261115120000+0100")?,
        version: Version::V251,
        processing: Processing::Training,
    })
}

fn new_order<'a>(sample: &'a [u8], source: Code<'a>) -> Result<Order<'a>, Error> {
    Ok(Order::New {
        placer: OrderId::new(b"001", b"LAB")?,
        filler: None,
        test: Code::new(b"T1", b"Synthetic", b"L")?,
        specimen: Specimen {
            sample: SampleId::new(sample)?,
            field: SampleField::Placer1,
            source: Some(source),
        },
        priority: Priority::Stat,
        timing: Timing {
            priority_version: None,
            start: Some(Timestamp::new(b"202611151300")?),
            end: Some(Timestamp::new(b"202611151400")?),
        },
    })
}

fn subject() -> Subject<'static> {
    Subject::Patient(Patient {
        identifier_type: None,
        identifier: b"P1",
        namespace: b"LAB",
        family_name: b"Synthetic",
    })
}

fn sent() -> Result<OrderMessage, Error> {
    order::oml(
        &[
            new_order(b"000aB", Code::new(b"SER", b"Serum", b"HL70487")?)?,
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: Some(OrderId::new(b"F2", b"M")?),
            },
        ],
        subject(),
        header()?,
        Options::default(),
    )
}

fn reply(code: &[u8], body: &[u8]) -> Result<Message, Error> {
    let mut bytes =
        b"MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611151201||ORL^O22^ORL_O22|orl1|T|2.5.1\rMSA|".to_vec();
    bytes.extend_from_slice(code);
    bytes.extend_from_slice(b"|oml1\r");
    bytes.extend_from_slice(body);
    Ok(Message::parse(&bytes, Options::default())?)
}

fn orders_reply(code: &[u8], body: &[u8]) -> Result<Message, Error> {
    let mut bytes = b"PID|1||P1^^^LAB||Synthetic\r".to_vec();
    bytes.extend_from_slice(body);
    reply(code, &bytes)
}

// HL7 2.5.1 §4.4.6
#[test]
fn builds_oml_with_timing_specimen_and_cancel() -> Result<(), Error> {
    let sent = sent()?;
    let mut segments = sent.message().segments();
    let header = segments.next().ok_or(Error::Limit)?;
    assert_eq!(
        header.field(9).map(analyte_hl7::Field::raw),
        Some(b"OML^O21^OML_O21".as_slice())
    );
    assert!(header.field(21).is_none());
    assert_eq!(
        segments.next().map(analyte_hl7::Segment::id),
        Some(b"PID".as_slice())
    );
    let orc = segments.next().ok_or(Error::Limit)?;
    assert_eq!(
        orc.field(1).map(analyte_hl7::Field::raw),
        Some(b"NW".as_slice())
    );
    assert_eq!(
        orc.field(7).map(analyte_hl7::Field::raw),
        Some(b"".as_slice())
    );
    let tq1 = segments.next().ok_or(Error::Limit)?;
    assert_eq!(
        tq1.raw(),
        b"TQ1|1||||||202611151300|202611151400|S^^HL70485"
    );
    let obr = segments.next().ok_or(Error::Limit)?;
    assert_eq!(obr.id(), b"OBR");
    for field in [15, 18, 19, 27] {
        assert_eq!(
            obr.field(field).map(analyte_hl7::Field::raw),
            Some(b"".as_slice())
        );
    }
    let spm = segments.next().ok_or(Error::Limit)?;
    assert_eq!(spm.raw(), b"SPM|1|000aB||SER^Serum^HL70487");
    assert_eq!(
        segments.next().map(analyte_hl7::Segment::raw),
        Some(b"ORC|CA|002^LAB|F2^M".as_slice())
    );
    assert!(segments.next().is_none());
    assert!(sent.message().segments().all(|s| s.id() != b"SAC"));
    Ok(())
}

// HL7 2.5.1 §4.4.7
#[test]
fn orl_correlates_decisions_specimens_and_containers() -> Result<(), Error> {
    let sent = sent()?;
    let message=orders_reply(b"AA",b"ORC|OK|001^LAB|F1^M\rTQ1|1||||||202611151300|202611151400|S^^HL70485\rOBR|1|001^LAB|F1^M|T1^Synthetic^L\rSPM|1|000aB^peer&FILLER||SER^Serum^HL70487\rSAC|||tube-1^M\rSAC|||tube-2^M\rORC|CR|002^LAB|F2^M\r")?;
    let parsed = OrderResponse::new(&message, &sent)?;
    assert_eq!(parsed.acceptance(), AckCode::Aa);
    let first = parsed.decisions().first().ok_or(Error::Limit)?;
    assert!(matches!(first.outcome(), Outcome::Accepted));
    assert_eq!(first.placer().raw(), b"001^LAB");
    assert_eq!(first.timing().len(), 1);
    assert_eq!(first.specimens().len(), 1);
    let specimen = first.specimens().first().ok_or(Error::Limit)?;
    assert_eq!(
        specimen.segment().field(2).map(analyte_hl7::Field::raw),
        Some(b"000aB^peer&FILLER".as_slice())
    );
    assert_eq!(specimen.containers().len(), 2);
    assert_eq!(
        specimen
            .containers()
            .first()
            .and_then(|s| s.field(3))
            .map(analyte_hl7::Field::raw),
        Some(b"tube-1^M".as_slice())
    );
    let second = parsed.decisions().get(1).ok_or(Error::Limit)?;
    assert!(matches!(second.outcome(), Outcome::CancelAccepted));
    assert!(second.specimens().is_empty());
    assert_eq!(parsed.message().raw(), message.raw());
    Ok(())
}

// HL7 2.5.1 §4.5.1.1
#[test]
fn orl_new_and_cancel_rejections_keep_reason() -> Result<(), Error> {
    let sent = sent()?;
    let message=orders_reply(b"AE",b"ORC|UA|001^LAB||||||||||||||LOCAL^Unavailable^L\rORC|UC|002^LAB|F2^M|||||||||||||LOCAL^Running^L\r")?;
    let parsed = OrderResponse::new(&message, &sent)?;
    assert_eq!(parsed.acceptance(), AckCode::Ae);
    assert!(
        matches!(parsed.decisions().first().map(order::Decision::outcome),Some(Outcome::Rejected {reason:Some(reason)}) if reason.raw()==b"LOCAL^Unavailable^L")
    );
    assert!(
        matches!(parsed.decisions().get(1).map(order::Decision::outcome),Some(Outcome::UnableToCancel {reason:Some(reason)}) if reason.raw()==b"LOCAL^Running^L")
    );
    Ok(())
}

macro_rules! negative {
    ($name:ident,$body:expr,$pattern:pat) => {
        #[test]
        fn $name() -> Result<(), Error> {
            let sent = sent()?;
            let message = orders_reply(b"AA", $body)?;
            assert!(matches!(OrderResponse::new(&message, &sent), Err($pattern)));
            Ok(())
        }
    };
}

// HL7 2.5.1 §4.5.1.1
negative!(
    orl_unknown_control_is_refused,
    b"ORC|ZZ|001^LAB\r",
    Error::Control
);
// HL7 2.5.1 §4.5.1.1
negative!(
    orl_request_control_is_not_an_ack,
    b"ORC|NW|001^LAB\r",
    Error::Control
);
// HL7 2.5.1 §4.5.1.1
negative!(
    orl_ok_reply_to_cancel_is_refused,
    b"ORC|OK|002^LAB\r",
    Error::Control
);
// HL7 2.5.1 §4.5.1.1
negative!(
    orl_notification_control_is_not_an_ack,
    b"ORC|OC|001^LAB\r",
    Error::Control
);
// HL7 2.5.1 §4.5.1.2
negative!(
    orl_unknown_placer_is_refused,
    b"ORC|OK|other^LAB\r",
    Error::Correlation {
        segment: "ORC",
        field: 2
    }
);
// HL7 2.5.1 §4.5.1.2
negative!(
    orl_wrong_placer_namespace_is_refused,
    b"ORC|OK|001^OTHER\r",
    Error::Correlation {
        segment: "ORC",
        field: 2
    }
);
// HL7 2.5.1 §4.5.1.3
negative!(
    orl_wrong_filler_is_refused,
    b"ORC|CR|002^LAB|WRONG^M\r",
    Error::Correlation {
        segment: "ORC",
        field: 3
    }
);
// HL7 2.5.1 §4.4.7
negative!(
    orl_duplicate_order_is_refused,
    b"ORC|OK|001^LAB\rORC|OK|001^LAB\r",
    Error::Duplicate { field: 2 }
);
// HL7 2.5.1 §4.4.7
negative!(
    orl_conflicting_obr_placer_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|other^LAB||T1^Synthetic^L\r",
    Error::Correlation {
        segment: "OBR",
        field: 2
    }
);
// HL7 2.5.1 §4.4.7
negative!(
    orl_specimen_without_obr_is_refused,
    b"ORC|OK|001^LAB\rSPM|1|000aB||SER^Serum^HL70487\r",
    Error::Content { field: 0, .. }
);
// HL7 2.5.1 §4.4.7
negative!(
    orl_container_without_specimen_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSAC|||tube\r",
    Error::Content { field: 0, .. }
);
// HL7 2.5.1 §7.4.3.2
negative!(
    orl_sample_case_mismatch_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000AB||SER^Serum^HL70487\r",
    Error::Correlation {
        segment: "SPM",
        field: 2
    }
);
// HL7 2.5.1 §7.4.3.2
negative!(
    orl_sample_missing_leading_zero_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|00aB||SER^Serum^HL70487\r",
    Error::Correlation {
        segment: "SPM",
        field: 2
    }
);
// HL7 2.5.1 §7.4.3.2
negative!(
    orl_missing_sample_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|||SER^Serum^HL70487\r",
    Error::Correlation {
        segment: "SPM",
        field: 2
    }
);
// HL7 2.5.1 §7.4.3.2
negative!(
    orl_malformed_specimen_eip_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB^^extra||SER^Serum^HL70487\r",
    Error::Content { field: 2, .. }
);
// HL7 2.5.1 §7.4.3.2
negative!(
    orl_unpaired_universal_namespace_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB&&uid||SER^Serum^HL70487\r",
    Error::Content { field: 2, .. }
);
// HL7 2.5.1 §7.4.3.4
negative!(
    orl_missing_specimen_type_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB\r",
    Error::Content { field: 4, .. }
);
// HL7 2.5.1 §4.4.7
negative!(orl_extra_specimen_is_not_flattened,b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB||SER^Serum^HL70487\rSPM|2|000aB||SER^Serum^HL70487\r",Error::Content {field:2,..});
// HL7 2.5.1 §13.4.3
negative!(
    orl_container_without_identity_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB||SER^Serum^HL70487\rSAC\r",
    Error::Content { field: 3, .. }
);
// HL7 2.5.1 §13.4.3
negative!(orl_repeated_container_identity_is_refused,b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB||SER^Serum^HL70487\rSAC|||tube~other\r",Error::Content {field:3,..});
// HL7 2.5.1 §4.5.4
negative!(
    orl_invalid_timing_date_is_refused,
    b"ORC|OK|001^LAB\rTQ1|1||||||20260230\r",
    Error::Content { field: 7, .. }
);
// HL7 2.5.1 §4.5.4
negative!(
    orl_timing_conjunction_is_out_of_scope,
    b"ORC|OK|001^LAB\rTQ1|1|||||||||||S\r",
    Error::Content { field: 12, .. }
);
// HL7 2.5.1 §4.5.4
negative!(
    orl_repeated_timing_window_is_refused,
    b"ORC|OK|001^LAB\rTQ1|1\rTQ1|2\r",
    Error::Content { field: 0, .. }
);
// HL7 2.5.1 §4.5.5
negative!(
    orl_timing_relationship_is_refused,
    b"ORC|OK|001^LAB\rTQ1|1\rTQ2|1\r",
    Error::Content { field: 0, .. }
);
// HL7 2.5.1 §4.4.7
negative!(
    orl_misplaced_order_note_is_refused,
    b"ORC|OK|001^LAB\rNTE|1||Misplaced\r",
    Error::Content { field: 0, .. }
);
// HL7 2.5.1 §4.4.7
negative!(
    orl_bad_second_order_fails_whole_reply,
    b"ORC|OK|001^LAB\rORC|ZZ|002^LAB\r",
    Error::Control
);

// HL7 2.5.1 §4.4.7
#[test]
fn message_ack_never_fabricates_order_decisions() -> Result<(), Error> {
    let sent = sent()?;
    for code in [b"AA".as_slice(), b"AE", b"AR"] {
        let response = reply(code, b"")?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert_eq!(parsed.decisions().len(), 2);
        assert!(
            parsed
                .decisions()
                .iter()
                .all(|d| matches!(d.outcome(), Outcome::NotReported))
        );
    }
    let response = orders_reply(b"AA", b"ORC|OK|001^LAB\r")?;
    let parsed = OrderResponse::new(&response, &sent)?;
    assert!(matches!(
        parsed.decisions().get(1).map(order::Decision::outcome),
        Some(Outcome::NotReported)
    ));
    Ok(())
}

// HL7 2.5.1 §4.4.6, §4.4.7
#[test]
fn rejects_wrong_version_and_message_family() -> Result<(), Error> {
    let source = Code::new(b"SER", b"Serum", b"HL70487")?;
    let mut old = header()?;
    old.version = Version::V231;
    assert!(matches!(
        order::oml(
            &[new_order(b"000aB", source)?],
            subject(),
            old,
            Options::default()
        ),
        Err(Error::Content { field: 12, .. })
    ));
    let sent = sent()?;
    let response = orders_reply(b"AA", b"ORC|OK|001^LAB\r")?;
    let raw = String::from_utf8(response.raw().to_vec()).map_err(|_| Error::Limit)?;
    for kind in [
        "ORR^O02^ORR_O02",
        "ORL^O34^ORL_O42",
        "ORL^O22",
        "ORL^O22^ORL_O42",
    ] {
        let message = Message::parse(
            raw.rewrite("ORL^O22^ORL_O22", kind).as_bytes(),
            Options::default(),
        )?;
        if kind == "ORL^O22" {
            assert!(matches!(
                OrderResponse::new(&message, &sent),
                Err(Error::Syntax(analyte_hl7::Error::Control { field: 9, .. }))
            ));
        } else {
            assert!(matches!(
                OrderResponse::new(&message, &sent),
                Err(Error::Content { field: 9, .. })
            ));
        }
    }
    let orm = order::orm(
        &[new_order(b"000aB", source)?],
        subject(),
        header()?,
        Options::default(),
    )?;
    assert!(matches!(
        OrderResponse::new(&response, &orm),
        Err(Error::Content { field: 9, .. })
    ));
    Ok(())
}

// HL7 2.5.1 §4.4.7, §2.9
#[test]
fn orl_needs_patient_group_and_msa_correlation() -> Result<(), Error> {
    let sent = sent()?;
    let response = reply(b"AA", b"ORC|OK|001^LAB\r")?;
    assert!(matches!(
        OrderResponse::new(&response, &sent),
        Err(Error::Content { field: 0, .. })
    ));
    let response = orders_reply(b"AA", b"ORC|OK|001^LAB\r")?;
    let text = String::from_utf8(response.raw().to_vec()).map_err(|_| Error::Limit)?;
    let wrong = Message::parse(
        text.rewrite("MSA|AA|oml1", "MSA|AA|other").as_bytes(),
        Options::default(),
    )?;
    assert!(matches!(
        OrderResponse::new(&wrong, &sent),
        Err(Error::Correlation {
            segment: "MSA",
            field: 2
        })
    ));
    for code in [b"CA".as_slice(), b"CE", b"CR"] {
        let response = reply(code, b"")?;
        assert!(matches!(
            OrderResponse::new(&response, &sent),
            Err(Error::Content { field: 1, .. })
        ));
    }
    Ok(())
}

// HL7 2.5.1 §7.4.3.2
#[test]
fn sample_delimiters_are_escaped_keeping_identity() -> Result<(), Error> {
    let sample = b"000aB|^~\\&";
    let sent = order::oml(
        &[new_order(sample, Code::new(b"SER", b"Serum", b"HL70487")?)?],
        subject(),
        header()?,
        Options::default(),
    )?;
    let spm = sent
        .message()
        .segments()
        .find(|s| s.id() == b"SPM")
        .ok_or(Error::Limit)?;
    let encoded = spm.field(2).ok_or(Error::Limit)?;
    assert_eq!(
        decode_text(encoded.raw(), sent.message())?.as_bytes(),
        sample
    );
    let mut body = b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\r".to_vec();
    body.extend_from_slice(spm.raw());
    body.push(b'\r');
    let response = orders_reply(b"AA", &body)?;
    assert!(OrderResponse::new(&response, &sent).is_ok());
    Ok(())
}

// HL7 2.5.1 §2.A.13
#[test]
fn cwe_keeps_supplied_version_never_guesses() -> Result<(), Error> {
    let local = Code::new(b"LOCAL", b"Synthetic", b"L")?;
    assert!(matches!(
        order::oml(
            &[new_order(b"000aB", local)?],
            subject(),
            header()?,
            Options::default()
        ),
        Err(Error::Content { field: 4, .. })
    ));
    let local = local.with_version(b"v1")?;
    let sent = order::oml(
        &[new_order(b"000aB", local)?],
        subject(),
        header()?,
        Options::default(),
    )?;
    assert_eq!(
        sent.message()
            .segments()
            .find(|s| s.id() == b"SPM")
            .and_then(|s| s.field(4))
            .map(analyte_hl7::Field::raw),
        Some(b"LOCAL^Synthetic^L^^^^v1".as_slice())
    );
    assert!(
        order::orm(
            &[new_order(b"000aB", local)?],
            subject(),
            header()?,
            Options::default()
        )
        .is_err()
    );
    Ok(())
}

// HL7 2.5.1 §4.4.6
#[test]
fn builder_is_bounded_and_deterministic() -> Result<(), Error> {
    let first = sent()?;
    let second = sent()?;
    assert_eq!(first.message().raw(), second.message().raw());
    let order = new_order(b"000aB", Code::new(b"SER", b"Serum", b"HL70487")?)?;
    assert!(order::oml(&[], subject(), header()?, Options::default()).is_err());
    assert!(matches!(
        order::oml(&[order, order], subject(), header()?, Options::default()),
        Err(Error::Duplicate { field: 2 })
    ));
    let options = Options {
        limits: analyte_hl7::Limits {
            message_bytes: 40,
            ..analyte_hl7::Limits::default()
        },
        ..Options::default()
    };
    assert!(matches!(
        order::oml(&[order], subject(), header()?, options),
        Err(Error::Limit)
    ));
    Ok(())
}

proptest::proptest! {
    // HL7 2.5.1 §7.4.3.2
    #[test]
    fn sample_strings_round_trip(value in "0[0-9A-Za-z]{1,15}") {
        let message=Code::new(b"SER",b"Serum",b"HL70487").and_then(|source|new_order(value.as_bytes(),source)).and_then(|order|order::oml(&[order],subject(),header()?,Options::default()));
        proptest::prop_assert!(message.is_ok());
        if let Ok(message)=message {
            proptest::prop_assert_eq!(message.message().segments().find(|s|s.id()==b"SPM").and_then(|s|s.field(2)).map(analyte_hl7::Field::raw),Some(value.as_bytes()));
        }
    }
}

// HL7 2.5.1 §4.4.6, §4.4.7
#[test]
fn specimens_stay_with_their_own_orders() -> Result<(), Error> {
    let first = new_order(b"000aB", Code::new(b"SER", b"Serum", b"HL70487")?)?;
    let mut second = new_order(b"000cD", Code::new(b"SER", b"Serum", b"HL70487")?)?;
    match &mut second {
        Order::New { placer, .. } => *placer = OrderId::new(b"003", b"LAB")?,
        Order::Cancel { .. } => return Err(Error::Control),
    }
    let sent = order::oml(
        &[
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: None,
            },
            first,
            second,
        ],
        subject(),
        header()?,
        Options::default(),
    )?;
    let specimens: Vec<_> = sent
        .message()
        .segments()
        .filter(|s| s.id() == b"SPM")
        .map(analyte_hl7::Segment::raw)
        .collect();
    assert_eq!(
        specimens,
        vec![
            b"SPM|1|000aB||SER^Serum^HL70487".as_slice(),
            b"SPM|2|000cD||SER^Serum^HL70487"
        ]
    );
    let correct=orders_reply(b"AA",b"ORC|OK|003^LAB\rOBR|1|003^LAB||T1^Synthetic^L\rSPM|1|000cD||SER^Serum^HL70487\rORC|CR|002^LAB\rORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|2|000aB||SER^Serum^HL70487\r")?;
    assert!(OrderResponse::new(&correct, &sent).is_ok());
    let swapped = orders_reply(
        b"AA",
        b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000cD||SER^Serum^HL70487\r",
    )?;
    assert!(matches!(
        OrderResponse::new(&swapped, &sent),
        Err(Error::Correlation {
            segment: "SPM",
            field: 2
        })
    ));
    Ok(())
}

// HL7 2.5.1 §7.4.3.2
#[test]
fn specimen_id_length_checked_before_correlation() -> Result<(), Error> {
    let sent = sent()?;
    let mut body = b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|".to_vec();
    body.extend_from_slice(&[b'a'; 81]);
    body.extend_from_slice(b"||SER^Serum^HL70487\r");
    let response = orders_reply(b"AA", &body)?;
    assert!(matches!(
        OrderResponse::new(&response, &sent),
        Err(Error::Content { field: 2, .. })
    ));
    let sample = vec![b'|'; 27];
    assert!(matches!(
        order::oml(
            &[new_order(
                &sample,
                Code::new(b"SER", b"Serum", b"HL70487")?
            )?],
            subject(),
            header()?,
            Options::default()
        ),
        Err(Error::Content { field: 2, .. })
    ));
    let sample = vec![b'|'; 26];
    assert!(
        order::oml(
            &[new_order(
                &sample,
                Code::new(b"SER", b"Serum", b"HL70487")?
            )?],
            subject(),
            header()?,
            Options::default()
        )
        .is_ok()
    );
    Ok(())
}

// HL7 2.5.1 §4.4.7
#[test]
fn sft_and_header_notes_stay_in_message() -> Result<(), Error> {
    let sent = sent()?;
    let response=reply(b"AA",b"SFT|Synthetic|1|Software|binary\rNTE|1||Header note\rPID|1||P1^^^LAB||Synthetic\rORC|OK|001^LAB\r")?;
    let parsed = OrderResponse::new(&response, &sent)?;
    assert_eq!(parsed.message().raw(), response.raw());
    assert!(
        parsed
            .decisions()
            .first()
            .is_some_and(|d| d.notes().is_empty())
    );
    Ok(())
}

// HL7 2.5.1 §13.4.3
negative!(container_null_identifier_is_refused,b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB||SER^Serum^HL70487\rSAC|||\"\"^M\r",Error::Content {field:3,..});
// HL7 2.5.1 §4.5.4
negative!(
    invalid_timing_set_id_is_refused,
    b"ORC|OK|001^LAB\rTQ1|2\r",
    Error::Content { field: 1, .. }
);
// HL7 2.5.1 §4.5.4
negative!(
    priority_code_version_is_not_guessed,
    b"ORC|OK|001^LAB\rTQ1|1||||||||LOCAL^^L\r",
    Error::Content { field: 9, .. }
);
// HL7 2.5.1 §7.4.3
negative!(
    invalid_specimen_set_id_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|x|000aB||SER^Serum^HL70487\r",
    Error::Content { field: 1, .. }
);
// HL7 2.5.1 §7.4.3
negative!(
    invalid_specimen_type_code_is_refused,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB||SER^Serum^L\r",
    Error::Content { field: 4, .. }
);

// HL7 2.5.1 §4.4.6
#[test]
fn cancel_only_adds_no_specimen_or_container() -> Result<(), Error> {
    let sent = order::oml(
        &[Order::Cancel {
            placer: OrderId::new(b"001", b"LAB")?,
            filler: None,
        }],
        Subject::NonPatient,
        header()?,
        Options::default(),
    )?;
    assert_eq!(sent.message().segments().count(), 2);
    assert!(sent.message().raw().ends_with(b"ORC|CA|001^LAB|\r"));
    Ok(())
}

proptest::proptest! {
    // HL7 2.5.1 §4.5.1.1
    #[test]
    fn orl_unknown_control_codes_are_errors(value in "[A-Z]{2}") {
        proptest::prop_assume!(!["OK","UA"].contains(&value.as_str()));
        let input=sent().and_then(|sent| {
            let mut body=b"ORC|".to_vec();body.extend_from_slice(value.as_bytes());body.extend_from_slice(b"|001^LAB\r");
            let reply=orders_reply(b"AA",&body)?;
            Ok(matches!(OrderResponse::new(&reply,&sent),Err(Error::Control)))
        });
        proptest::prop_assert!(matches!(input,Ok(true)));
    }
}

// HL7 2.5.1 §7.4.3.2
negative!(
    specimen_namespace_is_not_taken_from_msh,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rSPM|1|000aB&UNKNOWN||SER^Serum^HL70487\r",
    Error::Correlation {
        segment: "SPM",
        field: 2
    }
);

// HL7 2.5.1 §4.5.4.9
#[test]
fn local_priority_kept_without_defaulting() -> Result<(), Error> {
    let sent = sent()?;
    let response = orders_reply(b"AA", b"ORC|OK|001^LAB\rTQ1|1||||||||URGENT^^L^^^^v1\r")?;
    let parsed = OrderResponse::new(&response, &sent)?;
    let decision = parsed.decisions().first().ok_or(Error::Limit)?;
    assert!(matches!(decision.outcome(), Outcome::Accepted));
    assert_eq!(
        decision
            .timing()
            .first()
            .and_then(|s| s.field(9))
            .map(analyte_hl7::Field::raw),
        Some(b"URGENT^^L^^^^v1".as_slice())
    );
    Ok(())
}
