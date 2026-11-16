//! Tests protected evidence queries.
use analyte_hl7::{ControlId, Message, Options, Response, query};

fn request(version: &str) -> Result<Message, analyte_hl7::Error> {
    Message::parse(format!("MSH|^~\\&|A|B|C|D|202611150900||QRY^Q02^QRY_Q02|q1|P|{version}\rQRD|20261115|D|D|tag|||5^LI|0001^^^^^^^LCL|ORD|TEST\rQRF|LAB\r").as_bytes(), Options::default())
}

#[test]
fn hl7_231_2_18_and_2_6_s5_10_qck_answers_deferred_query() -> Result<(), analyte_hl7::Error> {
    for version in ["2.3.1", "2.5.1", "2.6"] {
        let message = request(version)?;
        let request = query::Request::new(&message)?;
        assert_eq!(request.query_id(), b"tag");
        for (answer, expected) in [
            (query::CacheAnswer::Found, b"QAK||OK\r".as_slice()),
            (query::CacheAnswer::Empty, b"QAK||NF\r"),
            (query::CacheAnswer::Unavailable, b"QAK||AE\r"),
        ] {
            let response = query::acknowledgment(
                request,
                Response {
                    control_id: ControlId::new(b"qck1")?,
                    timestamp: b"202611150901",
                },
                answer,
                Options::default(),
            )?;
            assert!(response.raw().ends_with(expected));
            assert!(
                response
                    .raw()
                    .windows(9)
                    .any(|v| v == b"QCK^Q02|q" || v == b"QCK^Q02^Q")
            );
            if answer == query::CacheAnswer::Unavailable {
                assert!(response.raw().windows(10).any(|v| v == b"MSA|AE|q1\r"));
            }
        }
    }
    Ok(())
}

#[test]
fn hl7_231_2_18_dsr_echoes_query_and_ack_correlates_dsr() -> Result<(), analyte_hl7::Error> {
    let message = request("2.3.1")?;
    let request = query::Request::new(&message)?;
    let dsr = query::display(
        request,
        Response {
            control_id: ControlId::new(b"dsr1")?,
            timestamp: b"202611150902",
        },
        ControlId::new(b"qck1")?,
        &[b"Synthetic line"],
        Options::default(),
    )?;
    assert!(dsr.raw().ends_with(b"QRF|LAB\rDSP|1||Synthetic line\r"));
    let ack = Message::parse(
        b"MSH|^~\\&|A|B|C|D|20261115||ACK^Q03^ACK|a1|P|2.3.1\rMSA|AA|dsr1\r",
        Options::default(),
    )?;
    assert!(query::DisplayAck::new(&ack, ControlId::new(b"dsr1")?).is_ok());
    assert!(query::DisplayAck::new(&ack, ControlId::new(b"q1")?).is_err());
    Ok(())
}

proptest::proptest! {
    #[test]
    fn hl7_231_2_24_4_query_id_is_preserved(id in "[A-Za-z0-9]{1,10}") {
        let bytes = format!("MSH|^~\\&|A|B|C|D|20261115||QRY^Q02^QRY_Q02|q1|P|2.3.1\rQRD|20261115|D|D|{id}|||5^LI|0001^^^^^^^LCL|ORD|TEST\r");
        let parsed = Message::parse(bytes.as_bytes(), Options::default());
        proptest::prop_assert!(parsed.is_ok());
        if let Ok(message) = parsed {
            let request = query::Request::new(&message);
            proptest::prop_assert!(request.is_ok());
            if let Ok(request) = request {
                proptest::prop_assert_eq!(request.query_id(), id.as_bytes());
            }
        }
    }
}

#[test]
fn hl7_231_2_24_4_rejects_vendor_quantity_and_missing_department() -> Result<(), analyte_hl7::Error>
{
    for definition in [
        b"QRD|20261115|D|D|tag|||RD|0001^^^^^^^LCL|ORD|TEST\r".as_slice(),
        b"QRD|20261115|D|D|tag|||5^LI|0001^^^^^^^LCL|ORD\r",
    ] {
        let mut bytes = b"MSH|^~\\&|A|B|C|D|20261115||QRY^Q02^QRY_Q02|q1|P|2.3.1\r".to_vec();
        bytes.extend_from_slice(definition);
        let message = Message::parse(&bytes, Options::default())?;
        assert!(query::Request::new(&message).is_err());
    }
    Ok(())
}

#[test]
fn hl7_231_2_18_dsr_requires_data_and_distinct_response_ids() -> Result<(), analyte_hl7::Error> {
    let message = request("2.3.1")?;
    let request = query::Request::new(&message)?;
    let response = Response {
        control_id: ControlId::new(b"dsr1")?,
        timestamp: b"202611150902",
    };
    for lines in [&[][..], &[b"injected|field".as_slice()][..]] {
        assert!(
            query::display(
                request,
                response,
                ControlId::new(b"qck1")?,
                lines,
                Options::default()
            )
            .is_err()
        );
    }
    assert!(
        query::display(
            request,
            response,
            ControlId::new(b"dsr1")?,
            &[b"line"],
            Options::default()
        )
        .is_err()
    );
    assert!(
        query::acknowledgment(
            request,
            Response {
                control_id: ControlId::new(b"q1")?,
                timestamp: b"202611150902"
            },
            query::CacheAnswer::Found,
            Options::default()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn hl7_231_2_24_22_and_251_5_5_2_unavailable_is_ae_never_nf() -> Result<(), analyte_hl7::Error> {
    for version in ["2.3.1", "2.5.1"] {
        let message = request(version)?;
        let reply = query::acknowledgment(
            query::Request::new(&message)?,
            Response {
                control_id: ControlId::new(b"unavailable")?,
                timestamp: b"202611150902",
            },
            query::CacheAnswer::Unavailable,
            Options::default(),
        )?;
        let segments: Vec<_> = reply.segments().collect();
        let msa = segments.iter().find(|s| s.id() == b"MSA").map(|s| s.raw());
        let qak = segments.iter().find(|s| s.id() == b"QAK").map(|s| s.raw());
        assert_eq!(msa, Some(b"MSA|AE|q1".as_slice()));
        assert_eq!(qak, Some(b"QAK||AE".as_slice()));
        assert!(!reply.raw().windows(2).any(|v| v == b"NF"));
    }
    Ok(())
}

#[test]
fn hl7_251_5_10_legacy_query_scope_rejects_modern_and_undeclared_events()
-> Result<(), analyte_hl7::Error> {
    let source = request("2.5.1")?;
    let text = std::str::from_utf8(source.raw()).map_err(|_| analyte_hl7::Error::Encoding)?;
    for kind in [
        "QRY^Q01",
        "QBP^Q11^QBP_Q11",
        "QSB^Q16^QSB_Q16",
        "QVR^Q17^QVR_Q17",
        "QCN^J01^QCN_J01",
        "QSX^J02^QCN_J01",
    ] {
        let bytes = text.replace("QRY^Q02^QRY_Q02", kind);
        let message = Message::parse(bytes.as_bytes(), Options::default())?;
        assert!(matches!(
            query::Request::new(&message),
            Err(analyte_hl7::Error::Control {
                segment: "MSH",
                field: 9
            })
        ));
        assert_eq!(message.raw(), bytes.as_bytes());
    }
    Ok(())
}

#[test]
fn hl7_231_2_19_2_20_undeclared_enhanced_query_forms_are_rejected() -> Result<(), analyte_hl7::Error>
{
    let source = request("2.3.1")?;
    let text = std::str::from_utf8(source.raw()).map_err(|_| analyte_hl7::Error::Encoding)?;
    for kind in ["EQQ^Q04", "VQQ^Q07", "SPQ^Q08", "ERP^R09"] {
        let bytes = text.replace("QRY^Q02^QRY_Q02", kind);
        let message = Message::parse(bytes.as_bytes(), Options::default())?;
        assert!(matches!(
            query::Request::new(&message),
            Err(analyte_hl7::Error::Control {
                segment: "MSH",
                field: 9
            })
        ));
    }
    Ok(())
}

#[test]
fn hl7_231_2_24_8_2_20_3_and_251_5_6_3_continuation_and_erq_are_located()
-> Result<(), analyte_hl7::Error> {
    for edition in ["2.3.1", "2.5.1"] {
        let source = request(edition)?;
        for (segment, extra) in [
            ("DSC", b"DSC|pointer\r".as_slice()),
            ("ERQ", b"ERQ|tag|event\r"),
            ("QRD", b"QRD|extra\r"),
            ("QRF", b"QRF|extra\r"),
        ] {
            let bytes = [source.raw(), extra].concat();
            let message = Message::parse(&bytes, Options::default())?;
            assert!(
                matches!(query::Request::new(&message),Err(analyte_hl7::Error::Control {segment:s,field:0}) if s==segment)
            );
            assert_eq!(message.raw(), bytes);
        }
    }
    Ok(())
}
