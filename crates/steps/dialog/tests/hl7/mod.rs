//! Tests HL7 messages, acknowledgments and failure handling.
use analyte_dialog::hl7::*;
use analyte_fixtures::ConfigEdit as _;
use analyte_hl7::{AckCode, AckError, ControlId, Options, Response};
use proptest::prelude::*;
use std::time::Duration;

mod boundaries;
mod compatibility;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn dialog() -> Result<Dialog, Error> {
    Ok(Dialog::new(
        Config::new(Options::default(), Duration::from_secs(5), 1)?,
        Epoch(42),
    ))
}
fn message(mode: &str) -> Vec<u8> {
    format!("\x0bMSH|^~\\&|A|B|C|D|20261115||ORU^R01^ORU_R01|001|P|2.5.1|||{mode}\r\x1c\r")
        .into_bytes()
}
fn feed(d: &mut Dialog, bytes: &[u8]) -> Result<Vec<Action>, Error> {
    let mut out = Vec::new();
    for byte in bytes {
        out.extend(d.step(Duration::ZERO, Event::Byte(*byte))?);
    }
    Ok(out)
}
fn receipt(actions: &[Action]) -> Result<ExchangeId, Box<dyn std::error::Error>> {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Store(id, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or_else(|| "no commit".into())
}
fn respond(
    d: &mut Dialog,
    id: ExchangeId,
    control: &[u8],
) -> Result<Vec<Action>, Box<dyn std::error::Error>> {
    Ok(d.step(
        Duration::ZERO,
        Event::Respond(
            id,
            Response {
                control_id: ControlId::new(control)?,
                timestamp: b"20261115",
            },
        ),
    )?)
}
fn has_code(actions: &[Action], code: &[u8]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Write(analyte_dialog::Write { bytes, .. }) if bytes.windows(code.len()).any(|w| w == code)))
}

// HL7 2.5.1 §2.9.2
#[test]
fn aa_only_after_store_and_interpret() -> TestResult {
    let mut d = dialog()?;
    let id = receipt(&feed(&mut d, &message(""))?)?;
    assert!(
        d.step(Duration::ZERO, Event::Interpreted(id))?
            .iter()
            .any(|a| matches!(a, Action::Refused(_)))
    );
    assert!(
        d.step(Duration::ZERO, Event::Stored(id))?
            .iter()
            .any(|a| matches!(a, Action::Interpret(_, _)))
    );
    d.step(Duration::ZERO, Event::Interpreted(id))?;
    let actions = respond(&mut d, id, b"response1")?;
    assert!(has_code(&actions, b"MSA|AA|001\r"));
    assert!(has_code(&actions, b"|response1|"));
    Ok(())
}

// HL7 2.5.1 §2.9.2
#[test]
fn failed_store_ar_and_bad_content_ae() -> TestResult {
    for stored in [false, true] {
        let mut d = dialog()?;
        let id = receipt(&feed(&mut d, &message(""))?)?;
        if stored {
            d.step(Duration::ZERO, Event::Stored(id))?;
            d.step(
                Duration::ZERO,
                Event::NotInterpretable(
                    id,
                    AckError {
                        segment: b"OBX",
                        occurrence: 1,
                        field: 5,
                        code: b"102",
                        application_code: b"",
                    },
                    AckChoice::ContentError,
                ),
            )?;
        } else {
            d.step(Duration::ZERO, Event::StoreFailed(id))?;
        }
        let actions = respond(&mut d, id, b"negative1")?;
        assert!(has_code(
            &actions,
            if stored { b"MSA|AE|" } else { b"MSA|AR|" }
        ));
        if stored {
            assert!(has_code(&actions, b"ERR||OBX^1^5|102|E|"));
        }
    }
    Ok(())
}

// HL7 2.5.1 §2.9.3
#[test]
fn accept_and_application_conditions_independent() -> TestResult {
    for accept in ["AL", "NE", "ER", "SU"] {
        for application in ["AL", "NE", "ER", "SU"] {
            let mut d = dialog()?;
            let id = receipt(&feed(&mut d, &message(&format!("{accept}|{application}")))?)?;
            d.step(Duration::ZERO, Event::Stored(id))?;
            let actions = respond(&mut d, id, b"commit1")?;
            assert_eq!(
                has_code(&actions, b"MSA|CA|"),
                matches!(accept, "AL" | "SU")
            );
            d.step(Duration::ZERO, Event::Interpreted(id))?;
            assert_eq!(
                has_code(&respond(&mut d, id, b"app1")?, b"MSA|AA|"),
                matches!(application, "AL" | "SU")
            );
        }
    }
    Ok(())
}

// HL7 MLLP R1 §1.2
#[test]
fn merged_messages_queue_and_overflow_is_reported() -> TestResult {
    let mut d = dialog()?;
    let raw = message("");
    let id = receipt(&feed(&mut d, &raw)?)?;
    assert!(
        !feed(&mut d, &raw)?
            .iter()
            .any(|a| matches!(a, Action::Store(_, _)))
    );
    assert!(
        feed(&mut d, &raw)?
            .iter()
            .any(|a| matches!(a, Action::QueueFull(r) if r == &raw))
    );
    d.step(Duration::ZERO, Event::StoreFailed(id))?;
    let next = respond(&mut d, id, b"negative")?;
    assert_ne!(receipt(&next)?, id);
    Ok(())
}

// HL7 2.5.1 §2.9
#[test]
fn deadline_precedes_late_commit() -> TestResult {
    let mut d = dialog()?;
    let id = receipt(&feed(&mut d, &message("AL|AL"))?)?;
    let out = d.step(Duration::from_secs(5), Event::Stored(id))?;
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::ResponseNeeded(_, AckCode::Ce)))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Refused(_))));
    Ok(())
}

// HL7 2.5.1 §2.9.2
#[test]
fn lost_or_late_ack_leaves_send_uncertain() -> TestResult {
    for disconnect in [false, true] {
        let mut d = dialog()?;
        let raw = message("");
        let payload = raw.get(1..raw.len().saturating_sub(2)).ok_or("payload")?;
        d.step(Duration::ZERO, Event::Send(payload, Expect::Acknowledgment))?;
        let out = if disconnect {
            d.step(Duration::ZERO, Event::Disconnected)?
        } else {
            d.step(Duration::from_secs(5), Event::Tick)?
        };
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(id, Outcome::Uncertain) if id == b"001"))
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Write(_))));
    }
    Ok(())
}

proptest! {
    // ADR 0003
    #[test]
    fn random_input_never_acks_positively(bytes in prop::collection::vec(any::<u8>(), 0..1000), events in prop::collection::vec(any::<u8>(), 0..100)) {
        let mut d = dialog()?;
        let id = receipt(&feed(&mut d, &message("AL|AL"))?).map_err(|e| TestCaseError::fail(e.to_string()))?;
        for byte in bytes { let out = d.step(Duration::ZERO, Event::Byte(byte))?; prop_assert!(!has_code(&out, b"MSA|AA|") && !has_code(&out, b"MSA|CA|")); }
        for event in events {
            let out = match event % 4 {
                0 => d.step(Duration::ZERO, Event::Interpreted(id))?,
                1 => d.step(Duration::ZERO, Event::StoreFailed(id))?,
                2 => d.step(Duration::ZERO, Event::Tick)?,
                _ => respond(&mut d, id, b"fresh").map_err(|e| TestCaseError::fail(e.to_string()))?,
            };
            prop_assert!(!has_code(&out, b"MSA|AA|") && !has_code(&out, b"MSA|CA|"));
        }
    }
}

// HL7 2.5.1 §2.15.8
#[test]
fn ack_codes_classified_and_mismatch_unmatched() -> TestResult {
    for (code, accepted) in [
        ("AA", true),
        ("AE", false),
        ("AR", false),
        ("CA", false),
        ("CE", false),
        ("CR", false),
    ] {
        let mut d = dialog()?;
        let raw = message("");
        d.step(
            Duration::ZERO,
            Event::Send(
                raw.get(1..raw.len().saturating_sub(2)).ok_or("payload")?,
                Expect::Acknowledgment,
            ),
        )?;
        let wrong = format!(
            "\x0bMSH|^~\\&|C|D|A|B|20261115||ACK^R01^ACK|reply|P|2.5.1\rMSA|{code}|other\r\x1c\r"
        );
        assert!(
            feed(&mut d, wrong.as_bytes())?
                .iter()
                .any(|a| matches!(a, Action::Unmatched(_)))
        );
        let right = wrong.rewrite("|other", "|001");
        let out = feed(&mut d, right.as_bytes())?;

        if matches!(code, "CA" | "CE" | "CR") {
            assert!(out.iter().any(|a| matches!(
                a,
                Action::Parked(
                    _,
                    Error::AckPhase {
                        expected: AckPhase::Application,
                        ..
                    }
                )
            )));
            assert!(!out.iter().any(|a| matches!(a, Action::Sent(_, _))));
            let completion = right.rewrite(&format!("MSA|{code}|"), "MSA|AA|");
            assert!(
                feed(&mut d, completion.as_bytes())?
                    .iter()
                    .any(|a| matches!(a, Action::Sent(_, Outcome::Accepted(AckCode::Aa))))
            );
            assert!(
                feed(&mut d, right.as_bytes())?
                    .iter()
                    .any(|a| matches!(a, Action::Unmatched(_)))
            );
            continue;
        }
        assert_eq!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(_, Outcome::Accepted(_)))),
            accepted
        );
        assert_eq!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(_, Outcome::Rejected(_)))),
            !accepted
        );
        // An idle receiver must never acknowledge an acknowledgment.
        assert!(
            feed(&mut d, right.as_bytes())?
                .iter()
                .any(|a| matches!(a, Action::Unmatched(_)))
        );
    }
    Ok(())
}

// HL7 MLLP R1 §1.2
#[test]
fn invalid_trailer_partial_eof_and_timeout() -> TestResult {
    for eof in [true, false] {
        let mut d = dialog()?;
        feed(&mut d, b"\x0bMSH|")?;
        let out = if eof {
            d.step(Duration::ZERO, Event::Disconnected)?
        } else {
            d.step(Duration::from_secs(5), Event::Tick)?
        };
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Rejected(_, raw) if raw == b"\x0bMSH|"))
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Write(_))));
    }
    let mut d = dialog()?;
    assert!(
        feed(&mut d, b"\x0bMSH\x1cX")?
            .iter()
            .any(|a| matches!(a, Action::Rejected(_, _)))
    );
    Ok(())
}

// HL7 2.5.1 §2.9.3
#[test]
fn negative_suppression_and_interpret_timeout() -> TestResult {
    for mode in ["AL|AL", "NE|NE", "ER|ER", "SU|SU"] {
        let mut d = dialog()?;
        let id = receipt(&feed(&mut d, &message(mode))?)?;
        d.step(Duration::ZERO, Event::StoreFailed(id))?;
        assert_eq!(
            has_code(&respond(&mut d, id, b"reject1")?, b"MSA|CE|"),
            matches!(mode, "AL|AL" | "ER|ER")
        );
    }
    let mut d = dialog()?;
    let id = receipt(&feed(&mut d, &message(""))?)?;
    d.step(Duration::ZERO, Event::Stored(id))?;
    let out = d.step(Duration::from_secs(5), Event::Interpreted(id))?;
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::ResponseNeeded(_, AckCode::Ar)))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Refused(_))));
    Ok(())
}

// ADR 0003
#[test]
fn malformed_message_committed_before_parking() -> TestResult {
    let mut d = dialog()?;
    let raw = b"\x0bmalformed\x1c\r";
    let actions = feed(&mut d, raw)?;
    let id = receipt(&actions)?;
    assert!(!actions.iter().any(|a| matches!(a, Action::Parked(_, _))));
    let actions = d.step(Duration::ZERO, Event::Stored(id))?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Parked(bytes, _) if bytes == raw))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
    Ok(())
}

// HL7 2.5.1 sections 2.9.2-2.9.3 pp.2-26-28: CA cannot finish application work.
// HL7 2.5.1 §2.9.2, §2.9.3
#[test]
fn sender_preserves_ack_phases() -> TestResult {
    let mut d = dialog()?;
    let raw = message("AL|AL");
    d.step(
        Duration::ZERO,
        Event::Send(
            raw.get(1..raw.len().saturating_sub(2)).ok_or("payload")?,
            Expect::Acknowledgment,
        ),
    )?;
    let reply = |code: &str| {
        format!("\x0bMSH|^~\\&|C|D|A|B|20261115||ACK^R01^ACK|reply|P|2.5.1\rMSA|{code}|001\r\x1c\r")
    };
    let early = feed(&mut d, reply("AA").as_bytes())?;
    assert!(early.iter().any(|a| matches!(
        a,
        Action::Parked(
            _,
            Error::AckPhase {
                expected: AckPhase::Accept,
                received: AckCode::Aa
            }
        )
    )));
    assert!(!early.iter().any(|a| matches!(a, Action::Sent(_, _))));
    let accepted = feed(&mut d, reply("CA").as_bytes())?;
    assert!(!accepted.iter().any(|a| matches!(a, Action::Sent(_, _))));
    let duplicate = feed(&mut d, reply("CA").as_bytes())?;
    assert!(duplicate.iter().any(|a| matches!(
        a,
        Action::Parked(
            _,
            Error::AckPhase {
                expected: AckPhase::Application,
                received: AckCode::Ca
            }
        )
    )));
    let completed = feed(&mut d, reply("AA").as_bytes())?;
    assert!(
        completed
            .iter()
            .any(|a| matches!(a, Action::Sent(id, Outcome::Accepted(AckCode::Aa)) if id == b"001"))
    );
    Ok(())
}

fn ack_reply(code: &str) -> Vec<u8> {
    format!("\x0bMSH|^~\\&|C|D|A|B|20261115||ACK^R01^ACK|reply|P|2.5.1\rMSA|{code}|001\r\x1c\r")
        .into_bytes()
}
fn sending(mode: &str) -> Result<(Dialog, Vec<Action>), Box<dyn std::error::Error>> {
    let mut dialog = dialog()?;
    let wire = message(mode);
    let actions = dialog.step(
        Duration::ZERO,
        Event::Send(
            wire.get(1..wire.len().saturating_sub(2)).ok_or("payload")?,
            Expect::Acknowledgment,
        ),
    )?;
    Ok((dialog, actions))
}

// HL7 2.5.1 sections 2.9.3, 2.15.9.15-16: AL/NE/ER/SU apply separately.
// HL7 2.5.1 §2.9.3
#[test]
fn sender_accept_and_application_condition_matrix() -> TestResult {
    for accept in ["AL", "NE", "ER", "SU"] {
        for application in ["AL", "NE", "ER", "SU"] {
            check_sender_conditions(accept, application)?;
        }
    }
    for (mode, wrong, right) in [
        ("SU|AL", "CE", "CA"),
        ("ER|AL", "CA", "CE"),
        ("NE|SU", "AE", "AA"),
        ("NE|ER", "AA", "AR"),
    ] {
        let (mut d, _) = sending(mode)?;
        let wire = ack_reply(wrong);
        let out = feed(&mut d, &wire)?;
        assert!(
            out.iter().any(
                |a| matches!(a, Action::Parked(raw, Error::AckCondition { .. }) if raw == &wire)
            )
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Sent(_, _))));
        let out = feed(&mut d, &ack_reply(right))?;
        if right == "CA" {
            assert!(out.iter().any(|a| matches!(a, Action::CommitAccepted(_))));
        } else {
            assert!(out.iter().any(|a| matches!(a, Action::Sent(_, _))));
        }
    }
    for code in ["CE", "CR"] {
        let (mut d, _) = sending("AL|AL")?;
        assert!(
            feed(&mut d, &ack_reply(code))?
                .iter()
                .any(|a| matches!(a, Action::Sent(_, Outcome::Rejected(_))))
        );
    }
    Ok(())
}

fn check_sender_conditions(accept: &str, application: &str) -> TestResult {
    let (mut d, initial) = sending(&format!("{accept}|{application}"))?;
    assert!(initial.iter().any(|a| matches!(a, Action::Write(_))));
    if accept == "NE" && application == "NE" {
        assert!(
            initial
                .iter()
                .any(|a| matches!(a, Action::Sent(id, Outcome::NotRequested) if id == b"001"))
        );
        assert!(initial.iter().any(|a| matches!(a, Action::Deadline(None))));
        return Ok(());
    }
    if matches!(accept, "AL" | "SU") {
        let out = feed(&mut d, &ack_reply("CA"))?;
        if application == "NE" {
            assert!(
                out.iter()
                    .any(|a| matches!(a, Action::Sent(_, Outcome::Accepted(AckCode::Ca))))
            );
            return Ok(());
        }
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::CommitAccepted(id) if id == b"001"))
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Sent(_, _))));
    }
    if application == "NE" {
        let out = d.step(Duration::from_secs(5), Event::Tick)?;
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(_, Outcome::Uncertain)))
        );
        return Ok(());
    }
    let code = if application == "ER" { "AE" } else { "AA" };
    let out = feed(&mut d, &ack_reply(code))?;
    if code == "AE" {
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(_, Outcome::Rejected(AckCode::Ae))))
        );
    } else {
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(_, Outcome::Accepted(AckCode::Aa))))
        );
    }
    Ok(())
}

// HL7 2.5.1 section 2.9.3: commit acceptance is retained, not application completion.
// HL7 2.5.1 §2.9.3
#[test]
fn sender_application_wait_never_resends() -> TestResult {
    for disconnect in [false, true] {
        let (mut d, _) = sending("AL|AL")?;
        let mut out = Vec::new();
        for byte in ack_reply("CA") {
            out.extend(d.step(Duration::from_secs(4), Event::Byte(byte))?);
        }
        assert!(out.iter().any(|a| matches!(a, Action::CommitAccepted(_))));
        assert!(out.iter().any(
            |a| matches!(a, Action::Deadline(Some(until)) if *until == Duration::from_secs(9))
        ));
        let out = d.step(Duration::from_secs(8), Event::Tick)?;
        assert!(!out.iter().any(|a| matches!(a, Action::Sent(_, _))));
        let at = if disconnect { 8 } else { 9 };
        let out = d.step(
            Duration::from_secs(at),
            if disconnect {
                Event::Disconnected
            } else {
                Event::Tick
            },
        )?;
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Sent(id, Outcome::Uncertain) if id == b"001"))
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Write(_))));
    }
    Ok(())
}
fn display_ack(id: &str, code: &str, version: &str) -> Vec<u8> {
    format!("\x0bMSH|^~\\&|A|B|C|D|20261115||ACK^Q03^ACK|ack1|P|{version}\rMSA|{code}|{id}\r\x1c\r")
        .into_bytes()
}

// HL7 2.3.1 §2.18; HL7 2.5.1 §5.10
#[test]
fn dsr_decisions_follow_durable_ack_commit() -> TestResult {
    for (id, code, version, expected, accepted) in [
        ("dsr1", "AA", "2.5.1", "HL7-DSR-ACK-AA", true),
        ("dsr1", "AE", "2.5.1", "HL7-DSR-ACK-AE", false),
        ("dsr1", "AR", "2.5.1", "HL7-DSR-ACK-AR", false),
        ("other", "AA", "2.5.1", "HL7-DSR-ACK-invalid", false),
        ("dsr1", "CA", "2.5.1", "HL7-DSR-ACK-invalid", false),
        ("dsr1", "AA", "2.3.1", "HL7-DSR-ACK-invalid-edition", false),
    ] {
        let mut d = dialog()?;
        d.enable_order_profile(
            analyte_hl7::Version::V251,
            analyte_hl7::query::Profile::Display,
        )?;
        d.await_display(
            Duration::ZERO,
            ControlId::new(b"dsr1")?,
            Duration::from_secs(2),
        )?;
        let actions = feed(&mut d, &display_ack(id, code, version))?;
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::DisplayOutcome { .. } | Action::Write(_)))
        );
        let token = receipt(&actions)?;
        let actions = d.step(Duration::from_secs(1), Event::Stored(token))?;
        assert!(actions.iter().any(|a| matches!(a, Action::DisplayOutcome { receipt: Some(r), accepted: actual, code, .. } if *r == token && *actual == accepted && *code == expected)));
        assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
    }
    Ok(())
}

// HL7 2.3.1 §2.18; ADR 0003
#[test]
fn late_dsr_commit_keeps_receipt_unconfirmed() -> TestResult {
    let mut d = dialog()?;
    d.enable_order_profile(
        analyte_hl7::Version::V251,
        analyte_hl7::query::Profile::Display,
    )?;
    d.await_display(
        Duration::ZERO,
        ControlId::new(b"dsr1")?,
        Duration::from_secs(2),
    )?;
    let token = receipt(&feed(&mut d, &display_ack("dsr1", "AA", "2.5.1"))?)?;
    let actions = d.step(Duration::from_secs(2), Event::Stored(token))?;
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(
                a,
                Action::DisplayOutcome {
                    accepted: false,
                    code: "HL7-DSR-ACK-timeout",
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ReceiptRetained(id) if *id == token))
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Write(_) | Action::DisplayOutcome { accepted: true, .. }
    )));
    assert!(!d.orders_busy());
    assert!(
        !d.step(Duration::from_secs(3), Event::Tick)?
            .iter()
            .any(|a| matches!(a, Action::DisplayOutcome { .. }))
    );
    Ok(())
}

// HL7 MLLP R1 §1.2
#[test]
fn queries_share_decoder_and_first_byte_time() -> TestResult {
    let mut d = dialog()?;
    d.enable_order_profile(
        analyte_hl7::Version::V251,
        analyte_hl7::query::Profile::Display,
    )?;
    let query = String::from_utf8(message(""))?
        .rewrite("ORU^R01^ORU_R01", "QRY^Q02^QRY_Q02")
        .into_bytes();
    let first = receipt(&feed(&mut d, &query)?)?;
    for byte in &query {
        let actions = d.step(Duration::from_secs(1), Event::Byte(*byte))?;
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Store(..) | Action::Query { .. }))
        );
    }
    let actions = d.step(Duration::from_secs(2), Event::Stored(first))?;
    assert!(actions.iter().any(
        |a| matches!(a, Action::Query { receipt, started: Duration::ZERO, .. } if *receipt == first)
    ));
    let second = receipt(&actions)?;
    assert_ne!(first, second);
    let actions = d.step(Duration::from_secs(2), Event::Stored(second))?;
    assert!(actions.iter().any(|a| matches!(a, Action::Query { receipt, started, .. } if *receipt == second && *started == Duration::from_secs(1))));
    Ok(())
}
// HL7 2.3.1 §2.18
#[test]
fn qck_precedes_dsr_and_failed_qck_issues_nothing() -> TestResult {
    for (fail, now, expected) in [
        (true, 1, "HL7-QCK-write-failed"),
        (false, 2, "HL7-query-deadline-before-DSR"),
    ] {
        let mut d = dialog()?;
        d.enable_order_profile(
            analyte_hl7::Version::V251,
            analyte_hl7::query::Profile::Display,
        )?;
        let payload = analyte_framing::hl7::payload(&message(""))
            .ok_or("synthetic envelope")?
            .to_vec();
        let qck = analyte_hl7::Message::parse(&payload, Options::default())?;
        let dsr = qck.clone();
        let reply = DisplayReply {
            acknowledgment: qck,
            displays: [dsr].into(),
            query_deadline: Duration::from_secs(2),
            failure_deadline: Duration::from_secs(5),
            acknowledgment_wait: Duration::from_secs(3),
        };
        let actions = d.step(Duration::ZERO, Event::QueryReply(&reply))?;
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::QueryWrite {
                part: ReplyPart::Qck,
                ..
            }
        )));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::QueryWrite {
                part: ReplyPart::Dsr,
                ..
            }
        )));
        let event = if fail {
            Event::QueryWriteFailed(ReplyPart::Qck)
        } else {
            Event::QueryWritten(ReplyPart::Qck)
        };
        let actions = d.step(Duration::from_secs(now), event)?;
        assert!(actions.iter().any(|a| matches!(a, Action::DisplayOutcome { code, accepted: false, .. } if *code == expected)));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::QueryWrite { .. }))
        );
    }
    Ok(())
}

// HL7 2.3.1 §2.18
#[test]
fn dsr_issuance_starts_ack_budget_without_retry() -> TestResult {
    let mut d = dialog()?;
    d.enable_order_profile(
        analyte_hl7::Version::V251,
        analyte_hl7::query::Profile::Display,
    )?;
    let payload = analyte_framing::hl7::payload(&message(""))
        .ok_or("synthetic envelope")?
        .to_vec();
    let qck = analyte_hl7::Message::parse(&payload, Options::default())?;
    let reply = DisplayReply {
        acknowledgment: qck.clone(),
        displays: [qck].into(),
        query_deadline: Duration::from_secs(2),
        failure_deadline: Duration::from_secs(5),
        acknowledgment_wait: Duration::from_secs(3),
    };
    d.step(Duration::ZERO, Event::QueryReply(&reply))?;
    let actions = d.step(Duration::from_secs(1), Event::QueryWritten(ReplyPart::Qck))?;
    assert!(actions.iter().any(|a| matches!(a, Action::QueryWrite { part: ReplyPart::Dsr, until, .. } if *until == Duration::from_secs(2))));
    assert_eq!(d.next_deadline(), Some(Duration::from_secs(4)));
    let actions = d.step(
        Duration::from_secs(1),
        Event::QueryWriteFailed(ReplyPart::Dsr),
    )?;
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::DisplayOutcome {
            code: "HL7-DSR-write-failed",
            accepted: false,
            ..
        }
    )));
    assert!(
        !d.step(Duration::from_secs(5), Event::Tick)?
            .iter()
            .any(|a| matches!(a, Action::QueryWrite { .. }))
    );
    Ok(())
}

// HL7 2.9.1 section 2.8.3.2: NE/AL still requires a separate channel.
// HL7 2.9.1 §2.8.3.2
#[test]
fn enhanced_application_ack_needs_separate_channel() -> TestResult {
    for mode in ["NE|AL", "AL|AL", "AL|ER", "AL|SU"] {
        let mut dialog = dialog()?;
        let raw = String::from_utf8(message(mode))?
            .rewrite("2.5.1", "2.9.1")
            .into_bytes();
        let actions = feed(&mut dialog, &raw)?;
        let id = receipt(&actions)?;
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Write(_)))
        );
        let actions = dialog.step(Duration::ZERO, Event::Stored(id))?;
        assert!(actions.iter().any(|action|matches!(action, Action::Parked(bytes, Error::SeparateApplicationChannel) if bytes==&raw)));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Write(_) | Action::Interpret(..)))
        );
    }
    Ok(())
}
