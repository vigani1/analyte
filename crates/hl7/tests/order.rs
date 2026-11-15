//! Synthetic ORM/ORR fixtures: HL7 2.3.1 ch.4 / 2.5.1 §§4.4.1–2, 4.5.1.
use analyte_hl7::order::{
    self, Code, Control, Error, Header, Order, OrderId, OrderMessage, OrderResponse, Outcome,
    Patient, Priority, Processing, SampleField, SampleId, Specimen, Subject, Timestamp, Timing,
};
use analyte_hl7::{AckCode, ControlId, Message, Options, Version, decode_text};

fn header(version: Version) -> Result<Header<'static>, Error> {
    Ok(Header {
        application: b"HOST",
        facility: b"LAB",
        peer_application: b"MACHINE",
        peer_facility: b"LAB",
        control_id: ControlId::new(b"sent1")?,
        timestamp: Timestamp::new(b"20261115090000+0100")?,
        version,
        processing: Processing::Training,
    })
}

fn new_order(sample: &[u8], field: SampleField) -> Result<Order<'_>, Error> {
    Ok(Order::New {
        placer: OrderId::new(b"001", b"LAB")?,
        filler: None,
        test: Code::new(b"T1", b"Synthetic", b"L")?,
        specimen: Specimen {
            sample: SampleId::new(sample)?,
            field,
            source: Some(Code::new(b"SER", b"Serum", b"L")?),
        },
        priority: Priority::Stat,
        timing: Timing {
            start: Some(Timestamp::new(b"202611151000")?),
            end: Some(Timestamp::new(b"202611151100")?),
        },
    })
}

fn sent(version: Version) -> Result<OrderMessage, Error> {
    order::orm(
        &[
            new_order(b"000aB", SampleField::Placer1)?,
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: Some(OrderId::new(b"F2", b"M")?),
            },
        ],
        Subject::Patient(Patient {
            identifier: b"P1",
            namespace: b"LAB",
            family_name: b"Synthetic",
        }),
        header(version)?,
        Options::default(),
    )
}

fn reply(version: Version, code: &[u8], body: &[u8]) -> Result<Message, Error> {
    let edition = match version {
        Version::V231 => "2.3.1",
        Version::V251 => "2.5.1",
    };
    let mut bytes = format!(
        "MSH|^~\\&|MACHINE|LAB|HOST|LAB|202611150901||ORR^O02^ORR_O02|reply1|T|{edition}\rMSA|"
    )
    .into_bytes();
    bytes.extend_from_slice(code);
    bytes.extend_from_slice(b"|sent1\r");
    bytes.extend_from_slice(body);
    Ok(Message::parse(&bytes, Options::default())?)
}

fn orc(control: &[u8], placer: &[u8], filler: &[u8], status: &[u8], reason: &[u8]) -> Vec<u8> {
    let mut bytes = b"ORC".to_vec();
    for value in [
        control, placer, filler, b"", status, b"", b"", b"", b"", b"", b"", b"", b"", b"", b"",
        reason,
    ] {
        bytes.push(b'|');
        bytes.extend_from_slice(value);
    }
    bytes.push(b'\r');
    bytes
}

#[test]
fn hl7_231_4_2_1_and_251_4_4_1_build_new_and_cancel_groups() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let mut segments = sent.message().segments();
        assert_eq!(
            segments.next().map(analyte_hl7::Segment::id),
            Some(b"MSH".as_slice())
        );
        assert_eq!(
            segments.next().map(analyte_hl7::Segment::raw),
            Some(b"PID|1||P1^^^LAB||Synthetic".as_slice())
        );
        let common = segments.next().ok_or(Error::Limit)?;
        assert_eq!(
            common.raw(),
            b"ORC|NW|001^LAB||||F|^^^202611151000^202611151100^S"
        );
        let detail = segments.next().ok_or(Error::Limit)?;
        for (field, value) in [
            (2, b"001^LAB".as_slice()),
            (4, b"T1^Synthetic^L"),
            (15, b"SER&Serum&L"),
            (18, b"000aB"),
            (27, b"^^^202611151000^202611151100^S"),
        ] {
            assert_eq!(
                detail.field(field).map(analyte_hl7::Field::raw),
                Some(value)
            );
        }
        assert_eq!(
            segments.next().map(analyte_hl7::Segment::raw),
            Some(b"ORC|CA|002^LAB|F2^M".as_slice())
        );
        assert!(segments.next().is_none());
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_and_251_4_5_1_reports_each_matching_outcome() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        for (new, cancel, accepted) in [
            (b"OK".as_slice(), b"CR".as_slice(), true),
            (b"UA", b"UC", false),
        ] {
            let mut body = orc(
                new,
                b"001^LAB",
                b"F1^M",
                b"",
                if accepted {
                    b""
                } else {
                    b"LOCAL^Cannot perform^L"
                },
            );
            body.extend_from_slice(b"NTE|1||Order note\r");
            body.extend_from_slice(&orc(
                cancel,
                b"002^LAB",
                b"F2^M",
                b"",
                if accepted {
                    b""
                } else {
                    b"LOCAL^Already running^L"
                },
            ));
            let message = reply(version, b"AA", &body)?;
            let response = OrderResponse::new(&message, &sent)?;
            assert_eq!(response.acceptance(), AckCode::Aa);
            assert_eq!(response.message().raw(), message.raw());
            let mut decisions = response.decisions().iter();
            let first = decisions.next().ok_or(Error::Limit)?;
            assert_eq!(first.placer().raw(), b"001^LAB");
            assert_eq!(first.request(), Control::New);
            assert_eq!(first.notes().len(), 1);
            assert_eq!(
                first.filler().map(analyte_hl7::Field::raw),
                Some(b"F1^M".as_slice())
            );
            if accepted {
                assert!(matches!(first.outcome(), Outcome::Accepted));
            } else {
                assert!(
                    matches!(first.outcome(),Outcome::Rejected {reason:Some(reason)} if reason.raw()==b"LOCAL^Cannot perform^L")
                );
            }
            let second = decisions.next().ok_or(Error::Limit)?;
            assert_eq!(second.request(), Control::Cancel);
            if accepted {
                assert!(matches!(second.outcome(), Outcome::CancelAccepted));
            } else {
                assert!(
                    matches!(second.outcome(),Outcome::UnableToCancel {reason:Some(reason)} if reason.raw()==b"LOCAL^Already running^L")
                );
            }
            assert!(decisions.next().is_none());
        }
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_2_and_251_4_4_2_message_ack_is_not_order_acceptance() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        for code in [b"AA".as_slice(), b"AE", b"AR"] {
            let message = reply(version, code, b"")?;
            let response = OrderResponse::new(&message, &sent)?;
            assert_eq!(response.decisions().len(), 2);
            assert!(
                response
                    .decisions()
                    .iter()
                    .all(|d| matches!(d.outcome(), Outcome::NotReported))
            );
        }
        let message = reply(version, b"AA", b"ORC|UA|001^LAB\r")?;
        let response = OrderResponse::new(&message, &sent)?;
        assert!(matches!(
            response.decisions().first().map(order::Decision::outcome),
            Some(Outcome::Rejected { reason: None })
        ));
        assert!(matches!(
            response.decisions().get(1).map(order::Decision::outcome),
            Some(Outcome::NotReported)
        ));
    }
    Ok(())
}

macro_rules! bad_order_reply {
    ($name:ident,$body:expr,$pattern:pat) => {
        #[test]
        fn $name() -> Result<(), Error> {
            for version in [Version::V231, Version::V251] {
                let sent = sent(version)?;
                let message = reply(version, b"AA", $body)?;
                assert!(matches!(OrderResponse::new(&message, &sent), Err($pattern)));
            }
            Ok(())
        }
    };
}

bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_unknown_control,
    b"ORC|ZZ|001^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_request_code_in_reply,
    b"ORC|NW|001^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_notification_is_not_acceptance,
    b"ORC|OC|001^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_received_is_not_accepted,
    b"ORC|RR|001^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_cancel_reply_to_new,
    b"ORC|CR|001^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_new_reply_to_cancel,
    b"ORC|OK|002^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_2_and_251_4_5_1_2_unknown_placer,
    b"ORC|OK|other^LAB\r",
    Error::Correlation {
        segment: "ORC",
        field: 2
    }
);
bad_order_reply!(
    hl7_231_4_3_1_2_and_251_4_5_1_2_wrong_namespace,
    b"ORC|OK|001^OTHER\r",
    Error::Correlation {
        segment: "ORC",
        field: 2
    }
);
bad_order_reply!(
    hl7_231_4_3_1_3_and_251_4_5_1_3_wrong_filler,
    b"ORC|CR|002^LAB|WRONG^M\r",
    Error::Correlation {
        segment: "ORC",
        field: 3
    }
);
bad_order_reply!(
    hl7_231_4_3_1_and_251_4_5_1_duplicate_reply,
    b"ORC|OK|001^LAB\rORC|OK|001^LAB\r",
    Error::Duplicate { field: 2 }
);
bad_order_reply!(
    hl7_231_4_3_1_and_251_4_5_1_missing_placer,
    b"ORC|OK\r",
    Error::Content { field: 2, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_and_251_4_5_1_null_placer,
    b"ORC|OK|\"\"\r",
    Error::Content { field: 2, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_and_251_4_5_1_repeated_placer,
    b"ORC|OK|001^LAB~001^LAB\r",
    Error::Content { field: 2, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_and_251_4_5_1_malformed_ei,
    b"ORC|OK|001^LAB^uid\r",
    Error::Content { field: 2, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_b_and_251_4_5_1_b_orc_obr_identity_conflict,
    b"ORC|OK|001^LAB\rOBR|1|other^LAB||T1^Synthetic^L\r",
    Error::Correlation {
        segment: "OBR",
        field: 2
    }
);
bad_order_reply!(
    hl7_231_4_3_1_b_and_251_4_5_1_b_orc_obr_filler_conflict,
    b"ORC|OK|001^LAB|F1^M\rOBR|1|001^LAB|F2^M|T1^Synthetic^L\r",
    Error::Correlation {
        segment: "OBR",
        field: 3
    }
);
bad_order_reply!(hl7_231_4_3_1_b_and_251_4_5_1_b_timing_conflict,b"ORC|OK|001^LAB|||||^^^20261115\rOBR|1|001^LAB||T1^Synthetic^L|||||||||||||||||||||||^^^20261116\r",Error::Correlation {segment:"OBR",field:27});
bad_order_reply!(
    hl7_231_4_2_2_and_251_4_4_2_obr_requires_orc,
    b"OBR|1|001^LAB||T1^Synthetic^L\r",
    Error::Content { field: 0, .. }
);
bad_order_reply!(
    hl7_231_4_2_2_and_251_4_4_2_obr_requires_service,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB\r",
    Error::Content { field: 4, .. }
);
bad_order_reply!(
    hl7_231_4_2_2_and_251_4_4_2_wrong_segment_owner,
    b"ORC|OK|001^LAB\rPID|1||P1^^^LAB\r",
    Error::Content { field: 0, .. }
);
bad_order_reply!(
    hl7_231_4_2_2_and_251_4_4_2_unsupported_result_content,
    b"ORC|OK|001^LAB\rOBX|1|ST|T1||value\r",
    Error::Content { field: 0, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_5_and_251_4_5_1_5_invalid_status,
    b"ORC|OK|001^LAB|||ZZ\r",
    Error::Content { field: 5, .. }
);

#[test]
fn hl7_231_4_3_1_b_and_251_4_5_1_b_obr_identity_fallback_and_response_order() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let message = reply(
            version,
            b"AA",
            b"ORC|CR|002^LAB|F2^M\rORC|OK\rOBR|1|001^LAB|F1^M|T1^Synthetic^L\rNTE|1||Detail note\r",
        )?;
        let response = OrderResponse::new(&message, &sent)?;
        assert!(matches!(
            response.decisions().first().map(order::Decision::outcome),
            Some(Outcome::Accepted)
        ));
        assert!(matches!(
            response.decisions().get(1).map(order::Decision::outcome),
            Some(Outcome::CancelAccepted)
        ));
        assert!(
            response
                .decisions()
                .first()
                .and_then(order::Decision::detail)
                .is_some()
        );
    }
    Ok(())
}

#[test]
fn hl7_231_2_12_and_251_2_9_msa_correlates_sent_orm() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let message = reply(version, b"AA", b"ORC|OK|001^LAB\r")?;
        let bytes = String::from_utf8(message.raw().to_vec()).map_err(|_| Error::Limit)?;
        for (from, to, segment, field) in [
            ("MSA|AA|sent1", "MSA|AA|wrong", "MSA", 2),
            ("|reply1|", "|sent1|", "MSH", 10),
        ] {
            let changed = Message::parse(bytes.replace(from, to).as_bytes(), Options::default())?;
            assert!(
                matches!(OrderResponse::new(&changed,&sent),Err(Error::Correlation {segment:s,field:f}) if s==segment && f==field)
            );
        }
        for code in [b"CA".as_slice(), b"CE", b"CR"] {
            let message = reply(version, code, b"")?;
            assert!(matches!(
                OrderResponse::new(&message, &sent),
                Err(Error::Content { field: 1, .. })
            ));
        }
    }
    let sent = sent(Version::V231)?;
    let message = reply(Version::V251, b"AA", b"")?;
    assert!(matches!(
        OrderResponse::new(&message, &sent),
        Err(Error::Correlation {
            segment: "MSH",
            field: 12
        })
    ));
    Ok(())
}

#[test]
fn hl7_231_7_3_1_18_19_and_251_7_4_1_18_19_sample_spelling_and_escaping() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        for (placement, field) in [(SampleField::Placer1, 18), (SampleField::Placer2, 19)] {
            let sample = b"000aB|^~\\&";
            let order = new_order(sample, placement)?;
            let sent = order::orm(
                &[order],
                Subject::NonPatient,
                header(version)?,
                Options::default(),
            )?;
            let detail = sent
                .message()
                .segments()
                .find(|s| s.id() == b"OBR")
                .ok_or(Error::Limit)?;
            let value = detail.field(field).ok_or(Error::Limit)?;
            assert_eq!(decode_text(value.raw(), sent.message())?.as_bytes(), sample);
            assert_eq!(SampleId::new(sample)?.raw(), sample);
        }
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_1_and_251_4_4_1_rejects_empty_duplicate_and_overlimit_orders() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let input = new_order(b"000aB", SampleField::Placer1)?;
        assert!(
            order::orm(
                &[],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            )
            .is_err()
        );
        assert!(matches!(
            order::orm(
                &[input, input],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
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
            order::orm(&[input], Subject::NonPatient, header(version)?, options),
            Err(Error::Limit)
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_and_251_4_5_1_replay_is_pure_and_keeps_identities() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let first = sent(version)?;
        let second = sent(version)?;
        assert_eq!(first.message().raw(), second.message().raw());
        let message = reply(version, b"AA", b"ORC|OK|001^LAB\r")?;
        for _ in 0..2 {
            let parsed = OrderResponse::new(&message, &first)?;
            assert_eq!(
                parsed.decisions().first().map(|d| d.placer().raw()),
                Some(b"001^LAB".as_slice())
            );
        }
        assert_eq!(first.message().raw(), second.message().raw());
    }
    Ok(())
}

proptest::proptest! {
    #[test]
    fn hl7_231_7_3_1_18_and_251_7_4_1_18_sample_ids_are_strings(value in "0[0-9A-Za-z]{1,15}") {
        for version in [Version::V231,Version::V251] {
            let sent=new_order(value.as_bytes(),SampleField::Placer1).and_then(|order|order::orm(&[order],Subject::NonPatient,header(version)?,Options::default()));
            proptest::prop_assert!(sent.is_ok());
            if let Ok(sent)=sent {
                let sample=sent.message().segments().find(|s|s.id()==b"OBR").and_then(|s|s.field(18)).map(analyte_hl7::Field::raw);
                proptest::prop_assert_eq!(sample,Some(value.as_bytes()));
            }
        }
    }
}

#[test]
fn hl7_231_4_3_1_1_and_251_4_5_1_1_exhaustive_request_reply_pairs() -> Result<(), Error> {
    let controls = [
        Control::New,
        Control::Cancel,
        Control::Accepted,
        Control::Rejected,
        Control::Cancelled,
        Control::UnableToCancel,
    ];
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        for control in controls {
            for (placer, allowed) in [
                (
                    b"001^LAB".as_slice(),
                    matches!(control, Control::Accepted | Control::Rejected),
                ),
                (
                    b"002^LAB".as_slice(),
                    matches!(control, Control::Cancelled | Control::UnableToCancel),
                ),
            ] {
                let message = reply(version, b"AA", &orc(control.wire(), placer, b"", b"", b""))?;
                let result = OrderResponse::new(&message, &sent);
                if allowed {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(result, Err(Error::Control)));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_2_3_and_251_4_5_1_2_3_universal_namespaces_preserved() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let id = OrderId::with_universal(b"001", b"LAB", Some((b"U", b"ISO")))?;
        let sent = order::orm(
            &[Order::Cancel {
                placer: id,
                filler: None,
            }],
            Subject::NonPatient,
            header(version)?,
            Options::default(),
        )?;
        let common = sent
            .message()
            .segments()
            .find(|s| s.id() == b"ORC")
            .ok_or(Error::Limit)?;
        assert_eq!(
            common.field(2).map(analyte_hl7::Field::raw),
            Some(b"001^LAB^U^ISO".as_slice())
        );
        let response = reply(version, b"AA", b"ORC|CR|001^LAB^U^ISO\r")?;
        assert!(OrderResponse::new(&response, &sent).is_ok());
        let wrong = reply(version, b"AA", b"ORC|CR|001^LAB^OTHER^ISO\r")?;
        assert!(matches!(
            OrderResponse::new(&wrong, &sent),
            Err(Error::Correlation {
                segment: "ORC",
                field: 2
            })
        ));
        let alias = OrderId::new(b"001", b"LAB")?;
        assert!(matches!(
            order::orm(
                &[
                    Order::Cancel {
                        placer: id,
                        filler: None
                    },
                    Order::Cancel {
                        placer: alias,
                        filler: None
                    }
                ],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
            Err(Error::Duplicate { field: 2 })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_7_3_1_3_and_251_7_4_1_3_filler_identifies_one_order() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let shared = Some(OrderId::new(b"F1", b"M")?);
        let orders = [
            Order::Cancel {
                placer: OrderId::new(b"001", b"LAB")?,
                filler: shared,
            },
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: shared,
            },
        ];
        assert!(matches!(
            order::orm(
                &orders,
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
            Err(Error::Duplicate { field: 3 })
        ));
        let sent = sent(version)?;
        let response = reply(
            version,
            b"AA",
            b"ORC|OK|001^LAB|F2^M\rORC|CR|002^LAB|F2^M\r",
        )?;
        assert!(matches!(
            OrderResponse::new(&response, &sent),
            Err(Error::Correlation {
                segment: "ORC",
                field: 3
            })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_2_7_and_251_2_5_4_response_delimiters_are_independent() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let edition = match version {
            Version::V231 => "2.3.1",
            Version::V251 => "2.5.1",
        };
        let bytes = format!(
            "MSH!$%?@!MACHINE!LAB!HOST!LAB!20261115!!ORR$O02$ORR_O02!reply!T!{edition}\rMSA!AA!sent1\rORC!OK!001$LAB$$!F1$M\rORC!CR!002$LAB!F2$M\r"
        );
        let response = Message::parse(bytes.as_bytes(), Options::default())?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert!(matches!(
            parsed.decisions().first().map(order::Decision::outcome),
            Some(Outcome::Accepted)
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_5_and_251_4_5_1_5_snapshot_does_not_change_request_outcome() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let response = reply(version, b"AA", b"ORC|OK|001^LAB|||CM\r")?;
        let parsed = OrderResponse::new(&response, &sent)?;
        let decision = parsed.decisions().first().ok_or(Error::Limit)?;
        assert!(matches!(decision.outcome(), Outcome::Accepted));
        assert_eq!(
            decision
                .common()
                .and_then(|s| s.field(5))
                .map(analyte_hl7::Field::raw),
            Some(b"CM".as_slice())
        );
    }
    Ok(())
}

#[test]
fn hl7_231_4_3_1_16_and_251_4_5_1_16_rejection_reason_validation() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        for reason in [b"\"\"".as_slice(), b"A~B", b"A^B^C^D^E^F^G", b"A&B"] {
            let response = reply(version, b"AA", &orc(b"UA", b"001^LAB", b"", b"", reason))?;
            assert!(matches!(
                OrderResponse::new(&response, &sent),
                Err(Error::Content { field: 16, .. })
            ));
        }
        let response = reply(
            version,
            b"AE",
            &orc(b"UA", b"001^LAB", b"", b"", b"LOCAL^Rejected^L"),
        )?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert_eq!(parsed.acceptance(), AckCode::Ae);
        assert!(matches!(
            parsed.decisions().first().map(order::Decision::outcome),
            Some(Outcome::Rejected { .. })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_2_and_251_4_4_2_requires_orr_and_msa() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let response = reply(version, b"AA", b"ORC|OK|001^LAB\r")?;
        let text = String::from_utf8(response.raw().to_vec()).map_err(|_| Error::Limit)?;
        let changed = Message::parse(
            text.replace("ORR^O02^ORR_O02", "ACK^O01^ACK").as_bytes(),
            Options::default(),
        )?;
        assert!(matches!(
            OrderResponse::new(&changed, &sent),
            Err(Error::Content { field: 9, .. })
        ));
        let changed = Message::parse(
            text.replace("MSA|AA|sent1\r", "").as_bytes(),
            Options::default(),
        )?;
        assert!(matches!(
            OrderResponse::new(&changed, &sent),
            Err(Error::Syntax(_))
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_2_8_and_251_2a_caller_data_rejects_injection_and_oversize() -> Result<(), Error> {
    assert!(OrderId::new(b"001|ORC", b"LAB").is_err());
    assert!(OrderId::new(b"001", b"").is_err());
    assert!(OrderId::new(b"1234567890123456789012", b"LAB").is_err());
    assert!(OrderId::with_universal(b"001", b"", Some((b"U", b"ZZZ"))).is_err());
    assert!(Code::new(b"T1", b"Injected\rOBR", b"L").is_err());
    assert!(Code::new(b"T1", b"Synthetic", b"").is_err());
    assert!(SampleId::new(b"").is_err());
    assert!(SampleId::new(b"000\r1").is_err());
    assert!(Timestamp::new(b"20260230").is_err());
    for version in [Version::V231, Version::V251] {
        let sample = vec![b'|'; 21];
        let input = new_order(&sample, SampleField::Placer1)?;
        assert!(matches!(
            order::orm(
                &[input],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
            Err(Error::Content { field: 18, .. })
        ));
        let mut injected = header(version)?;
        injected.control_id = ControlId::new(b"bad|id")?;
        assert!(matches!(
            order::orm(
                &[new_order(b"000aB", SampleField::Placer1)?],
                Subject::NonPatient,
                injected,
                Options::default()
            ),
            Err(Error::Content { field: 10, .. })
        ));
    }
    Ok(())
}

proptest::proptest! {
    #[test]
    fn hl7_231_4_3_1_1_and_251_4_5_1_1_arbitrary_control_codes_never_default_to_acceptance(value in "[A-Z]{2}") {
        let parsed=Control::parse(value.as_bytes());
        match parsed {
            Ok(control)=>proptest::prop_assert_eq!(control.wire(),value.as_bytes()),
            Err(error)=>{
                proptest::prop_assert!(matches!(error,Error::Control));
                proptest::prop_assert!(!["NW","CA","OK","UA","CR","UC"].contains(&value.as_str()));
            },
        }
    }
}

bad_order_reply!(
    hl7_231_4_2_2_and_251_4_4_2_multiple_details_are_not_flattened,
    b"ORC|OK|001^LAB\rOBR|1|001^LAB||T1^Synthetic^L\rOBR|2|001^LAB||T2^Other^L\r",
    Error::Content { field: 0, .. }
);
bad_order_reply!(
    hl7_231_4_3_1_1_and_251_4_5_1_1_bad_second_order_rejects_response_atomically,
    b"ORC|OK|001^LAB\rORC|ZZ|002^LAB\r",
    Error::Control
);
bad_order_reply!(
    hl7_231_4_3_1_2_and_251_4_5_1_2_no_numeric_placer_normalization,
    b"ORC|OK|1^LAB\r",
    Error::Correlation {
        segment: "ORC",
        field: 2
    }
);

#[test]
fn hl7_231_4_2_2_and_251_4_4_2_header_error_and_patient_context_retained() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let body=match version {
            Version::V231=>b"ERR|ORC^1^1^102\rNTE|1||Header note\rPID|1||P1^^^LAB||Synthetic\rNTE|1||Patient note\rORC|UA|001^LAB\r".as_slice(),
            Version::V251=>b"ERR||ORC^1^1|102|E\rNTE|1||Header note\rPID|1||P1^^^LAB||Synthetic\rNTE|1||Patient note\rORC|UA|001^LAB\r",
        };
        let response = reply(version, b"AE", body)?;
        let parsed = OrderResponse::new(&response, &sent)?;
        assert_eq!(parsed.acceptance(), AckCode::Ae);
        assert_eq!(parsed.message().raw(), response.raw());
        assert!(
            parsed
                .decisions()
                .first()
                .is_some_and(|d| d.notes().is_empty())
        );
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_1_and_251_4_4_1_nonpatient_cancel_needs_no_invented_detail() -> Result<(), Error> {
    for version in [Version::V231, Version::V251] {
        let sent = order::orm(
            &[Order::Cancel {
                placer: OrderId::new(b"001", b"LAB")?,
                filler: None,
            }],
            Subject::NonPatient,
            header(version)?,
            Options::default(),
        )?;
        assert_eq!(sent.message().segments().count(), 2);
        assert!(sent.message().raw().ends_with(b"ORC|CA|001^LAB|\r"));
        assert!(
            sent.message()
                .segments()
                .all(|s| s.id() != b"OBR" && s.id() != b"PID")
        );
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_1_18_and_251_2_15_9_18_utf8_sample_is_retained() -> Result<(), Error> {
    let sample = "000éB".as_bytes();
    let message = order::orm(
        &[new_order(sample, SampleField::Placer1)?],
        Subject::NonPatient,
        header(Version::V251)?,
        Options {
            charset: analyte_hl7::Charset::Utf8,
            ..Options::default()
        },
    )?;
    let field = message
        .message()
        .segments()
        .find(|s| s.id() == b"OBR")
        .and_then(|s| s.field(18))
        .ok_or(Error::Limit)?;
    assert_eq!(field.raw(), sample);
    Ok(())
}

#[test]
fn hl7_231_7_3_1_3_and_251_7_4_1_3_duplicate_new_filler_assignments_rejected() -> Result<(), Error>
{
    for version in [Version::V231, Version::V251] {
        let orders = [
            Order::Cancel {
                placer: OrderId::new(b"001", b"LAB")?,
                filler: None,
            },
            Order::Cancel {
                placer: OrderId::new(b"002", b"LAB")?,
                filler: None,
            },
        ];
        let sent = order::orm(
            &orders,
            Subject::NonPatient,
            header(version)?,
            Options::default(),
        )?;
        let response = reply(
            version,
            b"AA",
            b"ORC|CR|001^LAB|F1^M\rORC|CR|002^LAB|F1^M\r",
        )?;
        assert!(matches!(
            OrderResponse::new(&response, &sent),
            Err(Error::Duplicate { field: 3 })
        ));
        let first = OrderId::with_universal(b"001", b"A", Some((b"U", b"ISO")))?;
        let second = OrderId::with_universal(b"001", b"B", Some((b"U", b"ISO")))?;
        assert!(matches!(
            order::orm(
                &[
                    Order::Cancel {
                        placer: first,
                        filler: None
                    },
                    Order::Cancel {
                        placer: second,
                        filler: None
                    }
                ],
                Subject::NonPatient,
                header(version)?,
                Options::default()
            ),
            Err(Error::Duplicate { field: 2 })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_4_2_3_and_251_4_4_3_unsupported_order_families_are_located_errors() -> Result<(), Error>
{
    for version in [Version::V231, Version::V251] {
        let sent = sent(version)?;
        let message = reply(version, b"AA", b"ORC|OK|001^LAB\r")?;
        let text = String::from_utf8(message.raw().to_vec()).map_err(|_| Error::Limit)?;
        let families: &[&str] = match version {
            Version::V231 => &["OSR^Q06^OSR_Q06"],
            Version::V251 => &[
                "OSR^Q06^OSR_Q06",
                "ORG^O20^ORG_O20",
                "ORL^O36^ORL_O36",
                "ORI^O24^ORI_O24",
            ],
        };
        for family in families {
            let wrong = Message::parse(
                text.replace("ORR^O02^ORR_O02", family).as_bytes(),
                Options::default(),
            )?;
            assert!(matches!(
                OrderResponse::new(&wrong, &sent),
                Err(Error::Content { field: 9, .. })
            ));
            assert_eq!(
                wrong.raw(),
                text.replace("ORR^O02^ORR_O02", family).as_bytes()
            );
        }
    }
    for code in [
        b"SS".as_slice(),
        b"SR",
        b"XO",
        b"XR",
        b"UX",
        b"DC",
        b"DR",
        b"UD",
    ] {
        assert!(matches!(Control::parse(code), Err(Error::Control)));
    }
    Ok(())
}

#[test]
fn hl7_231_ch4_explicit_group_position_and_identifier_do_not_invent_context() -> Result<(), Error> {
    let mut request = new_order(b"000aB", SampleField::Placer1)?;
    if let Order::New { specimen, .. } = &mut request {
        specimen.source = None;
    }
    let built = order::orm(
        &[request],
        Subject::Identifier {
            identifier: b"000Patient",
            namespace: b"LAB",
        },
        header(Version::V231)?,
        Options::default(),
    )?;
    let enriched = order::context(
        &built,
        b"app-order",
        Some((SampleField::Placer2, b"007b")),
        Options::default(),
    )?;
    let common = enriched
        .message()
        .segments()
        .find(|s| s.id() == b"ORC")
        .ok_or(Error::Limit)?;
    assert_eq!(common.field(4).ok_or(Error::Limit)?.raw(), b"app-order");
    let detail = enriched
        .message()
        .segments()
        .find(|s| s.id() == b"OBR")
        .ok_or(Error::Limit)?;
    assert_eq!(detail.field(15).ok_or(Error::Limit)?.raw(), b"");
    assert_eq!(detail.field(18).ok_or(Error::Limit)?.raw(), b"000aB");
    assert_eq!(detail.field(19).ok_or(Error::Limit)?.raw(), b"007b");
    // HL7 2.3.1 section 3.3.2 p.3-36: PID-5 is required when PID is present.
    assert!(!enriched.message().segments().any(|s| s.id() == b"PID"));
    assert!(
        order::context(
            &built,
            b"app-order",
            Some((SampleField::Placer1, b"overwrite")),
            Options::default()
        )
        .is_err()
    );
    assert!(
        order::context(
            &built,
            b"app-order-identifier-too-long",
            None,
            Options::default()
        )
        .is_err()
    );
    assert!(
        order::oml(
            &[request],
            Subject::Unspecified,
            header(Version::V251)?,
            Options::default()
        )
        .is_err()
    );
    Ok(())
}
