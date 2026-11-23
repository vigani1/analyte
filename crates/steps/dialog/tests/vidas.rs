//! Tests VIDAS result uploads and work downloads.
use analyte_dialog::vidas::{Action, Config as DialogConfig, Dialog, Event, Timer};
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

fn policy() -> DialogConfig {
    DialogConfig {
        ack: Duration::from_secs(1),
        inter_byte: Duration::from_secs(2),
        session: Duration::from_secs(3),
        commit: Duration::from_secs(1),
        retries: 0,
        backoff: Duration::ZERO,
    }
}

// VIDAS BCI Link rev A §3; ADR 0003
#[test]
fn ack_waits_for_store_and_keeps_incomplete_transfer() -> Result<(), Box<dyn std::error::Error>> {
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
                .step(Duration::ZERO, Event::Control(5))
                .unwrap()
                .contains(&Action::Write(vec![6]))
        );
        let actions = dialog
            .step(Duration::ZERO, Event::Packet(packet.ok_or("packet")?))
            .unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Store(_))));
        assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
        assert!(
            dialog
                .step(Duration::ZERO, Event::Stored)
                .unwrap()
                .contains(&Action::Write(vec![6]))
        );
        assert!(
            dialog
                .step(Duration::ZERO, Event::Disconnected)
                .unwrap()
                .contains(&Action::Park)
        );
    }
    Ok(())
}

// VIDAS BCI Link rev A §5
#[test]
fn download_never_resends_after_lost_ack() -> Result<(), Box<dyn std::error::Error>> {
    let request = Message::parse("mtmpr|piX|si|ci0001|rtHCG|qd1|", SampleLimit::Twelve)?;
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, &[], false)?;
        let bytes = c.encode(request.text().as_bytes())?;
        let mut dialog = Dialog::download(c, policy(), &request)?;
        assert!(dialog.start().contains(&Action::Write(vec![5])));
        assert!(
            dialog
                .step(Duration::ZERO, Event::Control(6))
                .unwrap()
                .contains(&Action::Write(bytes))
        );
        let timeout = dialog
            .step(Duration::ZERO, Event::Expired(Timer::Acknowledgment))
            .unwrap();
        assert!(timeout.contains(&Action::Park));
        assert!(!timeout.iter().any(|a| matches!(a, Action::Write(_))));
        assert!(
            dialog
                .step(Duration::ZERO, Event::Control(6))
                .unwrap()
                .is_empty()
        );
    }
    Ok(())
}

// VIDAS BCI guide C pp. 3-2 and 3-4.
#[test]
fn alternate_acknowledges_each_complete_packet_in_one_session()
-> Result<(), Box<dyn std::error::Error>> {
    let c = config(Mode::Alternate, b"\r\n", true)?;
    let first = Message::parse("mtmpr|piX|si|ci0001|rtSYN|", SampleLimit::Twelve)?;
    let second = Message::parse("mtmpr|piX|si|ci0001|rtSUB|", SampleLimit::Twelve)?;
    let first_wire = c.encode(first.text().as_bytes())?;
    let second_wire = c.encode(second.text().as_bytes())?;
    let mut dialog = Dialog::download_batch(c.clone(), policy(), &[first, second])?;
    let writes = |actions: Vec<Action>| {
        actions
            .into_iter()
            .filter_map(|action| {
                if let Action::Write(bytes) = action {
                    Some(bytes)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(writes(dialog.start()), [c.control(5)]);
    let ack = Event::Control(6);
    assert_eq!(
        writes(dialog.step(Duration::ZERO, ack.clone())?),
        [first_wire]
    );
    assert_eq!(
        writes(dialog.step(Duration::from_millis(750), ack.clone())?),
        [second_wire]
    );
    assert_eq!(
        writes(dialog.step(Duration::from_millis(1500), ack)?),
        [c.control(4)]
    );
    Ok(())
}

proptest::proptest! {
    // VIDAS BCI Link rev A §3
    #[test]
    fn arbitrary_chunk_splits_roundtrip(body in "[ -~]{1,500}") {
        let c = config(Mode::Alternate, &[], false).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let raw = c.encode(body.as_bytes()).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let mut decoder = Decoder::new(c);
        let mut output = None;
        for byte in &raw { output = decoder.push(*byte).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?; }
        proptest::prop_assert_eq!(output.map(|p| (p.body, p.raw)), Some((body.into_bytes(), raw)));
    }

    // VIDAS BCI Link rev A §4
    #[test]
    fn parser_never_panics(text in ".{0,2100}") {
        if let Ok(message) = Message::parse(&text, SampleLimit::Seventeen) {
            proptest::prop_assert_eq!(message.text(), text);
        }
    }
}

// VIDAS BCI Link rev A §3
#[test]
fn store_failure_parks_and_good_sessions_close() -> Result<(), Box<dyn std::error::Error>> {
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, &[], false)?;
        let mut decoder = Decoder::new(c.clone());
        let mut packet = None;
        for byte in c.encode(b"synthetic")? {
            packet = decoder.push(byte)?;
        }
        let packet = packet.ok_or("packet")?;
        let mut failed = Dialog::upload(c.clone(), policy())?;
        let _ = failed.step(Duration::ZERO, Event::Control(5)).unwrap();
        let _ = failed
            .step(Duration::ZERO, Event::Packet(packet.clone()))
            .unwrap();
        let actions = failed.step(Duration::ZERO, Event::StoreFailed).unwrap();
        assert!(actions.contains(&Action::Park));
        assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
        assert!(
            failed
                .step(Duration::ZERO, Event::Stored)
                .unwrap()
                .is_empty()
        );
        let mut upload = Dialog::upload(c.clone(), policy())?;
        let _ = upload.step(Duration::ZERO, Event::Control(5)).unwrap();
        let _ = upload.step(Duration::ZERO, Event::Packet(packet)).unwrap();
        let _ = upload.step(Duration::ZERO, Event::Stored).unwrap();
        if mode == Mode::Literal {
            assert!(
                !upload
                    .step(Duration::ZERO, Event::Control(3))
                    .unwrap()
                    .contains(&Action::Close)
            );
        }
        assert!(
            upload
                .step(Duration::ZERO, Event::Control(4))
                .unwrap()
                .contains(&Action::Close)
        );
        let request = Message::parse("mtmpr|piX|si|ci0001|rtHCG|", SampleLimit::Twelve)?;
        let mut download = Dialog::download(c, policy(), &request)?;
        let _ = download.step(Duration::ZERO, Event::Control(6)).unwrap();
        let actions = download.step(Duration::ZERO, Event::Control(6)).unwrap();
        assert_eq!(
            actions.contains(&Action::Write(vec![3])),
            mode == Mode::Literal
        );
        assert!(actions.contains(&Action::Write(vec![4])));
        assert!(actions.contains(&Action::Close));
    }
    Ok(())
}

// ADR 0001, ADR 0010
#[test]
fn dialog_owns_commit_and_session_deadlines() -> Result<(), Box<dyn std::error::Error>> {
    let c = config(Mode::Literal, &[], false)?;
    let mut dialog = Dialog::upload(c.clone(), policy())?;
    let _ = dialog.start();
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(2)));
    assert!(
        dialog
            .step(Duration::from_secs(1), Event::Activity)?
            .is_empty()
    );
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(3)));
    assert!(
        dialog
            .step(Duration::from_secs(2), Event::Activity)?
            .is_empty()
    );
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(3)));
    assert!(
        dialog
            .step(Duration::from_secs(3), Event::Activity)?
            .contains(&Action::Park)
    );
    assert_eq!(dialog.next_deadline(), None);
    assert_eq!(dialog.expired_timer(), Some(Timer::Session));

    let mut dialog = Dialog::upload(c.clone(), policy())?;
    let _ = dialog.start();
    let _ = dialog.step(Duration::ZERO, Event::Control(5))?;
    let mut decoder = Decoder::new(c.clone());
    let mut packet = None;
    for byte in c.encode(b"synthetic")? {
        packet = decoder.push(byte)?;
    }
    let actions = dialog.step(Duration::ZERO, Event::Packet(packet.ok_or("packet")?))?;
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Store(_)))
    );
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(1)));
    let actions = dialog.step(Duration::from_secs(1), Event::Stored)?;
    assert!(actions.contains(&Action::Park));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Write(_)))
    );
    assert_eq!(dialog.next_deadline(), None);
    Ok(())
}

// VIDAS BCI Link rev A chapter 3
#[test]
fn byte_input_splits_controls_and_acks_after_store() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_framing::vidas::{ACK, CR, ENQ, EOT, ETX, LF};
    for suffix in [&[][..], &[CR][..], &[LF][..], &[CR, LF][..]] {
        let config = config(Mode::Literal, suffix, false)?;
        let mut dialog = Dialog::upload(config.clone(), policy())?;
        let _ = dialog.start();
        let mut actions = Vec::new();
        for byte in config.control(ENQ) {
            actions.extend(dialog.step(Duration::ZERO, Event::Byte(byte))?);
        }
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Write(bytes) if *bytes == config.control(ACK)))
        );
        actions.clear();
        let packet = config.encode(b"A")?;
        for byte in &packet {
            actions.extend(dialog.step(Duration::ZERO, Event::Byte(*byte))?);
        }
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Store(p) if p.raw == packet))
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
        let committed = dialog.step(Duration::ZERO, Event::Stored)?;
        assert!(
            committed
                .iter()
                .any(|a| matches!(a, Action::Write(bytes) if *bytes == config.control(ACK)))
        );
        actions.clear();
        for byte in config.control(ETX).into_iter().chain(config.control(EOT)) {
            actions.extend(dialog.step(Duration::ZERO, Event::Byte(byte))?);
        }
        assert!(actions.contains(&Action::Close));
    }
    let mut dialog = Dialog::upload(config(Mode::Literal, b"\r\n", false)?, policy())?;
    let _ = dialog.start();
    dialog.step(Duration::ZERO, Event::Byte(ENQ))?;
    assert!(matches!(
        dialog.step(Duration::ZERO, Event::Byte(b'X')),
        Err(analyte_dialog::vidas::Error::Framing(
            analyte_framing::vidas::Error::ControlSuffix
        ))
    ));
    Ok(())
}

// BCI Link LIS guide 161150-486 C pp. 3-1 to 3-4; the configured session deadline bounds retries.
#[test]
fn repeated_bad_checksums_cannot_extend_the_session() -> Result<(), Box<dyn std::error::Error>> {
    use analyte_framing::vidas::{ENQ, ETX, NAK};
    for mode in [Mode::Literal, Mode::Alternate] {
        let c = config(mode, b"\r\n", false)?;
        let mut bad = c.encode(b"synthetic")?;
        let checksum = bad.len() - if mode == Mode::Literal { 1 } else { 4 };
        *bad.get_mut(checksum).ok_or("fixture checksum")? = ETX;
        let mut dialog = Dialog::upload(c.clone(), policy())?;
        let _ = dialog.start();
        for byte in c.control(ENQ) {
            dialog.step(Duration::ZERO, Event::Byte(byte))?;
        }
        for elapsed in [1, 2] {
            let now = Duration::from_secs(elapsed);
            let (last, prefix) = bad.split_last().ok_or("fixture")?;
            for &byte in prefix {
                assert!(dialog.step(now, Event::Byte(byte))?.is_empty());
            }
            let actions = dialog.step(now, Event::Byte(*last))?;
            assert!(actions.contains(&Action::RejectChecksum(bad.clone())));
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Write(_) | Action::Store(_)))
            );
            let actions = dialog.step(now, Event::Stored)?;
            assert!(actions.contains(&Action::Write(c.control(NAK))));
        }
        let ended = dialog.step(policy().session, Event::Tick)?;
        assert!(ended.contains(&Action::Park));
        assert_eq!(dialog.expired_timer(), Some(Timer::Session));
        assert_eq!(dialog.next_deadline(), None);
    }
    Ok(())
}
