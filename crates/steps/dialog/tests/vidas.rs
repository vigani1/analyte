//! Tests vidas bci a 3 checksum precedes etx; vidas bci a 3 suffixes and literal boundary.
use analyte_dialog::vidas::{Action, Dialog, Event, Policy, Timer};
use analyte_framing::vidas::{Checksum, Config, Decoder, Mode};
use analyte_vidas::syntax::{Message, SampleLimit};
use std::time::Duration;

fn config(
    mode: Mode,
    suffix: &[u8],
    include_suffix: bool,
) -> Result<Config, Box<dyn std::error::Error>> {
    Ok(Config::new(
        mode,
        3,
        1920,
        suffix,
        Checksum {
            include_stx: false,
            include_suffix,
            hex: analyte_framing::vidas::HexOptions {
                uppercase: false,
                accept_either_case: true,
            },
        },
    )?)
}

fn policy() -> Policy {
    Policy {
        acknowledgment: Duration::from_secs(1),
        interbyte: Duration::from_secs(2),
        session: Duration::from_secs(3),
        commit: Duration::from_secs(1),
        retries: 0,
        backoff: Duration::ZERO,
    }
}

#[test]
fn vidas_bci_a_3_checksum_precedes_etx() -> Result<(), Box<dyn std::error::Error>> {
    let c = config(Mode::Alternate, &[], false)?;
    // Independent arithmetic: RS + 'A' + GS = 124 = 0x7c.
    let raw = c.encode(b"A")?;
    assert_eq!(raw, b"\x02\x1eA\x1d7c\x03");
    let mut decoder = Decoder::new(c);
    for byte in b"\x02\x1eA\x1d7c" {
        assert!(decoder.push(*byte)?.is_none());
    }
    let packet = decoder.push(3)?.ok_or("missing packet")?;
    assert_eq!(packet.raw, raw);
    assert_eq!(packet.body, b"A");
    Ok(())
}

#[test]
fn vidas_bci_a_3_suffixes_and_literal_boundary() -> Result<(), Box<dyn std::error::Error>> {
    for mode in [Mode::Literal, Mode::Alternate] {
        for suffix in [b"".as_slice(), b"\r", b"\n", b"\r\n"] {
            for include in [false, true] {
                let c = config(mode, suffix, include)?;
                let raw = c.encode(b"ci0012|")?;
                let mut decoder = Decoder::new(c);
                let mut packet = None;
                for byte in &raw {
                    packet = decoder.push(*byte)?;
                }
                let packet = packet.ok_or("missing boundary")?;
                assert_eq!(packet.body, b"ci0012|");
                assert_eq!(packet.raw, raw);
            }
        }
    }
    Ok(())
}

#[test]
fn vidas_bci_a_3_bad_checksum_and_bounds_stop() -> Result<(), Box<dyn std::error::Error>> {
    let c = config(Mode::Literal, &[], false)?;
    let mut d = Decoder::new(c.clone());
    for byte in b"\x02\x1eA\x1d00".iter().take(5) {
        assert!(d.push(*byte)?.is_none());
    }
    assert!(d.push(b'0').is_err());
    assert!(d.push(2).is_err());
    assert!(c.encode(&vec![b'A'; 1921]).is_err());
    Ok(())
}

#[test]
fn vidas_bci_a_3_adr0003_store_gate_and_incomplete_transfer()
-> Result<(), Box<dyn std::error::Error>> {
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, &[], false)?;
        let mut d = Decoder::new(c.clone());
        let mut packet = None;
        for byte in c.encode(b"synthetic")? {
            packet = d.push(byte)?;
        }
        let mut dialog = Dialog::upload(c, policy())?;
        assert!(
            dialog
                .on(Event::Control(5))
                .contains(&Action::Send(vec![6]))
        );
        let actions = dialog.on(Event::Packet(packet.ok_or("packet")?));
        assert!(actions.iter().any(|a| matches!(a, Action::Store(_))));
        assert!(!actions.iter().any(|a| matches!(a, Action::Send(_))));
        assert!(dialog.on(Event::Stored).contains(&Action::Send(vec![6])));
        assert!(dialog.on(Event::Disconnected).contains(&Action::Park));
    }
    Ok(())
}

#[test]
fn vidas_bci_a_5_download_never_resends_after_lost_ack() -> Result<(), Box<dyn std::error::Error>> {
    let request = Message::parse("mtmpr|piX|si|ci0001|rtHCG|qd1|", SampleLimit::Twelve)?;
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, &[], false)?;
        let bytes = c.encode(request.text().as_bytes())?;
        let mut dialog = Dialog::download(c, policy(), &request)?;
        assert!(dialog.start().contains(&Action::Send(vec![5])));
        assert!(dialog.on(Event::Control(6)).contains(&Action::Send(bytes)));
        let timeout = dialog.on(Event::Expired(Timer::Acknowledgment));
        assert!(timeout.contains(&Action::Park));
        assert!(!timeout.iter().any(|a| matches!(a, Action::Send(_))));
        assert!(dialog.on(Event::Control(6)).is_empty());
    }
    Ok(())
}

proptest::proptest! {
    #[test]
    fn vidas_bci_a_3_arbitrary_chunk_splits_roundtrip(body in "[ -~]{1,500}") {
        let c = config(Mode::Alternate, &[], false).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let raw = c.encode(body.as_bytes()).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let mut decoder = Decoder::new(c);
        let mut output = None;
        for byte in &raw { output = decoder.push(*byte).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?; }
        proptest::prop_assert_eq!(output.map(|p| (p.body, p.raw)), Some((body.into_bytes(), raw)));
    }

    #[test]
    fn vidas_bci_a_4_parser_never_panics(text in ".{0,2100}") {
        if let Ok(message) = Message::parse(&text, SampleLimit::Seventeen) {
            proptest::prop_assert_eq!(message.text(), text);
        }
    }
}

#[test]
fn vidas_bci_a_3_storage_failure_and_successful_sessions() -> Result<(), Box<dyn std::error::Error>>
{
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, &[], false)?;
        let mut decoder = Decoder::new(c.clone());
        let mut packet = None;
        for byte in c.encode(b"synthetic")? {
            packet = decoder.push(byte)?;
        }
        let packet = packet.ok_or("packet")?;
        let mut failed = Dialog::upload(c.clone(), policy())?;
        let _ = failed.on(Event::Control(5));
        let _ = failed.on(Event::Packet(packet.clone()));
        let actions = failed.on(Event::StorageFailed);
        assert!(actions.contains(&Action::Park));
        assert!(!actions.iter().any(|a| matches!(a, Action::Send(_))));
        assert!(failed.on(Event::Stored).is_empty());
        let mut upload = Dialog::upload(c.clone(), policy())?;
        let _ = upload.on(Event::Control(5));
        let _ = upload.on(Event::Packet(packet));
        let _ = upload.on(Event::Stored);
        if mode == Mode::Literal {
            assert!(!upload.on(Event::Control(3)).contains(&Action::Complete));
        }
        assert!(upload.on(Event::Control(4)).contains(&Action::Complete));
        let request = Message::parse("mtmpr|piX|si|ci0001|rtHCG|", SampleLimit::Twelve)?;
        let mut download = Dialog::download(c, policy(), &request)?;
        let _ = download.on(Event::Control(6));
        let actions = download.on(Event::Control(6));
        assert_eq!(
            actions.contains(&Action::Send(vec![3])),
            mode == Mode::Literal
        );
        assert!(actions.contains(&Action::Send(vec![4])));
        assert!(actions.contains(&Action::Complete));
    }
    Ok(())
}
