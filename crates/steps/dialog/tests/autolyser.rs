//! Tests Autolyser polling, checksums and order exchanges.
use analyte_autolyser::syntax::*;
use analyte_dialog::autolyser::{
    Action, Config, Dialog, Error as DialogError, Event, InsertionError, Reception, Recovery,
    Request,
};
use proptest::prelude::*;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const RESULT: &[u8] = b"TEST0042       TS002ALB 0004.21CRE 00087.5191\x04";
const ORDER: &[u8] = b"TEST0042       TSN0502ALB CRE 139\x04";

fn config() -> Config {
    Config {
        poll: Duration::from_secs(1),
        ack: Duration::from_secs(2),
        response: Duration::from_secs(4),
        inter_byte: Duration::from_secs(2),
        storage: Duration::from_secs(2),
        max_bytes: 200,
        controls: false,
        recovery: Recovery::ManualLast,
    }
}

fn start(dialog: &mut Dialog, request: Request, time: u64) -> Result<(), DialogError> {
    assert!(
        matches!(dialog.step(Duration::from_secs(time), Event::Poll(request))?.first(), Some(Action::Write(bytes)) if bytes == &[2])
    );
    assert!(
        matches!(dialog.step(Duration::from_secs(time), Event::Byte(6))?.first(), Some(Action::Write(bytes)) if bytes == &[match request { Request::Next => b'R', Request::Last => b'L', Request::First => b'A' }, 4])
    );
    Ok(())
}

fn receive(dialog: &mut Dialog, raw: &[u8], time: u64) -> Result<Vec<Action>, DialogError> {
    let mut actions = Vec::new();
    for byte in raw {
        actions.extend(dialog.step(Duration::from_secs(time), Event::Byte(*byte))?);
    }
    Ok(actions)
}

#[test]
fn autolyser_rev18_p179_checksum_is_decimal_sum() -> TestResult {
    let report = Report::parse(analyte_framing::autolyser::verified(RESULT)?, false)?;
    assert_eq!(
        analyte_framing::autolyser::finish(report.raw.clone()),
        RESULT
    );
    assert_eq!(report.patient.bytes(), b"TEST0042       ");
    let rebuilt = Report::new(
        &report.patient,
        report.list,
        report.sample,
        None,
        &report.values,
    )?;
    assert_eq!(analyte_framing::autolyser::finish(rebuilt.raw), RESULT);
    let order = Worklist::new(
        PatientCode::from_eight(b"TEST0042")?,
        List::Routine,
        Sample::Serum,
        false,
        5,
        vec![Field::new(b"ALB ")?, Field::new(b"CRE ")?],
    )?;
    assert_eq!(analyte_framing::autolyser::finish(order.encode()), ORDER);
    assert_eq!(
        Worklist::parse(analyte_framing::autolyser::verified(ORDER)?)?,
        order
    );
    Ok(())
}

#[test]
fn autolyser_rev18_p178_controls_are_explicit_and_values_stay_written() -> TestResult {
    let control = Control {
        is_control: true,
        known: false,
        level: Field::new(b"002")?,
        lot: Field::new(b"SYNTHETIC           ")?,
    };
    let values = [Value {
        code: Field::new(b"TEST")?,
        text: Field::new(b"<NC>   ")?,
    }];
    let report = Report::new(
        &PatientCode::from_eight(b"TEST0042")?,
        List::Stat,
        Sample::Urine,
        Some(&control),
        &values,
    )?;
    // Rev18 §23.2 p.178 offsets and §23.3 p.179 checksum, independently summed.
    let expected = b"TEST0042       RUYU002SYNTHETIC           001TEST<NC>   ";
    let sum = expected
        .iter()
        .fold(0u64, |sum, byte| sum + u64::from(*byte))
        % 256;
    let mut wire = expected.to_vec();
    wire.extend_from_slice(format!("{sum:03}\x04").as_bytes());
    assert_eq!(analyte_framing::autolyser::finish(report.raw.clone()), wire);
    let mut shifted = wire.clone();
    shifted.insert(17, b' ');
    assert!(Report::parse(&shifted, true).is_err());
    let mut corrupt = wire;
    *corrupt.get_mut(0).ok_or("fixture")? = b'X';
    assert!(matches!(
        analyte_framing::autolyser::verified(&corrupt),
        Err(analyte_framing::autolyser::Error::Checksum)
    ));
    assert_eq!(
        analyte_framing::autolyser::finish(report.raw.clone()).len(),
        60
    );
    assert_eq!(report.control, Some(control));
    assert_eq!(report.values, values);
    assert!(Report::parse(&report.raw, false).is_err());
    Ok(())
}

#[test]
fn autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::Next, 0).unwrap();
    let corrupt = b"TEST0042       TS002ALB 0004.21CRE 00087.5190\x04";
    let actions = receive(&mut dialog, corrupt, 0).unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(1, bytes) if bytes == corrupt))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Report(_) | Action::Park(_)))
    );
    assert!(matches!(
        dialog
            .step(Duration::ZERO, Event::Stored(1))
            .unwrap()
            .first(),
        Some(Action::Park(DialogError::Framing(
            analyte_framing::autolyser::Error::Checksum
        )))
    ));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Poll(Request::Next))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Poll(Request::First))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    start(&mut dialog, Request::Last, 1).unwrap();
    let actions = receive(&mut dialog, RESULT, 1).unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(2, bytes) if bytes == RESULT))
    );
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Stored(2))
            .unwrap()
            .first(),
        Some(Action::Report(_))
    ));
    start(&mut dialog, Request::Next, 2).unwrap();
    Ok(())
}

#[test]
fn autolyser_rev18_p177_empty_queue_still_requires_storage() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::First, 0).unwrap();
    let actions = receive(&mut dialog, &[21], 0).unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(1, bytes) if bytes == &[21]))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Empty)));
    assert!(matches!(
        dialog
            .step(Duration::ZERO, Event::Stored(1))
            .unwrap()
            .first(),
        Some(Action::Empty)
    ));
    Ok(())
}

#[test]
fn autolyser_rev18_p175_download_serializes_and_preserves_binary_reply() -> TestResult {
    let order = Worklist::parse(analyte_framing::autolyser::verified(ORDER)?)?;
    let mut dialog = Dialog::new(config())?;
    assert!(
        matches!(dialog.step(Duration::ZERO, Event::Download(&order)).unwrap().first(), Some(Action::Write(bytes)) if bytes == &[2])
    );
    assert!(matches!(
        dialog
            .step(Duration::ZERO, Event::Poll(Request::Next))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    assert!(
        matches!(dialog.step(Duration::ZERO, Event::Byte(6)).unwrap().first(), Some(Action::Write(bytes)) if bytes == ORDER)
    );
    let actions = receive(&mut dialog, &[b'N', 21], 0).unwrap();
    assert!(!actions.iter().any(|a| matches!(a, Action::Insertion(_))));
    assert!(matches!(
        dialog
            .step(Duration::ZERO, Event::Stored(1))
            .unwrap()
            .first(),
        Some(Action::Insertion(Reception::Rejected(
            InsertionError::RepeatListMismatch
        )))
    ));
    assert_eq!(InsertionError::from(255), InsertionError::Other(255));
    let _ = dialog
        .step(Duration::from_secs(1), Event::Download(&order))
        .unwrap();
    let _ = dialog.step(Duration::from_secs(1), Event::Byte(6)).unwrap();
    let actions = receive(&mut dialog, &[b'Y', 3], 1).unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(2, bytes) if bytes == &[b'Y', 3]))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Insertion(_))));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Stored(2))
            .unwrap()
            .first(),
        Some(Action::Insertion(Reception::Accepted(3)))
    ));
    Ok(())
}

#[test]
fn autolyser_rev18_p177_partial_timeout_and_storage_failure_never_resend() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::Next, 0).unwrap();
    receive(&mut dialog, b"TEST", 0).unwrap();
    let actions = dialog.step(Duration::from_secs(2), Event::Tick).unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(1, raw) if raw == b"TEST"))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Close)));
    assert!(
        dialog
            .step(Duration::from_secs(2), Event::StoreFailed(1))
            .unwrap()
            .iter()
            .any(|a| matches!(a, Action::Close))
    );
    assert!(matches!(
        dialog
            .step(Duration::from_secs(3), Event::Poll(Request::Next))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    Ok(())
}

proptest! {
    #[test]
    fn autolyser_rev18_p179_arbitrary_input_is_bounded(raw in prop::collection::vec(any::<u8>(), 0..12000)) {
        let _ = Report::parse(&raw, false);
        let _ = Report::parse(&raw, true);
        let _ = Worklist::parse(&raw);
    }

    #[test]
    fn autolyser_rev18_p179_checksum_wraps_explicitly(raw in prop::collection::vec(any::<u8>(), 0..1000)) {
        let sum: u64 = raw.iter().map(|b| u64::from(*b)).sum();
        prop_assert_eq!(u64::from(analyte_framing::autolyser::checksum(&raw)), sum % 256);
    }
}

#[test]
fn autolyser_rev18_p177_overlong_and_short_reports_are_stored_before_rejection() -> TestResult {
    for raw in [b"X\x04".as_slice(), b"12345678".as_slice()] {
        let mut policy = config();
        policy.max_bytes = 8;
        let mut dialog = Dialog::new(policy)?;
        start(&mut dialog, Request::Next, 0).unwrap();
        let actions = receive(&mut dialog, raw, 0).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Store(1, bytes) if bytes == raw))
        );
        if !raw.ends_with(&[4]) {
            assert!(actions.iter().any(|a| matches!(a, Action::Close)));
        }
        assert!(
            dialog
                .step(Duration::ZERO, Event::Stored(1))
                .unwrap()
                .iter()
                .any(|a| matches!(a, Action::Park(_)))
        );
    }
    Ok(())
}

#[test]
fn autolyser_rev18_p175_disconnect_during_commit_cannot_reopen_connection() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::Next, 0).unwrap();
    receive(&mut dialog, RESULT, 0).unwrap();
    assert!(
        dialog
            .step(Duration::ZERO, Event::Disconnect)
            .unwrap()
            .iter()
            .any(|a| matches!(a, Action::Close))
    );
    assert!(
        dialog
            .step(Duration::ZERO, Event::Stored(1))
            .unwrap()
            .iter()
            .any(|a| matches!(a, Action::Report(_)))
    );
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Poll(Request::Next))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    Ok(())
}

#[test]
fn autolyser_rev18_p177_l_empty_does_not_resolve_corruption() -> TestResult {
    let mut policy = config();
    policy.recovery = Recovery::LastOnPoll;
    let mut dialog = Dialog::new(policy)?;
    start(&mut dialog, Request::Next, 0).unwrap();
    receive(&mut dialog, b"X\x04", 0).unwrap();
    let _ = dialog.step(Duration::ZERO, Event::Stored(1)).unwrap();
    assert!(
        dialog
            .step(Duration::from_secs(1), Event::Tick)
            .unwrap()
            .iter()
            .any(|a| matches!(a, Action::Write(bytes) if bytes == &[2]))
    );
    assert!(
        dialog
            .step(Duration::from_secs(1), Event::Byte(6))
            .unwrap()
            .iter()
            .any(|a| matches!(a, Action::Write(bytes) if bytes == &[b'L', 4]))
    );
    receive(&mut dialog, &[21], 1).unwrap();
    let _ = dialog
        .step(Duration::from_secs(1), Event::Stored(2))
        .unwrap();
    assert!(matches!(
        dialog
            .step(Duration::from_secs(2), Event::Poll(Request::First))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(2), Event::Poll(Request::Next))
            .unwrap()
            .first(),
        Some(Action::Refused(_))
    ));
    Ok(())
}

#[test]
fn autolyser_rev18_p175_lost_download_reply_is_uncertain_without_retry() -> TestResult {
    let order = Worklist::parse(analyte_framing::autolyser::verified(ORDER)?)?;
    let mut dialog = Dialog::new(config())?;
    let _ = dialog
        .step(Duration::ZERO, Event::Download(&order))
        .unwrap();
    let _ = dialog.step(Duration::ZERO, Event::Byte(6)).unwrap();
    let actions = dialog.step(Duration::from_secs(2), Event::Tick).unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(1, bytes) if bytes.is_empty()))
    );
    let actions = dialog
        .step(Duration::from_secs(2), Event::Stored(1))
        .unwrap();
    assert!(actions.iter().any(|a| matches!(a, Action::Uncertain)));
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
    Ok(())
}

#[test]
fn autolyser_rev18_section23_1_p175_explicit_worklist_fields() -> TestResult {
    for list in [List::Routine, List::Stat] {
        for sample in [Sample::Serum, Sample::Urine] {
            for clone in [false, true] {
                let order = Worklist::new(
                    PatientCode::from_eight(b"TEST0042")?,
                    list,
                    sample,
                    clone,
                    7,
                    vec![Field::new(b"TEST")?],
                )?;
                let bytes = order.encode();
                assert_eq!(
                    bytes.get(15..20),
                    Some(
                        [
                            match list {
                                List::Routine => b'T',
                                List::Stat => b'R',
                            },
                            match sample {
                                Sample::Serum => b'S',
                                Sample::Urine => b'U',
                            },
                            if clone { b'Y' } else { b'N' },
                            b'0',
                            b'7'
                        ]
                        .as_slice()
                    )
                );
            }
        }
    }
    Ok(())
}

#[test]
fn autolyser_rev18_section23_2_p177_required_bounds_and_independent_deadlines() -> TestResult {
    for index in 0..5 {
        let mut policy = config();
        match index {
            0 => policy.poll = Duration::ZERO,
            1 => policy.ack = Duration::ZERO,
            2 => policy.response = Duration::ZERO,
            3 => policy.inter_byte = Duration::ZERO,
            4 => policy.storage = Duration::ZERO,
            _ => unreachable!(),
        }
        assert!(matches!(Dialog::new(policy), Err(DialogError::Config)));
    }
    for max_bytes in [0, 1, 11039] {
        let mut policy = config();
        policy.max_bytes = max_bytes;
        assert!(matches!(Dialog::new(policy), Err(DialogError::Config)));
    }
    for total in [false, true] {
        let mut policy = config();
        if total {
            policy.inter_byte = Duration::from_secs(10);
        }
        let mut dialog = Dialog::new(policy)?;
        start(&mut dialog, Request::Next, 0).unwrap();
        receive(&mut dialog, b"TEST", 1).unwrap();
        let time = if total { 4 } else { 3 };
        if total {
            receive(&mut dialog, b"X", 3).unwrap();
        }
        let before = dialog
            .step(Duration::from_secs(time - 1), Event::Tick)
            .unwrap();
        assert!(
            !before
                .iter()
                .any(|a| matches!(a, Action::Store(_, _) | Action::Close))
        );
        let expected = if total {
            b"TESTX".as_slice()
        } else {
            b"TEST".as_slice()
        };
        let actions = dialog.step(Duration::from_secs(time), Event::Tick).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Store(1, raw) if raw == expected))
        );
        assert!(actions.iter().any(|a| matches!(a, Action::Close)));
        let _ = dialog
            .step(Duration::from_secs(time), Event::Stored(1))
            .unwrap();
        assert!(
            !dialog
                .step(Duration::from_secs(20), Event::Tick)
                .unwrap()
                .iter()
                .any(|a| matches!(a, Action::Write(_)))
        );
    }
    Ok(())
}

#[test]
fn adr0001_autolyser_manual_recovery_has_no_polling_wake() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    // Before the transport opens, there is no protocol operation to time.
    assert_eq!(dialog.next_deadline(), None);
    start(&mut dialog, Request::Next, 0).unwrap();
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(2)));
    receive(
        &mut dialog,
        b"TEST0042       TS002ALB 0004.21CRE 00087.5190\x04",
        0,
    )
    .unwrap();
    let actions = dialog.step(Duration::ZERO, Event::Stored(1)).unwrap();
    assert!(matches!(actions.first(), Some(Action::Park(_))));
    assert_eq!(dialog.next_deadline(), None);
    assert!(
        dialog
            .step(Duration::from_secs(10), Event::Tick)
            .unwrap()
            .is_empty()
    );
    assert_eq!(dialog.next_deadline(), None);
    Ok(())
}

#[test]
fn adr0001_autolyser_step_rejects_reversed_time_before_mutation() {
    let mut dialog = Dialog::new(config()).unwrap();
    dialog.step(Duration::from_secs(2), Event::Tick).unwrap();
    assert!(matches!(
        dialog.step(Duration::from_secs(1), Event::Poll(Request::Next)),
        Err(DialogError::TimeReversed)
    ));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(2), Event::Poll(Request::Next))
            .unwrap()
            .first(),
        Some(Action::Write(_))
    ));
}

#[test]
fn adr0001_autolyser_deadline_overflow_is_exhausted() {
    let mut dialog = Dialog::new(config()).unwrap();
    let actions = dialog
        .step(Duration::MAX, Event::Poll(Request::Next))
        .unwrap();
    assert!(matches!(
        actions.as_slice(),
        [Action::Refused(DialogError::Exhausted)]
    ));
}
