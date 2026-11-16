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
