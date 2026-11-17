//! Tests ASTM link state transitions and acknowledgment safety.
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Error, Event, Role, Timers};
use analyte_framing::astm::{self, ACK, ENQ, EOT, FrameNumber, Limit, NAK};
use proptest::prelude::*;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[expect(
    clippy::expect_used,
    reason = "unexpected host time error must fail the test"
)]
fn step(d: &mut Dialog, t: u64, e: Event) -> Vec<Action> {
    d.step(Duration::from_secs(t), e)
        .expect("valid synthetic monotonic time")
}

fn feed(d: &mut Dialog, t: u64, bytes: &[u8]) -> Vec<Action> {
    bytes
        .iter()
        .flat_map(|b| step(d, t, Event::Byte(*b)))
        .collect()
}

fn receiver() -> Dialog {
    let mut d = Dialog::new(Config::default(), Epoch(1));
    step(&mut d, 0, Event::Capacity(true));
    step(&mut d, 0, Event::Byte(ENQ));
    d
}

fn wire(text: &[u8], number: FrameNumber, limit: Limit) -> Result<Vec<Vec<u8>>, astm::Error> {
    Ok(astm::encode(text, number, limit)?.collect())
}

fn commit(d: &mut Dialog, actions: &[Action], ok: bool) -> Vec<Action> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Store(id, _, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .flat_map(|id| {
            step(
                d,
                0,
                if ok {
                    Event::Stored(id)
                } else {
                    Event::StoreFailed(id)
                },
            )
        })
        .collect()
}

#[test]
fn lis01_6_2_1_to_7_neutral_readiness_and_busy() {
    let mut d = Dialog::new(Config::default(), Epoch(1));
    assert!(!feed(&mut d, 0, b"garbage").contains(&Action::Write(vec![ACK])));
    assert!(feed(&mut d, 0, &[ENQ]).contains(&Action::Write(vec![NAK])));
    step(&mut d, 0, Event::Capacity(true));
    assert!(feed(&mut d, 0, &[ENQ]).contains(&Action::Write(vec![ACK])));
}

#[test]
fn adr0003_lis01_6_3_2_6_5_1_commit_duplicate_and_conflict() -> TestResult {
    let mut d = receiver();
    let frames = wire(b"R|synthetic\r", FrameNumber::FIRST, Limit::default())?;
    let frame = frames.first().ok_or("missing frame")?;
    let a = feed(&mut d, 0, frame);
    assert!(a.iter().any(|action| matches!(action, Action::Store(_, number, bytes) if *number == FrameNumber::FIRST && bytes == frame)));
    assert!(!a.contains(&Action::Write(vec![ACK])));
    let committed = commit(&mut d, &a, true);
    assert!(committed.contains(&Action::Write(vec![ACK])));
    assert!(committed.contains(&Action::Record(b"R|synthetic\r".to_vec())));
    let duplicate = feed(&mut d, 0, frame);
    assert!(duplicate.contains(&Action::Write(vec![ACK])));
    assert!(
        !duplicate
            .iter()
            .any(|a| matches!(a, Action::Store(..) | Action::Record(..)))
    );
    let other = wire(b"different\r", FrameNumber::FIRST, Limit::default())?;
    let conflict = feed(&mut d, 0, other.first().ok_or("missing frame")?);
    assert!(conflict.iter().any(|a| matches!(a, Action::Conflict(..))));
    assert!(conflict.contains(&Action::Write(vec![NAK])));
    Ok(())
}

#[test]
fn adr0003_lis01_6_5_2_5_failed_and_unanswered_commit_never_ack() -> TestResult {
    for success in [false, true] {
        let mut d = receiver();
        let frames = wire(b"X\r", FrameNumber::FIRST, Limit::default())?;
        let a = feed(&mut d, 0, frames.first().ok_or("missing frame")?);
        let reply = if success {
            step(&mut d, 15, Event::Tick)
        } else {
            commit(&mut d, &a, false)
        };
        assert!(reply.contains(&Action::Write(vec![NAK])));
        assert!(!reply.contains(&Action::Write(vec![ACK])));
        assert!(reply.iter().any(|a| matches!(a, Action::StoreFailed(_))));
    }
    Ok(())
}

#[test]
fn lis01_6_3_1_2_6_5_2_4_etb_timeout_and_split_merge() -> TestResult {
    let frames = wire(b"ABCDEF\r", FrameNumber::FIRST, Limit::new(10)?)?;
    let mut d = receiver();
    let first = feed(&mut d, 0, frames.first().ok_or("missing frame")?);
    assert!(
        !commit(&mut d, &first, true)
            .iter()
            .any(|a| matches!(a, Action::Record(_)))
    );
    feed(&mut d, 29, b"garbage");
    assert!(step(&mut d, 30, Event::Tick).contains(&Action::Ended(End::Timeout)));
    let all: Vec<u8> = frames.concat();
    for boundary in 0..=all.len() {
        let mut d = receiver();
        let (left, right) = all.split_at(boundary);
        let mut outputs = feed(&mut d, 0, left);
        outputs.extend(feed(&mut d, 0, right));
        // Without stop-and-wait commits, only the first frame requests storage.
        assert_eq!(
            outputs
                .iter()
                .filter(|a| matches!(a, Action::Store(..)))
                .count(),
            1
        );
    }
    let mut d = receiver();
    let mut records = Vec::new();
    for frame in frames {
        let a = feed(&mut d, 0, &frame);
        records.extend(commit(&mut d, &a, true));
    }
    assert!(records.contains(&Action::Record(b"ABCDEF\r".to_vec())));
    Ok(())
}

#[test]
fn lis01_6_3_4_6_4_6_5_1_2_sender_six_failures_and_eot() {
    let mut d = Dialog::new(Config::default(), Epoch(1));
    assert!(
        step(&mut d, 0, Event::Send(vec![b"O\r".to_vec()])).contains(&Action::Write(vec![ENQ]))
    );
    let first = feed(&mut d, 0, &[ACK]);
    for _ in 0..5 {
        assert_eq!(feed(&mut d, 0, &[NAK]), first);
    }
    let end = feed(&mut d, 0, b"?");
    assert!(end.contains(&Action::Sent(End::Abort)));
    assert!(end.contains(&Action::Write(vec![EOT])));
    step(&mut d, 0, Event::Send(vec![b"O\r".to_vec()]));
    feed(&mut d, 0, &[ACK]);
    assert!(feed(&mut d, 0, &[ACK]).contains(&Action::Sent(End::Normal)));
}

#[test]
fn lis01_6_2_6_6_5_2_1_3_busy_and_reply_timeouts() {
    let mut d = Dialog::new(Config::default(), Epoch(1));
    step(&mut d, 0, Event::Send(vec![b"O\r".to_vec()]));
    feed(&mut d, 0, &[NAK]);
    assert!(!step(&mut d, 9, Event::Tick).contains(&Action::Write(vec![ENQ])));
    assert!(step(&mut d, 10, Event::Tick).contains(&Action::Write(vec![ENQ])));
    feed(&mut d, 24, b"ignored");
    let timeout = step(&mut d, 25, Event::Tick);
    assert!(timeout.contains(&Action::Sent(End::Timeout)));
    assert!(timeout.contains(&Action::Write(vec![EOT])));
    assert!(
        feed(&mut d, 25, &[ACK])
            .iter()
            .all(|a| !matches!(a, Action::Write(_)))
    );
    step(&mut d, 25, Event::Send(vec![b"O\r".to_vec()]));
    feed(&mut d, 25, &[ACK]);
    assert!(step(&mut d, 40, Event::Tick).contains(&Action::Sent(End::Timeout)));
}

#[test]
fn lis01_6_2_7_1_6_5_2_2_contention_both_roles() -> TestResult {
    for role in [Role::Computer, Role::Instrument] {
        let config = Config::new(
            Limit::default(),
            Timers::default(),
            role,
            1024,
            32,
            analyte_dialog::astm::Interrupt::Honor,
        )?;
        let mut d = Dialog::new(config, Epoch(1));
        step(&mut d, 0, Event::Send(vec![b"O\r".to_vec()]));
        feed(&mut d, 0, &[ENQ]);
        match role {
            Role::Computer => {
                assert!(step(&mut d, 20, Event::Tick).contains(&Action::Ended(End::Timeout)));
            }
            Role::Instrument => {
                assert!(step(&mut d, 1, Event::Tick).contains(&Action::Write(vec![ENQ])));
            }
        }
    }
    Ok(())
}

#[test]
fn lis01_6_3_5_interrupt_honor_ignore_and_issue() -> TestResult {
    for honor in [true, false] {
        let config = Config::new(
            Limit::default(),
            Timers::default(),
            Role::Computer,
            1024,
            32,
            if honor {
                analyte_dialog::astm::Interrupt::Honor
            } else {
                analyte_dialog::astm::Interrupt::Ignore
            },
        )?;
        let mut d = Dialog::new(config, Epoch(1));
        step(
            &mut d,
            0,
            Event::Send(vec![b"O\r".to_vec(), b"L\r".to_vec()]),
        );
        feed(&mut d, 0, &[ACK]);
        let a = feed(&mut d, 0, &[EOT]);
        assert_eq!(a.contains(&Action::Sent(End::Interrupt)), honor);
        if !honor {
            assert!(a.contains(&Action::Write(b"\x022L\r\x038E\r\n".to_vec())));
        }
        if honor {
            assert!(
                step(&mut d, 14, Event::Send(vec![vec![]])).contains(&Action::Refused(Error::Busy))
            );
            assert!(
                step(&mut d, 15, Event::Send(vec![b"O\r".to_vec()]))
                    .contains(&Action::Write(vec![ENQ]))
            );
        }
    }
    let mut d = receiver();
    step(&mut d, 0, Event::Interrupt(true));
    let frames = wire(b"X\r", FrameNumber::FIRST, Limit::default())?;
    let a = feed(&mut d, 0, frames.first().ok_or("missing frame")?);
    assert!(commit(&mut d, &a, true).contains(&Action::Write(vec![EOT])));
    Ok(())
}

#[test]
fn lis01_6_3_3_6_5_1_6_6_reject_checksum_sequence_and_controls() -> TestResult {
    let mut d = receiver();
    assert!(feed(&mut d, 0, b"\x021X\x0300\r\n").contains(&Action::Write(vec![NAK])));
    let frames = wire(b"X\r", FrameNumber::FIRST.next(), Limit::default())?;
    assert!(
        feed(&mut d, 0, frames.first().ok_or("missing frame")?).contains(&Action::Write(vec![NAK]))
    );
    assert!(
        d.step(Duration::ZERO, Event::Byte(EOT))?
            .contains(&Action::Ended(End::Normal))
    );
    assert!(
        step(&mut d, 0, Event::Send(vec![vec![10]]))
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Framing(_))))
    );
    Ok(())
}

proptest! {
    #[test]
    fn adr0003_random_bytes_and_tick_orders_never_ack_uncommitted(events in prop::collection::vec((any::<u8>(), 0u64..100), 0..1000)) {
        let mut d = Dialog::new(Config::default(), Epoch(42));
        // Capacity stays false: no establishment or data ACK is authorized.
        for (byte, t) in events {
            if let Ok(a) = d.step(Duration::from_secs(t), Event::Byte(byte)) {
                prop_assert!(!a.contains(&Action::Write(vec![ACK])));
            }
            if let Ok(a) = d.step(Duration::from_secs(t), Event::Tick) {
                prop_assert!(!a.contains(&Action::Write(vec![ACK])));
            }
        }
    }
}

#[test]
fn lis01_6_3_1_6_3_2_6_3_3_6_6_large_records_rollover_separate_records() -> TestResult {
    let config = Config::new(
        Limit::new(10)?,
        Timers::default(),
        Role::Computer,
        1000,
        100,
        analyte_dialog::astm::Interrupt::Honor,
    )?;
    let mut d = Dialog::new(config, Epoch(7));
    step(
        &mut d,
        0,
        Event::Send(vec![vec![b'X'; 26], b"Y\r".to_vec()]),
    );
    let mut frames = Vec::new();
    for _ in 0..10 {
        for action in feed(&mut d, 0, &[ACK]) {
            if let Action::Write(raw) = action {
                frames.push(raw);
            }
        }
    }
    let expected = wire(&[b'X'; 26], FrameNumber::FIRST, Limit::new(10)?)?;
    assert_eq!(frames.get(..9), Some(expected.as_slice()));
    assert!(frames.iter().all(|raw| raw.len() <= 10));
    assert_eq!(frames.get(7).and_then(|f| f.get(1)), Some(&b'0'));
    assert_eq!(frames.get(8).and_then(|f| f.get(1)), Some(&b'1'));
    assert_eq!(frames.get(9).and_then(|f| f.get(1)), Some(&b'2'));
    let max = wire(&vec![b'X'; 63_994], FrameNumber::FIRST, Limit::default())?;
    assert_eq!(max.first().map(Vec::len), Some(64_000));
    assert_eq!(max.len(), 2);
    Ok(())
}

#[test]
fn adr0003_late_commit_and_failed_retry_have_distinct_identity() -> TestResult {
    let mut d = receiver();
    let frames = wire(b"X\r", FrameNumber::FIRST, Limit::default())?;
    let frame = frames.first().ok_or("missing frame")?;
    let a = feed(&mut d, 0, frame);
    let old = a
        .iter()
        .find_map(|a| {
            if let Action::Store(id, _, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or("no commit")?;
    commit(&mut d, &a, false);
    let b = feed(&mut d, 0, frame);
    assert!(!b.contains(&Action::Store(old, FrameNumber::FIRST, frame.clone())));
    assert!(step(&mut d, 0, Event::Stored(old)).contains(&Action::Refused(Error::StaleCommit)));
    let id = b
        .iter()
        .find_map(|a| {
            if let Action::Store(id, _, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or("no retry commit")?;
    let late = step(&mut d, 15, Event::Stored(id));
    assert!(late.contains(&Action::StoreFailed(id)));
    assert!(late.contains(&Action::Write(vec![NAK])));
    assert!(late.contains(&Action::Refused(Error::StaleCommit)));
    assert!(!late.contains(&Action::Write(vec![ACK])));
    Ok(())
}

#[test]
fn lis01_6_5_2_6_one_message_buffer_and_validated_vendor_values() -> TestResult {
    assert!(
        Config::new(
            Limit::default(),
            Timers::default(),
            Role::Computer,
            0,
            1,
            analyte_dialog::astm::Interrupt::Honor
        )
        .is_err()
    );
    let timers = Timers {
        receive: Duration::from_secs(15),
        ..Timers::default()
    };
    let mut d = Dialog::new(
        Config::new(
            Limit::new(240)?,
            timers,
            Role::Computer,
            100,
            10,
            analyte_dialog::astm::Interrupt::Honor,
        )?,
        Epoch(1),
    );
    step(&mut d, 0, Event::Capacity(true));
    feed(&mut d, 0, &[ENQ]);
    assert!(step(&mut d, 15, Event::Tick).contains(&Action::Ended(End::Timeout)));
    step(&mut d, 15, Event::Send(vec![b"O\r".to_vec()]));
    assert!(
        step(&mut d, 15, Event::Send(vec![b"L\r".to_vec()]))
            .contains(&Action::Refused(Error::Busy))
    );
    Ok(())
}

proptest! {
    #[test]
    fn adr0003_active_receiver_arbitrary_ticks_bytes_without_commits_never_ack(events in prop::collection::vec((any::<u8>(), 0u64..100), 0..500)) {
        let mut d = receiver();
        step(&mut d, 0, Event::Capacity(false));
        // A synthetic valid frame ensures this exercises the pending-commit state.
        feed(&mut d, 0, b"\x021X\x038C\r\n");
        for (byte, t) in events {
            if let Ok(a) = d.step(Duration::from_secs(t), Event::Byte(byte)) {
                prop_assert!(!a.contains(&Action::Write(vec![ACK])));
            }
            if let Ok(a) = d.step(Duration::from_secs(t), Event::Tick) {
                prop_assert!(!a.contains(&Action::Write(vec![ACK])));
            }
        }
    }
}

#[test]
fn lis01_6_4_eot_before_final_frame_ends_incomplete() -> TestResult {
    let mut d = receiver();
    let frames = wire(&[b'A'; 40], FrameNumber::FIRST, Limit::new(17)?)?;
    let first = frames.first().ok_or("missing frame")?;
    let a = feed(&mut d, 0, first);
    let acked = commit(&mut d, &a, true);
    assert!(acked.contains(&Action::Write(vec![ACK])));
    let ended = feed(&mut d, 1, &[EOT]);
    assert!(ended.contains(&Action::Ended(End::Incomplete)));
    assert!(!ended.iter().any(|a| matches!(a, Action::Record(_))));
    Ok(())
}

#[test]
fn adr0003_commit_timeout_keeps_transfer_open_for_retransmission() -> TestResult {
    let mut d = receiver();
    let frames = wire(b"X\r", FrameNumber::FIRST, Limit::default())?;
    let frame = frames.first().ok_or("missing frame")?;
    let _ = feed(&mut d, 0, frame);
    let timeout = step(&mut d, 15, Event::Tick);
    assert!(timeout.contains(&Action::Write(vec![NAK])));
    assert!(!timeout.iter().any(|a| matches!(a, Action::Ended(_))));
    // The sender retransmits the same frame; it is committed again and ACKed.
    let again = feed(&mut d, 16, frame);
    assert!(again.iter().any(|a| matches!(a, Action::Store(..))));
    let id = again
        .iter()
        .find_map(|a| {
            if let Action::Store(id, _, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or("missing commit")?;
    let acked = step(&mut d, 16, Event::Stored(id));
    assert!(acked.contains(&Action::Write(vec![ACK])));
    Ok(())
}

// LIS01-A2 sections 6.5.2.4 p.10, 8.5.2.4 p.17: every frame reply renews reception.
#[test]
fn lis01_6_5_2_4_8_5_2_4_nak_at_29_expires_at_59() -> TestResult {
    for kind in ["checksum", "conflict", "sequence"] {
        let mut d = receiver();
        let first = wire(b"H|x\r", FrameNumber::FIRST, Limit::default())?.remove(0);
        let actions = feed(&mut d, 0, &first);
        commit(&mut d, &actions, true);
        let mut bad = wire(
            b"H|y\r",
            if kind == "sequence" {
                FrameNumber::FIRST.next().next()
            } else {
                FrameNumber::FIRST
            },
            Limit::default(),
        )?
        .remove(0);
        if kind == "checksum" {
            *bad.get_mut(3).ok_or("frame short")? = b'z';
        }
        assert!(feed(&mut d, 29, &bad).contains(&Action::Write(vec![NAK])));
        assert!(!step(&mut d, 30, Event::Tick).contains(&Action::Ended(End::Timeout)));
        assert!(!step(&mut d, 58, Event::Tick).contains(&Action::Ended(End::Timeout)));
        assert!(step(&mut d, 59, Event::Tick).contains(&Action::Ended(End::Timeout)));
    }
    Ok(())
}

#[test]
fn lis01_6_3_1_1_8_3_1_1_separate_records_never_share_strict_frame() -> TestResult {
    for payload in [b"H|x\rL|1\r".as_slice(), b"H|x\rL".as_slice()] {
        let mut d = receiver();
        let frames = wire(payload, FrameNumber::FIRST, Limit::default())?;
        let actions = feed(&mut d, 0, &frames[0]);
        assert!(actions.contains(&Action::Write(vec![NAK])));
        assert!(!actions.iter().any(|a| matches!(a, Action::Store(..))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Rejected(_, raw) if raw == &frames[0]))
        );
    }
    Ok(())
}

#[test]
fn adr0001_0003_disconnect_returns_partial_frame_and_never_writes_or_reopens() -> TestResult {
    let mut dialog = receiver();
    feed(&mut dialog, 0, b"\x021R|partial");
    let actions = step(&mut dialog, 1, Event::Disconnected);
    assert!(actions.iter().any(|action| matches!(action,
        Action::Rejected(astm::Error::Incomplete, raw) if raw == b"\x021R|partial")));
    assert!(actions.contains(&Action::Ended(End::Incomplete)));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Write(_)))
    );
    assert_eq!(dialog.next_deadline(), None);
    assert!(
        !feed(&mut dialog, 2, &[ENQ])
            .iter()
            .any(|action| matches!(action, Action::Write(_)))
    );
    assert!(
        !step(&mut dialog, 2, Event::Disconnected)
            .iter()
            .any(|action| matches!(action, Action::Rejected(_, _)))
    );
    Ok(())
}

#[test]
fn adr0003_rule2_unsolicited_partial_frame_is_owned_and_bounded_without_ack() {
    let mut dialog = Dialog::new(Config::default(), Epoch(2));
    assert!(
        !feed(&mut dialog, 0, b"\x021R|unsolicited")
            .iter()
            .any(|a| matches!(a, Action::Write(_)))
    );
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(30)));
    let actions = step(&mut dialog, 1, Event::Disconnected);
    assert!(actions.iter().any(|a| matches!(a, Action::Rejected(astm::Error::Incomplete, raw) if raw == b"\x021R|unsolicited")));
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
}
