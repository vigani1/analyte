//! Tests source-specific POCT field and type constraints with synthetic messages.
mod peer_messages;
use analyte_poct::{Message, MessageError};
use peer_messages::{hello, limits, observation, status};

// POCT01-A2 Appendix B 12.3 p.189: numeric fixed metadata differs from PQ.
#[test]
fn poct01_b_12_3_p189_int_real_fixed_value_metadata() -> Result<(), MessageError> {
    for (raw, field, wrong) in [
        (status("s", "2"), "DST.new_observations_qty", "INT"),
        (hello("h"), "DCP.application_timeout", "REAL"),
    ] {
        let valid = raw.replace(&format!("<{field} V="), &format!("<{field} V-T=\"ST\" V="));
        Message::parse(valid.as_bytes(), limits())?;
        assert!(
            Message::parse(
                valid
                    .replace("V-T=\"ST\"", &format!("V-T=\"{wrong}\""))
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
            pq.replace("V-T=\"REAL\"", "V-T=\"ST\"").as_bytes(),
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
        let raw = hello("h").replace("A1-B2-C3-D4-E5-F6-07-08", id);
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
                hello("h").replace("A1-B2-C3-D4-E5-F6-07-08", id).as_bytes(),
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
        let raw = hello("h").replace(
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
            status("s", "0").replace(
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
        let raw = hello("h").replace(
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
        ("CTC.lot_number", observation("o", false, "<OBS.value V=\"1\" U=\"mg/L\"/>").replace("<CTC.name V=\"synthetic-control\"/>", "<CTC.name V=\"synthetic-control\"/><CTC.lot_number V=\"lot-01\" T=\"CS\"/>")),
        ("RGT.lot_number", observation("o", true, "<OBS.value V=\"1\" U=\"mg/L\"/>").replace("</PT>", "</PT><RGT><RGT.name V=\"reagent\"/><RGT.lot_number V=\"lot-01\" T=\"CS\"/><RGT.expiration_date V=\"2027-01-01\"/></RGT>")),
    ] {
        Message::parse(raw.as_bytes(), limits())?;
        assert!(Message::parse(raw.replace("T=\"CS\"", "T=\"CV\"").as_bytes(), limits()).is_err());
        let qualified = raw.replace(&format!("<{field} V="), &format!("<{field} SN=\"local\" V="));
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
        let unknown = raw.replace(
            &format!("<{field} V=\"{valid}\""),
            &format!("<{field} V=\"local-code\""),
        );
        assert!(Message::parse(unknown.as_bytes(), limits()).is_err());
        for (system, accepted) in [("local", true), ("POCT01", false), ("", false)] {
            let qualified = unknown.replace(
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
        let raw = hello("h").replace("<DSC.max_message_sz", &format!("<{field} V=\"custom\" SN=\"local\"/><{field} V=\"custom\" SN=\"local\" DN=\"label\"/><DSC.max_message_sz"));
        assert!(Message::parse(raw.as_bytes(), limits()).is_err());
        let distinct = raw.replace("SN=\"local\" DN=", "SN=\"other\" DN=");
        Message::parse(distinct.as_bytes(), limits())?;
    }
    Ok(())
}
