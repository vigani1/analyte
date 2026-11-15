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
    format!("\x0bMSH|^~\\&|A|B|C|D|20261115||ORU^R01|001|P|2.5.1|||{mode}\r\x1c\r").into_bytes()
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
        ("CA", true),
        ("AE", false),
        ("AR", false),
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
            "\x0bMSH|^~\\&|C|D|A|B|20261115||ACK^R01|reply|P|2.5.1\rMSA|{code}|other\r\x1c\r"
        );
        assert!(
            feed(&mut d, wrong.as_bytes())?
                .iter()
                .any(|a| matches!(a, Action::Unmatched(_)))
        );
        let right = wrong.replace("|other", "|001");
        let out = feed(&mut d, right.as_bytes())?;
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
