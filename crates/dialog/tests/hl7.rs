//! Synthetic HL7 dialog conversations and safety properties.
use analyte_dialog::hl7::*;
use analyte_hl7::{AckCode, AckError, ControlId, Options, Response};
use proptest::prelude::*;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn dialog() -> Result<Dialog, Error> {
    Ok(Dialog::new(
        Config::new(Options::default(), Duration::from_secs(5), 1)?,
        42,
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
fn receipt(actions: &[Action]) -> Result<ReceiptId, Box<dyn std::error::Error>> {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Commit(id, _) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or_else(|| "no commit".into())
}
fn respond(
    d: &mut Dialog,
    id: ReceiptId,
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
        .any(|a| matches!(a, Action::Write(bytes) if bytes.windows(code.len()).any(|w| w == code)))
}

#[test]
fn hl7_251_2_9_2_aa_only_after_store_and_interpret() -> TestResult {
    let mut d = dialog()?;
    let id = receipt(&feed(&mut d, &message(""))?)?;
    assert!(
        d.step(Duration::ZERO, Event::Interpreted(id))?
            .iter()
            .any(|a| matches!(a, Action::Refused(_)))
    );
    assert!(
        d.step(Duration::ZERO, Event::Committed(id))?
            .iter()
            .any(|a| matches!(a, Action::Interpret(_, _)))
    );
    d.step(Duration::ZERO, Event::Interpreted(id))?;
    let actions = respond(&mut d, id, b"response1")?;
    assert!(has_code(&actions, b"MSA|AA|001\r"));
    assert!(has_code(&actions, b"|response1|"));
    Ok(())
}

#[test]
fn hl7_251_2_9_2_failed_store_and_interpretation() -> TestResult {
    for stored in [false, true] {
        let mut d = dialog()?;
        let id = receipt(&feed(&mut d, &message(""))?)?;
        if stored {
            d.step(Duration::ZERO, Event::Committed(id))?;
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
                    false,
                ),
            )?;
        } else {
            d.step(Duration::ZERO, Event::CommitFailed(id))?;
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

#[test]
fn hl7_251_2_9_3_independent_conditions() -> TestResult {
    for accept in ["AL", "NE", "ER", "SU"] {
        for application in ["AL", "NE", "ER", "SU"] {
            let mut d = dialog()?;
            let id = receipt(&feed(&mut d, &message(&format!("{accept}|{application}")))?)?;
            d.step(Duration::ZERO, Event::Committed(id))?;
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

#[test]
fn mllp_r1_1_2_split_merged_queue_and_overflow() -> TestResult {
    let mut d = dialog()?;
    let raw = message("");
    let id = receipt(&feed(&mut d, &raw)?)?;
    assert!(
        !feed(&mut d, &raw)?
            .iter()
            .any(|a| matches!(a, Action::Commit(_, _)))
    );
    assert!(
        feed(&mut d, &raw)?
            .iter()
            .any(|a| matches!(a, Action::QueueFull(r) if r == &raw))
    );
    d.step(Duration::ZERO, Event::CommitFailed(id))?;
    let next = respond(&mut d, id, b"negative")?;
    assert_ne!(receipt(&next)?, id);
    Ok(())
}

#[test]
fn hl7_251_2_9_deadline_precedes_late_commit() -> TestResult {
    let mut d = dialog()?;
    let id = receipt(&feed(&mut d, &message("AL|AL"))?)?;
    let out = d.step(Duration::from_secs(5), Event::Committed(id))?;
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::ResponseNeeded(_, AckCode::Ce)))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Refused(_))));
    Ok(())
}

#[test]
fn hl7_251_2_9_2_sender_correlation_and_uncertainty() -> TestResult {
    for disconnect in [false, true] {
        let mut d = dialog()?;
        let raw = message("");
        let payload = raw.get(1..raw.len().saturating_sub(2)).ok_or("payload")?;
        d.step(Duration::ZERO, Event::Send(payload))?;
        let out = if disconnect {
            d.step(Duration::ZERO, Event::Disconnect)?
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
    #[test]
    fn adr_0003_random_bytes_and_uncommitted_events_never_positive(bytes in prop::collection::vec(any::<u8>(), 0..1000), events in prop::collection::vec(any::<u8>(), 0..100)) {
        let mut d = dialog()?;
        let id = receipt(&feed(&mut d, &message("AL|AL"))?).map_err(|e| TestCaseError::fail(e.to_string()))?;
        for byte in bytes { let out = d.step(Duration::ZERO, Event::Byte(byte))?; prop_assert!(!has_code(&out, b"MSA|AA|") && !has_code(&out, b"MSA|CA|")); }
        for event in events {
            let out = match event % 4 {
                0 => d.step(Duration::ZERO, Event::Interpreted(id))?,
                1 => d.step(Duration::ZERO, Event::CommitFailed(id))?,
                2 => d.step(Duration::ZERO, Event::Tick)?,
                _ => respond(&mut d, id, b"fresh").map_err(|e| TestCaseError::fail(e.to_string()))?,
            };
            prop_assert!(!has_code(&out, b"MSA|AA|") && !has_code(&out, b"MSA|CA|"));
        }
    }
}

#[test]
fn hl7_251_2_15_8_all_codes_and_mismatched_ack() -> TestResult {
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
            Event::Send(raw.get(1..raw.len().saturating_sub(2)).ok_or("payload")?),
        )?;
        let wrong = format!(
            "\x0bMSH|^~\\&|C|D|A|B|20261115||ACK^R01^ACK|reply|P|2.5.1\rMSA|{code}|other\r\x1c\r"
        );
        assert!(
            feed(&mut d, wrong.as_bytes())?
                .iter()
                .any(|a| matches!(a, Action::Unmatched(_)))
        );
        let right = wrong.replace("|other", "|001");
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
            let completion = right.replace(&format!("MSA|{code}|"), "MSA|AA|");
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

#[test]
fn mllp_r1_1_2_invalid_trailer_partial_eof_and_timeout() -> TestResult {
    for eof in [true, false] {
        let mut d = dialog()?;
        feed(&mut d, b"\x0bMSH|")?;
        let out = if eof {
            d.step(Duration::ZERO, Event::Disconnect)?
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

#[test]
fn hl7_251_2_9_3_negative_suppression_and_interpret_timeout() -> TestResult {
    for mode in ["AL|AL", "NE|NE", "ER|ER", "SU|SU"] {
        let mut d = dialog()?;
        let id = receipt(&feed(&mut d, &message(mode))?)?;
        d.step(Duration::ZERO, Event::CommitFailed(id))?;
        assert_eq!(
            has_code(&respond(&mut d, id, b"reject1")?, b"MSA|CE|"),
            matches!(mode, "AL|AL" | "ER|ER")
        );
    }
    let mut d = dialog()?;
    let id = receipt(&feed(&mut d, &message(""))?)?;
    d.step(Duration::ZERO, Event::Committed(id))?;
    let out = d.step(Duration::from_secs(5), Event::Interpreted(id))?;
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::ResponseNeeded(_, AckCode::Ar)))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Refused(_))));
    Ok(())
}

#[test]
fn adr_0003_malformed_message_is_committed_before_parking() -> TestResult {
    let mut d = dialog()?;
    let raw = b"\x0bmalformed\x1c\r";
    let actions = feed(&mut d, raw)?;
    let id = receipt(&actions)?;
    assert!(!actions.iter().any(|a| matches!(a, Action::Parked(_, _))));
    let actions = d.step(Duration::ZERO, Event::Committed(id))?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Parked(bytes, _) if bytes == raw))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
    Ok(())
}

// HL7 2.5.1 sections 2.9.2-2.9.3 pp.2-26-28: CA cannot finish application work.
#[test]
fn hl7_251_2_9_2_2_9_3_sender_preserves_ack_phases() -> TestResult {
    let mut d = dialog()?;
    let raw = message("AL|AL");
    d.step(
        Duration::ZERO,
        Event::Send(raw.get(1..raw.len().saturating_sub(2)).ok_or("payload")?),
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
        Event::Send(wire.get(1..wire.len().saturating_sub(2)).ok_or("payload")?),
    )?;
    Ok((dialog, actions))
}

// HL7 2.5.1 sections 2.9.3, 2.15.9.15-16: AL/NE/ER/SU apply separately.
#[test]
fn hl7_251_2_9_3_sender_acceptance_and_application_condition_matrix() -> TestResult {
    for accept in ["AL", "NE", "ER", "SU"] {
        for application in ["AL", "NE", "ER", "SU"] {
            let (mut d, initial) = sending(&format!("{accept}|{application}"))?;
            assert!(initial.iter().any(|a| matches!(a, Action::Write(_))));
            if accept == "NE" && application == "NE" {
                assert!(
                    initial.iter().any(
                        |a| matches!(a, Action::Sent(id, Outcome::NotRequested) if id == b"001")
                    )
                );
                assert!(initial.iter().any(|a| matches!(a, Action::Deadline(None))));
                continue;
            }
            if matches!(accept, "AL" | "SU") {
                let out = feed(&mut d, &ack_reply("CA"))?;
                if application == "NE" {
                    assert!(
                        out.iter()
                            .any(|a| matches!(a, Action::Sent(_, Outcome::Accepted(AckCode::Ca))))
                    );
                    continue;
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
                continue;
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

// HL7 2.5.1 section 2.9.3: commit acceptance is retained, not application completion.
#[test]
fn hl7_251_2_9_3_sender_application_wait_is_bounded_and_never_resends() -> TestResult {
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
                Event::Disconnect
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

#[test]
fn hl7_231_2_18_251_5_10_dsr_decisions_follow_durable_ack_commit() -> TestResult {
    for (id, code, version, expected, accepted) in [
        ("dsr1", "AA", "2.5.1", "HL7-DSR-ACK-AA", true),
        ("dsr1", "AE", "2.5.1", "HL7-DSR-ACK-AE", false),
        ("dsr1", "AR", "2.5.1", "HL7-DSR-ACK-AR", false),
        ("other", "AA", "2.5.1", "HL7-DSR-ACK-invalid", false),
        ("dsr1", "CA", "2.5.1", "HL7-DSR-ACK-invalid", false),
        ("dsr1", "AA", "2.3.1", "HL7-DSR-ACK-invalid-edition", false),
    ] {
        let mut d = dialog()?;
        d.enable_orders(analyte_hl7::Version::V251)?;
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
        let actions = d.step(Duration::from_secs(1), Event::Committed(token))?;
        assert!(actions.iter().any(|a| matches!(a, Action::DisplayOutcome { receipt: Some(r), accepted: actual, code, .. } if *r == token && *actual == accepted && *code == expected)));
        assert!(!actions.iter().any(|a| matches!(a, Action::Write(_))));
    }
    Ok(())
}

#[test]
fn hl7_231_2_18_adr0003_late_dsr_commit_retains_receipt_without_confirming_work() -> TestResult {
    let mut d = dialog()?;
    d.enable_orders(analyte_hl7::Version::V251)?;
    d.await_display(
        Duration::ZERO,
        ControlId::new(b"dsr1")?,
        Duration::from_secs(2),
    )?;
    let token = receipt(&feed(&mut d, &display_ack("dsr1", "AA", "2.5.1"))?)?;
    let actions = d.step(Duration::from_secs(2), Event::Committed(token))?;
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

#[test]
fn mllp_r1_1_2_queries_share_decoder_and_keep_queued_first_byte_time() -> TestResult {
    let mut d = dialog()?;
    d.enable_orders(analyte_hl7::Version::V251)?;
    let query = String::from_utf8(message(""))?
        .replace("ORU^R01^ORU_R01", "QRY^Q02^QRY_Q02")
        .into_bytes();
    let first = receipt(&feed(&mut d, &query)?)?;
    for byte in &query {
        let actions = d.step(Duration::from_secs(1), Event::Byte(*byte))?;
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Commit(..) | Action::Query { .. }))
        );
    }
    let actions = d.step(Duration::from_secs(2), Event::Committed(first))?;
    assert!(actions.iter().any(
        |a| matches!(a, Action::Query { receipt, started: Duration::ZERO, .. } if *receipt == first)
    ));
    let second = receipt(&actions)?;
    assert_ne!(first, second);
    let actions = d.step(Duration::from_secs(2), Event::Committed(second))?;
    assert!(actions.iter().any(|a| matches!(a, Action::Query { receipt, started, .. } if *receipt == second && *started == Duration::from_secs(1))));
    Ok(())
}
