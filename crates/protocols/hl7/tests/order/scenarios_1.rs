//! Tests HL7 order messages and acknowledgments.
use super::*;

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
