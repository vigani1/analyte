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
