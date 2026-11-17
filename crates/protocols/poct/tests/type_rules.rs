//! Tests source-specific POCT field and type constraints with synthetic messages.
use analyte_fixtures::ConfigEdit as _;
use analyte_fixtures::poct as peer_messages;
use analyte_poct::{Message, MessageError};
use peer_messages::{hello, limits, observation, status};

// POCT01-A2 Appendix B 12.3 p.189: numeric fixed metadata differs from PQ.
#[test]
fn poct01_b_12_3_p189_int_real_fixed_value_metadata() -> Result<(), MessageError> {
    for (raw, field, wrong) in [
        (status("s", "2"), "DST.new_observations_qty", "INT"),
        (hello("h"), "DCP.application_timeout", "REAL"),
    ] {
        let valid = raw.rewrite(&format!("<{field} V="), &format!("<{field} V-T=\"ST\" V="));
        Message::parse(valid.as_bytes(), limits())?;
        assert!(
            Message::parse(
                valid
                    .rewrite("V-T=\"ST\"", &format!("V-T=\"{wrong}\""))
                    .as_bytes(),
                limits()
            )
            .is_err()
        );
    }
    let pq = observation("o", true, "<OBS.value V=\"1.20\" U=\"mg/L\" V-T=\"REAL\"/>");
    Message::parse(pq.as_bytes(), limits())?;
    assert!(
        Message::parse(
            pq.rewrite("V-T=\"REAL\"", "V-T=\"ST\"").as_bytes(),
            limits()
        )
        .is_err()
    );
    Ok(())
}

// POCT01-A2 B 5.5 p.95 and clarification p.286; DML has no legacy-ID exception.
#[test]
fn poct01_b_5_5_dml_device_id_is_eui64() -> Result<(), MessageError> {
    for id in ["a1-b2-c3-d4-e5-f6-07-08", "00-00-00-00-00-00-00-00"] {
        let raw = hello("h").rewrite("A1-B2-C3-D4-E5-F6-07-08", id);
        Message::parse(raw.as_bytes(), limits())?;
    }
    for id in [
        "legacy",
        "A1-B2-C3-D4-E5-F6-07",
        "A1-B2-C3-D4-E5-F6-07-G8",
        "A1B2C3D4E5F60708",
    ] {
        assert!(
            Message::parse(
                hello("h").rewrite("A1-B2-C3-D4-E5-F6-07-08", id).as_bytes(),
                limits()
            )
            .is_err()
        );
    }
    Ok(())
}

// POCT01-A2 B 5.2 p.92.
#[test]
fn poct01_b_5_2_access_point_id_and_nonnegative_port() -> Result<(), MessageError> {
    for (id, port, valid) in [
        ("01-02-03-04-05-06-07-08", "0", true),
        ("01-02-03-04-05-06-07-08", "23", true),
        ("01-02-03-04-05-06-07-08", "-1", false),
        ("legacy", "1", false),
    ] {
        let raw = hello("h").rewrite(
            "</HEL.R01>",
            &format!("<AP><AP.ap_id V=\"{id}\"/><AP.port_nbr V=\"{port}\"/></AP></HEL.R01>"),
        );
        assert_eq!(Message::parse(raw.as_bytes(), limits()).is_ok(), valid);
    }
    Ok(())
}

// POCT01-A2 B 5.9 p.97.
#[test]
fn poct01_b_5_9_counts_are_nonnegative() {
    for count in ["0", "+2", "-0", "999999999999999999999"] {
        assert!(Message::parse(status("s", count).as_bytes(), limits()).is_ok());
    }
    for field in ["DST.new_observations_qty", "DST.new_events_qty"] {
        let raw = if field == "DST.new_observations_qty" {
            status("s", "-1")
        } else {
            status("s", "0").rewrite(
                "<DST.condition_cd",
                "<DST.new_events_qty V=\"-1\"/><DST.condition_cd",
            )
        };
        assert!(Message::parse(raw.as_bytes(), limits()).is_err());
    }
}

// POCT01-A2 B 5.13 p.99: count decoded characters rather than encoded XML bytes.
#[test]
fn poct01_b_5_13_encoding_declaration_has_four_characters() {
    for (value, valid) in [("^~\\&amp;", true), ("abc", false), ("abcde", false)] {
        let raw = hello("h").rewrite(
            "</HDR>",
            &format!("<HDR.encoding_chars V=\"{value}\"/></HDR>"),
        );
        assert_eq!(Message::parse(raw.as_bytes(), limits()).is_ok(), valid);
    }
}

// POCT01-A2 B 5.4, 5.19 and clarification p.285 supersede the old element DTD lot type.
#[test]
fn poct01_b_5_4_clarification_p285_material_lots_are_cs() -> Result<(), MessageError> {
    for (field, raw) in [
        ("CTC.lot_number", observation("o", false, "<OBS.value V=\"1\" U=\"mg/L\"/>").rewrite("<CTC.name V=\"synthetic-control\"/>", "<CTC.name V=\"synthetic-control\"/><CTC.lot_number V=\"lot-01\" T=\"CS\"/>")),
        ("RGT.lot_number", observation("o", true, "<OBS.value V=\"1\" U=\"mg/L\"/>").rewrite("</PT>", "</PT><RGT><RGT.name V=\"reagent\"/><RGT.lot_number V=\"lot-01\" T=\"CS\"/><RGT.expiration_date V=\"2027-01-01\"/></RGT>")),
    ] {
        Message::parse(raw.as_bytes(), limits())?;
        assert!(Message::parse(raw.rewrite("T=\"CS\"", "T=\"CV\"").as_bytes(), limits()).is_err());
        let qualified = raw.rewrite(&format!("<{field} V="), &format!("<{field} SN=\"local\" V="));
        assert!(Message::parse(qualified.as_bytes(), limits()).is_err());
    }
    Ok(())
}

// POCT01-A2 B 8.4 and default tables 14/21/25/28/45/54/57.
#[test]
fn poct01_b_8_4_default_codes_and_qualified_extensions() -> Result<(), MessageError> {
    for (field, valid, raw) in [
        ("DST.condition_cd", "R", status("s", "0")),
        ("DSC.topics_supported_cd", "D_EV", hello("h")),
        (
            "EOT.topic_cd",
            "OBS",
            peer_messages::message("EOT.R01", "e", "<EOT><EOT.topic_cd V=\"OBS\"/></EOT>"),
        ),
        (
            "REQ.request_cd",
            "ROBS",
            peer_messages::message("REQ.R01", "r", "<REQ><REQ.request_cd V=\"ROBS\"/></REQ>"),
        ),
        (
            "TRM.reason_cd",
            "NRM",
            peer_messages::message("END.R01", "t", "<TRM><TRM.reason_cd V=\"NRM\"/></TRM>"),
        ),
        (
            "ACK.error_detail_cd",
            "202",
            peer_messages::message(
                "ACK.R01",
                "a",
                "<ACK><ACK.type_cd V=\"AE\"/><ACK.ack_control_id V=\"x\"/><ACK.error_detail_cd V=\"202\"/></ACK>",
            ),
        ),
    ] {
        Message::parse(raw.as_bytes(), limits())?;
        let unknown = raw.rewrite(
            &format!("<{field} V=\"{valid}\""),
            &format!("<{field} V=\"local-code\""),
        );
        assert!(Message::parse(unknown.as_bytes(), limits()).is_err());
        for (system, accepted) in [("local", true), ("POCT01", false), ("", false)] {
            let qualified = unknown.rewrite(
                &format!("<{field} V="),
                &format!("<{field} SN=\"{system}\" V="),
            );
            assert_eq!(
                Message::parse(qualified.as_bytes(), limits()).is_ok(),
                accepted
            );
        }
    }
    Ok(())
}

// POCT01-A2 B 8.13: display spelling does not make the same coded member distinct.
#[test]
fn poct01_b_8_13_advertised_sets_have_unique_members() -> Result<(), MessageError> {
    for field in ["DSC.topics_supported_cd", "DSC.directives_supported_cd"] {
        let raw = hello("h").rewrite("<DSC.max_message_sz", &format!("<{field} V=\"custom\" SN=\"local\"/><{field} V=\"custom\" SN=\"local\" DN=\"label\"/><DSC.max_message_sz"));
        assert!(Message::parse(raw.as_bytes(), limits()).is_err());
        let distinct = raw.rewrite("SN=\"local\" DN=", "SN=\"other\" DN=");
        Message::parse(distinct.as_bytes(), limits())?;
    }
    Ok(())
}

// POCT01-A2 B 12.3 p.196: translations use CD and retain modifier children.
#[test]
fn poct01_b_12_3_p196_translation_uses_cd() -> Result<(), MessageError> {
    let raw = observation("o", true, "<OBS.value V=\"1.20\" U=\"mg/L\"/>").rewrite("<TRANSLTN V=\"alternate\" SN=\"alternate-system\"/>", "<TRANSLTN T=\"CD\" V=\"alternate\" SN=\"alternate-system\"><MODIFIER T=\"CD\" V=\"modifier\" SN=\"local\"/></TRANSLTN>");
    Message::parse(raw.as_bytes(), limits())?;
    assert!(Message::parse(raw.rewrite("T=\"CD\"", "T=\"PQ\"").as_bytes(), limits()).is_err());
    let reordered = raw.rewrite("</TRANSLTN>", "<ORIGTXT>original</ORIGTXT></TRANSLTN>");
    assert!(Message::parse(reordered.as_bytes(), limits()).is_err());
    Ok(())
}

// POCT01-A2 B 12.3 pp.184-188: restricted text types use ordered singleton metadata.
#[test]
fn poct01_b_12_3_p188_organization_child_model() -> Result<(), MessageError> {
    for (field, raw) in [
        ("DEV.manufacturer_name", hello("h").rewrite("<DCP>", "<DEV.manufacturer_name V=\"Synthetic\"/><DCP>")),
        ("GIV", observation("o", true, "<OBS.value V=\"1\" U=\"mg/L\"/>").rewrite("<PT.patient_id V=\"000xyz\"/>", "<PT.patient_id V=\"000xyz\"/><PT.name V=\"Synthetic\"><GIV V=\"Synthetic\"/></PT.name>")),
        ("REF", hello("h").rewrite("</DCP>", "<DCP.vendor_specific><REF V=\"https://invalid.example/inert\"/></DCP.vendor_specific></DCP>")),
    ] {
        Message::parse(raw.as_bytes(), limits())?;
        let opening = format!("<{field} V=");
        let begin = raw.find(&opening).ok_or(MessageError::Structure)?;
        let end = raw.get(begin..).and_then(|s| s.find("/>")).and_then(|n| begin.checked_add(n)).ok_or(MessageError::Structure)?;
        let prefix = raw.get(..end).ok_or(MessageError::Structure)?;
        let suffix = raw.get(end+2..).ok_or(MessageError::Structure)?;
        for (children, valid) in [("<NOTE V=\"note\"/><CONFID V=\"confidence\"/>",true), ("mixed-text",false), ("<NOTE V=\"one\"/><NOTE V=\"two\"/>",false), ("<CONFID V=\"confidence\"/><NOTE V=\"note\"/>",false)] {
            let modified = format!("{prefix}>{children}</{field}>{suffix}");
            assert_eq!(Message::parse(modified.as_bytes(), limits()).is_ok(), valid);
        }
    }
    Ok(())
}

// POCT01-A2 B 12.3 pp.195-196: effective thumbnail properties come from its parent.
#[test]
fn poct01_b_12_3_p195_thumbnail_inherits_encapsulation() -> Result<(), MessageError> {
    let raw = hello("h").rewrite("</DCP>", "<DCP.vendor_specific ENC=\"B64\" COMPN=\"GZ\" MT=\"image/png\"><THUMBNAIL>synthetic</THUMBNAIL><THUMBNAIL ENC=\"TXT\" MT=\"text/plain\">override</THUMBNAIL></DCP.vendor_specific></DCP>");
    let parsed = Message::parse(raw.as_bytes(), limits())?;
    let thumbnails = parsed
        .document()
        .elements()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name() == "THUMBNAIL")
        .collect::<Vec<_>>();
    assert_eq!(thumbnails.len(), 2);
    for (n, (i, _)) in thumbnails.iter().enumerate() {
        let Some(analyte_poct::Value::Encapsulated(data)) = parsed.value(*i) else {
            return Err(MessageError::Structure);
        };
        assert_eq!(
            data.encoding,
            if n == 0 {
                analyte_poct::Encoding::Base64
            } else {
                analyte_poct::Encoding::Text
            }
        );
        assert_eq!(data.compression.as_deref(), Some("GZ"));
        assert_eq!(
            data.media_type,
            if n == 0 { "image/png" } else { "text/plain" }
        );
    }
    Ok(())
}

// POCT01-A2 B 12.3 p.181: trailing text after a child is not initial ED content.
#[test]
fn poct01_b_12_3_p181_encapsulation_uses_first_text_node() -> Result<(), MessageError> {
    let raw = hello("h").rewrite("</DCP>", "<DCP.vendor_specific>first&amp;text<REF V=\"https://invalid.example/inert\"/>later<NOTE V=\"note\"/>last</DCP.vendor_specific></DCP>");
    let parsed = Message::parse(raw.as_bytes(), limits())?;
    let (i, e) = parsed
        .document()
        .elements()
        .iter()
        .enumerate()
        .find(|(_, e)| e.name() == "DCP.vendor_specific")
        .ok_or(MessageError::Structure)?;
    assert_eq!(e.text(), "first&textlaterlast");
    let Some(analyte_poct::Value::Encapsulated(data)) = parsed.value(i) else {
        return Err(MessageError::Structure);
    };
    assert_eq!(data.text, "first&text");
    assert_eq!(parsed.document().raw(), raw.as_bytes());
    let commented = raw.rewrite("first&amp;text", "<!--not a text node-->after-comment");
    let parsed = Message::parse(commented.as_bytes(), limits())?;
    let (index, _) = parsed
        .document()
        .elements()
        .iter()
        .enumerate()
        .find(|(_, e)| e.name() == "DCP.vendor_specific")
        .ok_or(MessageError::Structure)?;
    let Some(analyte_poct::Value::Encapsulated(data)) = parsed.value(index) else {
        return Err(MessageError::Structure);
    };
    assert_eq!(data.text, "");
    Ok(())
}

// POCT01-A2 B 8.9 p.135, 12.3 p.189 and 6.5.2.1 p.116.
#[test]
fn poct01_b_8_9_p135_decimal_integral_with_trailing_point() -> Result<(), MessageError> {
    for value in ["10.", "+5.", "-0.", "0.50"] {
        let real = hello("h").rewrite(
            "DCP.application_timeout V=\"12.5\"",
            &format!("DCP.application_timeout V=\"{value}\""),
        );
        Message::parse(real.as_bytes(), limits())?;
        let pq = observation("o", true, &format!("<OBS.value V=\"{value}\" U=\"mg/L\"/>"));
        let parsed = Message::parse(pq.as_bytes(), limits())?;
        assert_eq!(parsed.document().raw(), pq.as_bytes());
    }
    for value in [".", "+.", "-", "+", "1..", "1e2", "1,2"] {
        let real = hello("h").rewrite(
            "DCP.application_timeout V=\"12.5\"",
            &format!("DCP.application_timeout V=\"{value}\""),
        );
        assert!(Message::parse(real.as_bytes(), limits()).is_err());
        assert!(
            Message::parse(
                observation("o", true, &format!("<OBS.value V=\"{value}\" U=\"mg/L\"/>"))
                    .as_bytes(),
                limits()
            )
            .is_err()
        );
    }
    assert!(Message::parse(status("s", "1.").as_bytes(), limits()).is_err());
    Ok(())
}
