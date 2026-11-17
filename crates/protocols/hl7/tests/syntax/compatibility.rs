//! Tests the bounded legacy receiving contract without upgrading source edition identities.
use analyte_hl7::*;

fn message(version: &str, time: &str, kind: &str, body: &str) -> Result<Message, Error> {
    Message::parse(
        format!("MSH|^~\\&|S|F|R|F|{time}||{kind}|000AbC|P|{version}\r{body}").as_bytes(),
        Options::default(),
    )
}

fn response() -> Result<Response<'static>, Error> {
    Ok(Response {
        control_id: ControlId::new(b"reply001")?,
        timestamp: b"202611161100",
    })
}

#[test]
fn hl7_2_5_1_s2_8_2_datatype_prefix_preserves_legacy_msh_identity() {
    for version in ["2.3", "2.4", "2.5"] {
        let input = message(version, "20261116", "ORU^R01^ORU_R01", "").unwrap();
        let msh = Msh::new(&input).unwrap();
        assert_eq!(msh.version().as_str(), version);
        assert_eq!(msh.control_id(), b"000AbC");
        assert_eq!(input.encode(), input.raw());
    }
}

#[test]
fn hl7_2_5_1_s2_15_9_7_required_since_2_4_not_retroactive_to_2_3() {
    for (version, required) in [("2.3", false), ("2.4", true), ("2.5", true)] {
        let input = message(version, "", "ORU^R01^ORU_R01", "").unwrap();
        if required {
            assert!(matches!(
                Msh::new(&input),
                Err(Error::Control {
                    segment: "MSH",
                    field: 7
                })
            ));
        } else {
            assert!(Msh::new(&input).is_ok());
        }
    }
}

#[test]
fn hl7_2_5_1_s2_a_44_msg_replaces_cm_as_of_2_5() {
    for version in ["2.3", "2.4"] {
        assert!(Msh::new(&message(version, "20261116", "ORU^R01", "").unwrap()).is_ok());
    }
    assert!(matches!(
        Msh::new(&message("2.5", "20261116", "ORU^R01", "").unwrap()),
        Err(Error::Control {
            segment: "MSH",
            field: 9
        })
    ));
}

#[test]
fn hl7_2_3_1_errata_s2_8_44_1_and_2_4_errata_s2_9_47_1_ts_precision() {
    for version in ["2.3", "2.4", "2.5"] {
        for time in [
            "2026",
            "202611",
            "20261116",
            "2026111610",
            "202611161015-0130",
            "20261116101527.1234+0000",
            "202611161015^L",
        ] {
            let input = message(version, time, "ORU^R01^ORU_R01", "").unwrap();
            let msh = Msh::new(&input).unwrap();
            assert_eq!(msh.segment().field(7).unwrap().raw(), time.as_bytes());
        }
        for time in [
            "202613",
            "202611161",
            "202611161015^S",
            "20261116101527.12345",
        ] {
            assert!(matches!(
                Msh::new(&message(version, time, "ORU^R01^ORU_R01", "").unwrap()),
                Err(Error::Control {
                    segment: "MSH",
                    field: 7
                })
            ));
        }
    }
}

#[test]
fn hl7_2_5_1_s2_8_2_and_s2_15_5_1_ack_edition_and_error_shapes() {
    for version in ["2.3", "2.4", "2.5"] {
        let input = message(version, "20261116", "ORU^R01^ORU_R01", "").unwrap();
        let ack = acknowledgment(
            &input,
            response().unwrap(),
            AckCode::Ae,
            Responsibility::Stored,
            Some(AckError::unlocated(APPLICATION_INTERNAL_ERROR, b"review")),
            Options::default(),
        )
        .unwrap()
        .unwrap();
        let msh = Msh::new(&ack).unwrap();
        assert_eq!(msh.version().as_str(), version);
        assert!(
            ack.raw()
                .windows(b"MSA|AE|000AbC\r".len())
                .any(|w| w == b"MSA|AE|000AbC\r")
        );
        let err = ack.segments().find(|s| s.id() == b"ERR").unwrap();
        let view = ErrView::new(err, msh.version()).unwrap();
        if version == "2.5" {
            assert!(matches!(view, ErrView::V251 { .. }));
            assert!(ack.raw().ends_with(b"ERR|||207|E|review\r"));
            assert_eq!(msh.segment().field(9).unwrap().raw(), b"ACK^R01^ACK");
        } else {
            assert!(matches!(view, ErrView::V231(_)));
            assert!(ack.raw().ends_with(b"ERR|^^^207\r"));
        }
        assert!(matches!(
            acknowledgment(
                &input,
                response().unwrap(),
                AckCode::Aa,
                Responsibility::NotStored,
                None,
                Options::default()
            ),
            Err(Error::NotStored)
        ));
    }
}

fn query(version: &str, who: &str) -> Result<Message, Error> {
    message(
        version,
        "20261116",
        "QRY^Q02^QRY_Q02",
        &format!("QRD|202611161015^L|D|D|tag001|||5^LI|{who}|ORD|TEST\rQRF|LAB\r"),
    )
}

#[test]
fn hl7_2_5_1_s2_8_and_s5_10_legacy_query_unavailable_is_not_empty() {
    for version in ["2.3", "2.4", "2.5"] {
        let input = query(version, "0001^^^^^^^LCL").unwrap();
        let request = query::Request::new(&input).unwrap();
        assert_eq!(request.query_id(), b"tag001");
        let reply = query::acknowledgment(
            request,
            response().unwrap(),
            query::CacheAnswer::Unavailable,
            Options::default(),
        )
        .unwrap();
        assert_eq!(Msh::new(&reply).unwrap().version().as_str(), version);
        assert!(reply.raw().ends_with(b"QAK||AE\r"));
        assert!(!reply.raw().windows(2).any(|w| w == b"NF"));
    }
}

#[test]
fn hl7_2_4_20020408_errata_xcn_17_unsettled_nested_range_is_refused_and_raw() {
    let who = format!("0001^^^^^^^LCL{}20261116&20261117", "^".repeat(9));
    let input = query("2.4", &who).unwrap();
    assert!(matches!(
        query::Request::new(&input),
        Err(Error::Control {
            segment: "QRD",
            field: 8
        })
    ));
    let qrd = input.segments().find(|s| s.id() == b"QRD").unwrap();
    assert_eq!(qrd.field(8).unwrap().raw(), who.as_bytes());
}

#[test]
fn hl7_2_5_1_s2_8_2_table_changes_do_not_establish_earlier_utf8_code() {
    for version in ["2.3", "2.4", "2.5"] {
        let raw = format!(
            "MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|id|P|{version}||||||UNICODE UTF-8\r"
        );
        assert_eq!(
            Message::parse(
                raw.as_bytes(),
                Options {
                    charset: Charset::Utf8,
                    allow_utf8_in_v231: true,
                    ..Options::default()
                }
            ),
            Err(Error::UnsupportedCharset)
        );
    }
}

#[test]
fn hl7_2_5_1_s2_8_1_2_unsettled_query_extensions_and_uri_codes_are_refused() {
    for version in ["2.3", "2.4", "2.5"] {
        let who = "0001^^^^^^^^NS&https://synthetic.invalid/id&URI";
        let input = query(version, who).unwrap();
        assert!(matches!(
            query::Request::new(&input),
            Err(Error::Control {
                segment: "QRD",
                field: 8
            })
        ));
        let input = message(
            version,
            "20261116",
            "QRY^Q02^QRY_Q02",
            "QRD|20261116|D|D|tag001|||5^LI|0001^^^^^^^LCL|ORD|TEST\rQRF|LAB|||||||||0.5\r",
        )
        .unwrap();
        assert!(matches!(
            query::Request::new(&input),
            Err(Error::Control {
                segment: "QRF",
                field: 10
            })
        ));
        assert!(input.raw().ends_with(b"QRF|LAB|||||||||0.5\r"));
    }
}
