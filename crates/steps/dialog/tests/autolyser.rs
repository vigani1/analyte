//! Tests autolyser rev18 p179 checksum is decimal sum; autolyser rev18 p178 controls are explicit and values stay written.
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

fn start(dialog: &mut Dialog, request: Request, time: u64) {
    assert!(
        matches!(dialog.step(Duration::from_secs(time), Event::Poll(request)).first(), Some(Action::Write(bytes)) if bytes == &[2])
    );
    assert!(
        matches!(dialog.step(Duration::from_secs(time), Event::Byte(6)).first(), Some(Action::Write(bytes)) if bytes == &[match request { Request::Next => b'R', Request::Last => b'L', Request::First => b'A' }, 4])
    );
}

fn receive(dialog: &mut Dialog, raw: &[u8], time: u64) -> Vec<Action> {
    let mut actions = Vec::new();
    for byte in raw {
        actions.extend(dialog.step(Duration::from_secs(time), Event::Byte(*byte)));
    }
    actions
}

#[test]
fn autolyser_rev18_p179_checksum_is_decimal_sum() -> TestResult {
    let report = Report::parse(RESULT, false)?;
    assert_eq!(report.raw, RESULT);
    assert_eq!(report.patient.bytes(), b"TEST0042       ");
    let rebuilt = Report::build(
        &report.patient,
        report.list,
        report.sample,
        None,
        &report.values,
    )?;
    assert_eq!(rebuilt.raw, RESULT);
    let order = Worklist::new(
        PatientCode::from_eight(b"TEST0042")?,
        List::Routine,
        Sample::Serum,
        false,
        5,
        vec![Field::new(b"ALB ")?, Field::new(b"CRE ")?],
    )?;
    assert_eq!(order.encode(), ORDER);
    assert_eq!(Worklist::parse(ORDER)?, order);
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
    let report = Report::build(
        &PatientCode::from_eight(b"TEST0042")?,
        List::Stat,
        Sample::Urine,
        Some(&control),
        &values,
    )?;
    assert_eq!(report.raw.len(), 60);
    assert_eq!(report.control, Some(control));
    assert_eq!(report.values, values);
    assert!(Report::parse(&report.raw, false).is_err());
    Ok(())
}

#[test]
fn autolyser_rev18_p177_raw_first_and_l_recovery_before_next_r() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::Next, 0);
    let corrupt = b"TEST0042       TS002ALB 0004.21CRE 00087.5190\x04";
    let actions = receive(&mut dialog, corrupt, 0);
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
        dialog.step(Duration::ZERO, Event::Stored(1)).first(),
        Some(Action::Park(DialogError::Record(Error::Checksum)))
    ));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Poll(Request::Next))
            .first(),
        Some(Action::Refused(_))
    ));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Poll(Request::First))
            .first(),
        Some(Action::Refused(_))
    ));
    start(&mut dialog, Request::Last, 1);
    let actions = receive(&mut dialog, RESULT, 1);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(2, bytes) if bytes == RESULT))
    );
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Stored(2))
            .first(),
        Some(Action::Report(_))
    ));
    start(&mut dialog, Request::Next, 2);
    Ok(())
}

#[test]
fn autolyser_rev18_p177_empty_queue_still_requires_storage() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::First, 0);
    assert!(
        receive(&mut dialog, &[21], 0)
            .iter()
            .any(|a| matches!(a, Action::Store(1, bytes) if bytes == &[21]))
    );
    assert!(matches!(
        dialog.step(Duration::ZERO, Event::Stored(1)).first(),
        Some(Action::Empty)
    ));
    Ok(())
}

#[test]
fn autolyser_rev18_p175_download_serializes_and_preserves_binary_reply() -> TestResult {
    let order = Worklist::parse(ORDER)?;
    let mut dialog = Dialog::new(config())?;
    assert!(
        matches!(dialog.step(Duration::ZERO, Event::Download(&order)).first(), Some(Action::Write(bytes)) if bytes == &[2])
    );
    assert!(matches!(
        dialog
            .step(Duration::ZERO, Event::Poll(Request::Next))
            .first(),
        Some(Action::Refused(_))
    ));
    assert!(
        matches!(dialog.step(Duration::ZERO, Event::Byte(6)).first(), Some(Action::Write(bytes)) if bytes == ORDER)
    );
    receive(&mut dialog, &[b'N', 21], 0);
    assert!(matches!(
        dialog.step(Duration::ZERO, Event::Stored(1)).first(),
        Some(Action::Insertion(Reception::Rejected(
            InsertionError::RepeatListMismatch
        )))
    ));
    assert_eq!(InsertionError::from(255), InsertionError::Other(255));
    Ok(())
}

#[test]
fn autolyser_rev18_p177_partial_timeout_and_storage_failure_never_resend() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::Next, 0);
    receive(&mut dialog, b"TEST", 0);
    let actions = dialog.step(Duration::from_secs(2), Event::Tick);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(1, raw) if raw == b"TEST"))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Close)));
    assert!(
        dialog
            .step(Duration::from_secs(2), Event::StoreFailed(1))
            .iter()
            .any(|a| matches!(a, Action::Close))
    );
    assert!(matches!(
        dialog
            .step(Duration::from_secs(3), Event::Poll(Request::Next))
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
        start(&mut dialog, Request::Next, 0);
        assert!(
            receive(&mut dialog, raw, 0)
                .iter()
                .any(|a| matches!(a, Action::Store(1, bytes) if bytes == raw))
        );
        assert!(
            dialog
                .step(Duration::ZERO, Event::Stored(1))
                .iter()
                .any(|a| matches!(a, Action::Park(_)))
        );
    }
    Ok(())
}

#[test]
fn autolyser_rev18_p175_disconnect_during_commit_cannot_reopen_connection() -> TestResult {
    let mut dialog = Dialog::new(config())?;
    start(&mut dialog, Request::Next, 0);
    receive(&mut dialog, RESULT, 0);
    assert!(
        dialog
            .step(Duration::ZERO, Event::Disconnect)
            .iter()
            .any(|a| matches!(a, Action::Close))
    );
    assert!(
        dialog
            .step(Duration::ZERO, Event::Stored(1))
            .iter()
            .any(|a| matches!(a, Action::Report(_)))
    );
    assert!(matches!(
        dialog
            .step(Duration::from_secs(1), Event::Poll(Request::Next))
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
    start(&mut dialog, Request::Next, 0);
    receive(&mut dialog, b"X\x04", 0);
    let _ = dialog.step(Duration::ZERO, Event::Stored(1));
    assert!(
        dialog
            .step(Duration::from_secs(1), Event::Tick)
            .iter()
            .any(|a| matches!(a, Action::Write(bytes) if bytes == &[2]))
    );
    assert!(
        dialog
            .step(Duration::from_secs(1), Event::Byte(6))
            .iter()
            .any(|a| matches!(a, Action::Write(bytes) if bytes == &[b'L', 4]))
    );
    receive(&mut dialog, &[21], 1);
    let _ = dialog.step(Duration::from_secs(1), Event::Stored(2));
    assert!(matches!(
        dialog
            .step(Duration::from_secs(2), Event::Poll(Request::Next))
            .first(),
        Some(Action::Refused(_))
    ));
    Ok(())
}

#[test]
fn autolyser_rev18_p175_lost_download_reply_is_uncertain_without_retry() -> TestResult {
    let order = Worklist::parse(ORDER)?;
    let mut dialog = Dialog::new(config())?;
    let _ = dialog.step(Duration::ZERO, Event::Download(&order));
    let _ = dialog.step(Duration::ZERO, Event::Byte(6));
    let actions = dialog.step(Duration::from_secs(2), Event::Tick);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Store(1, bytes) if bytes.is_empty()))
    );
    let actions = dialog.step(Duration::from_secs(2), Event::Stored(1));
    assert!(actions.iter().any(|a| matches!(a, Action::Uncertain)));
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
    Ok(())
}
