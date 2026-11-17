//! Tests legacy edition replies across the store-before-acknowledgment boundary.
use analyte_dialog::hl7::{Action, Config, Dialog, Event};
use analyte_hl7::{ControlId, Message, Msh, Options, Response};
use std::time::Duration;

#[test]
fn hl7_2_5_1_s2_8_and_2_6_2_8_2_2_9_1_s2_9_2_original_ack_requires_store_and_interpretation()
-> Result<(), Box<dyn std::error::Error>> {
    for version in ["2.3", "2.4", "2.5", "2.6", "2.8.2", "2.9.1"] {
        let mut dialog = Dialog::new(
            Config::new(Options::default(), Duration::from_secs(5), 1)?,
            42,
        );
        let raw = format!("MSH|^~\\&|S|F|R|F|20261116||ORU^R01^ORU_R01|000AbC|P|{version}\r");
        let frame = format!("\x0b{raw}\x1c\r");
        let mut actions = Vec::new();
        for byte in frame.bytes() {
            actions.extend(dialog.step(Duration::ZERO, Event::Byte(byte))?);
        }
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Write(_)))
        );
        let (receipt, stored) = actions
            .iter()
            .find_map(|action| {
                if let Action::Store(receipt, bytes) = action {
                    Some((*receipt, bytes))
                } else {
                    None
                }
            })
            .ok_or("missing durable-store request")?;
        assert_eq!(stored.as_slice(), frame.as_bytes());
        let early = dialog.step(Duration::ZERO, Event::Interpreted(receipt))?;
        assert!(
            early
                .iter()
                .any(|action| matches!(action, Action::Refused(_)))
        );
        assert!(
            !early
                .iter()
                .any(|action| matches!(action, Action::Write(_)))
        );
        let committed = dialog.step(Duration::ZERO, Event::Stored(receipt))?;
        assert!(
            committed
                .iter()
                .any(|action| matches!(action, Action::Interpret(_, _)))
        );
        assert!(
            !committed
                .iter()
                .any(|action| matches!(action, Action::Write(_)))
        );
        dialog.step(Duration::ZERO, Event::Interpreted(receipt))?;
        let reply = dialog.step(
            Duration::ZERO,
            Event::Respond(
                receipt,
                Response {
                    control_id: ControlId::new(b"reply001")?,
                    timestamp: b"202611161100",
                },
            ),
        )?;
        let wire = reply
            .iter()
            .find_map(|action| {
                if let Action::Write(analyte_dialog::Write { bytes: wire, .. }) = action {
                    Some(wire)
                } else {
                    None
                }
            })
            .ok_or("missing acknowledgment")?;
        let payload = wire
            .strip_prefix(b"\x0b")
            .and_then(|wire| wire.strip_suffix(b"\x1c\r"))
            .ok_or("invalid MLLP reply")?;
        let message = Message::parse(payload, Options::default())?;
        assert_eq!(Msh::new(&message)?.version().as_str(), version);
        assert!(message.raw().ends_with(b"MSA|AA|000AbC\r"));
    }
    Ok(())
}
