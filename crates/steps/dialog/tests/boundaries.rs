//! Synthetic state-machine boundaries from the clinical rules and protocol scopes.
//! LIS01-A2 §§6.2.6–6.2.7.1/6.3.5 govern takeover; §6.4 governs EOT.
//! Configured ceilings and commit identity ordinals are Analyte rules 1/7 and
//! ADRs 0003/0010, not limits or identities specified by LIS01-A2 §6.3.
use analyte_dialog::astm::{Action, Config, Dialog, End, Epoch, Error, Event, Role, Timers};
use analyte_framing::astm::{self, ACK, ENQ, EOT, FrameNumber, Limit, NAK};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn adr0003_0010_astm_commit_identity_tracks_transfer_and_failed_request_ordinals() -> TestResult {
    let mut dialog = sender(100, 10, 64)?;
    dialog.step(Duration::ZERO, Event::Capacity(true))?;
    let mut ordinal = 0;
    for transfer in 1..=2 {
        dialog.step(Duration::ZERO, Event::Byte(ENQ))?;
        for success in [false, true] {
            let mut out = Vec::new();
            for byte in b"\x021A\x0375\r\n" {
                out.extend(dialog.step(Duration::ZERO, Event::Byte(*byte))?);
            }
            let id = out
                .iter()
                .find_map(|a| {
                    if let Action::Commit(id, _, _) = a {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .ok_or("expected commit request")?;
            assert_eq!(id.epoch(), Epoch(91));
            assert_eq!(id.transfer(), transfer);
            assert_eq!(id.frame(), ordinal);
            ordinal += 1;
            let done = dialog.step(
                Duration::ZERO,
                if success {
                    Event::Committed(id)
                } else {
                    Event::CommitFailed(id)
                },
            )?;
            assert!(done.contains(&Action::Write(vec![if success { ACK } else { NAK }])));
        }
        assert!(
            dialog
                .step(Duration::ZERO, Event::Byte(EOT))?
                .contains(&Action::Ended(End::Normal))
        );
    }
    Ok(())
}

#[test]
fn lis01_6_2_6_6_2_7_6_3_5_peer_enq_can_take_over_waiting_sender() -> TestResult {
    for waiting in ["contention", "busy", "holdoff"] {
        let mut dialog = sender(100, 10, 64)?;
        dialog.step(Duration::ZERO, Event::Capacity(true))?;
        dialog.step(Duration::ZERO, Event::Send(vec![b"O\r".to_vec()]))?;
        match waiting {
            "contention" => {
                dialog.step(Duration::ZERO, Event::Byte(ENQ))?;
            }
            "busy" => {
                dialog.step(Duration::ZERO, Event::Byte(NAK))?;
            }
            "holdoff" => {
                dialog.step(Duration::ZERO, Event::Byte(ACK))?;
                dialog.step(Duration::ZERO, Event::Byte(EOT))?;
            }
            _ => unreachable!("fixed test cases"),
        }
        let ignored = dialog.step(Duration::from_secs(1), Event::Byte(b'?'))?;
        assert!(!ignored.iter().any(|a| matches!(a, Action::Write(_))));
        let ready = dialog.step(Duration::from_secs(1), Event::Byte(ENQ))?;
        assert!(
            ready.contains(&Action::Write(vec![ACK])),
            "{waiting}: {ready:?}"
        );
        assert!(ready.contains(&Action::Deadline(Some(Duration::from_secs(31)))));
        assert!(!ready.iter().any(|a| matches!(a, Action::Commit(..))));
    }
    Ok(())
}

#[test]
fn adr0003_lis01_6_4_eot_during_commit_ends_incomplete_and_late_commit_never_acks() -> TestResult {
    let mut dialog = sender(100, 10, 64)?;
    dialog.step(Duration::ZERO, Event::Capacity(true))?;
    dialog.step(Duration::ZERO, Event::Byte(ENQ))?;
    let mut out = Vec::new();
    for byte in b"\x021A\x0375\r\n" {
        out.extend(dialog.step(Duration::ZERO, Event::Byte(*byte))?);
    }
    let id = out
        .iter()
        .find_map(|a| {
            if let Action::Commit(id, _, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or("expected pending commit")?;
    let ignored = dialog.step(Duration::ZERO, Event::Byte(ACK))?;
    assert!(!ignored.iter().any(|a| matches!(
        a,
        Action::CommitFailed(_) | Action::Ended(_) | Action::Write(_)
    )));
    let ended = dialog.step(Duration::ZERO, Event::Byte(EOT))?;
    assert!(ended.contains(&Action::CommitFailed(id)));
    assert!(ended.contains(&Action::Ended(End::Incomplete)));
    assert!(ended.contains(&Action::Deadline(None)));
    let late = dialog.step(Duration::ZERO, Event::Committed(id))?;
    assert!(late.contains(&Action::Refused(Error::StaleCommit)));
    assert!(
        !late
            .iter()
            .any(|a| matches!(a, Action::Write(_) | Action::Record(_)))
    );
    Ok(())
}

fn sender(bytes: usize, frames: usize, limit: usize) -> Result<Dialog, Box<dyn std::error::Error>> {
    Ok(Dialog::new(
        Config::new(
            Limit::new(limit)?,
            Timers::default(),
            Role::Computer,
            bytes,
            frames,
            true,
        )?,
        Epoch(91),
    ))
}

#[test]
fn rule7_lis01_6_3_sender_limits_are_inclusive_and_refuse_before_enq() -> TestResult {
    for (bytes, frames, records, accepted) in [
        (4, 2, vec![b"AB".to_vec(), b"CD".to_vec()], true),
        (4, 2, vec![b"A".to_vec(), b"B".to_vec()], true),
        (4, 2, vec![b"ABCDE".to_vec()], false),
        (
            4,
            2,
            vec![b"A".to_vec(), b"B".to_vec(), b"C".to_vec()],
            false,
        ),
        (4, 2, vec![], false),
    ] {
        let mut dialog = sender(bytes, frames, 64)?;
        let out = dialog.step(Duration::ZERO, Event::Send(records))?;
        assert_eq!(out.contains(&Action::Write(vec![ENQ])), accepted);
        assert_eq!(out.contains(&Action::Refused(Error::Limit)), !accepted);
        if !accepted {
            assert!(!out.iter().any(|a| matches!(a, Action::Write(_))));
            // A refused batch must not leave a half-started conversation.
            assert!(
                dialog
                    .step(Duration::ZERO, Event::Send(vec![b"A".to_vec()]))?
                    .contains(&Action::Write(vec![ENQ]))
            );
        }
    }
    Ok(())
}

#[test]
fn rule7_lis01_6_3_frame_limit_counts_split_frames_before_enq() -> TestResult {
    for (record, accepted) in [(b"AB".as_slice(), true), (b"ABC".as_slice(), false)] {
        let mut dialog = sender(10, 2, 8)?;
        let out = dialog.step(Duration::ZERO, Event::Send(vec![record.to_vec()]))?;
        assert_eq!(out.contains(&Action::Write(vec![ENQ])), accepted);
        assert_eq!(out.contains(&Action::Refused(Error::Limit)), !accepted);
    }
    Ok(())
}

#[test]
fn adr0003_rule7_lis01_6_3_receiver_limits_refuse_without_commit_or_ack() -> TestResult {
    for (bytes, frames, texts) in [
        (2, 10, vec![b"AB".as_slice(), b"ABC".as_slice()]),
        (
            10,
            2,
            vec![b"A".as_slice(), b"B".as_slice(), b"C".as_slice()],
        ),
    ] {
        let mut dialog = sender(bytes, frames, 64)?;
        dialog.step(Duration::ZERO, Event::Capacity(true))?;
        assert!(
            dialog
                .step(Duration::ZERO, Event::Byte(ENQ))?
                .contains(&Action::Write(vec![ACK]))
        );
        let mut number = FrameNumber::FIRST;
        for (index, text) in texts.iter().enumerate() {
            let wire: Vec<_> = astm::encode(text, number, Limit::new(64)?)?
                .flatten()
                .collect();
            number = number.next();
            let mut out = Vec::new();
            for byte in &wire {
                out.extend(dialog.step(Duration::ZERO, Event::Byte(*byte))?);
            }
            assert!(!out.contains(&Action::Write(vec![ACK])));
            if index + 1 == texts.len() {
                assert!(out.contains(&Action::LimitReached));
                assert!(out.contains(&Action::Write(vec![NAK])));
                assert!(out.contains(&Action::UnexpectedFrame(wire)));
                assert!(!out.iter().any(|a| matches!(a, Action::Commit(..))));
            } else {
                let id = out
                    .iter()
                    .find_map(|a| {
                        if let Action::Commit(id, _, raw) = a {
                            assert_eq!(raw, &wire);
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .ok_or("expected commit before ACK")?;
                let committed = dialog.step(Duration::ZERO, Event::Committed(id))?;
                assert!(committed.contains(&Action::Write(vec![ACK])));
                assert!(committed.contains(&Action::Record(text.to_vec())));
            }
        }
    }
    Ok(())
}
