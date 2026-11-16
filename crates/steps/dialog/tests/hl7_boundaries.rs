//! Tests HL7 custody callbacks and absolute deadlines.
//! HL7 2.5.1 §§2.9.2–2.9.3 define response responsibility. Receipt ordinals,
//! callback phases, a non-extending timeout and diagnostic byte ceilings are
//! Analyte host policies; MLLP R1 does not prescribe a timeout duration.
use analyte_dialog::hl7::{Action, Config, Dialog, Error, Event, ReceiptId};
use analyte_hl7::{AckCode, AckError, ControlId, Options, Response};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;
// HL7 2.5.1 §2.15.9.9 and §2.A.44 require the message structure component.
const MESSAGE: &[u8] = b"MSH|^~\\&|A|B|C|D|20261115||ORU^R01^ORU_R01|001|P|2.5.1\r";

fn dialog(epoch: u64) -> Result<Dialog, Error> {
    Ok(Dialog::new(
        Config::new(Options::default(), Duration::from_secs(5), 1)?,
        epoch,
    ))
}
fn feed(d: &mut Dialog, bytes: &[u8]) -> Result<Vec<Action>, Error> {
    let mut out = Vec::new();
    for byte in bytes {
        out.extend(d.step(Duration::ZERO, Event::Byte(*byte))?);
    }
    Ok(out)
}
fn receive(d: &mut Dialog, payload: &[u8]) -> Result<ReceiptId, Box<dyn std::error::Error>> {
    let raw = [b"\x0b".as_slice(), payload, b"\x1c\r"].concat();
    feed(d, &raw)?
        .iter()
        .find_map(|a| {
            if let Action::Commit(id, bytes) = a {
                assert_eq!(bytes, &raw);
                Some(*id)
            } else {
                None
            }
        })
        .ok_or_else(|| "expected commit".into())
}
fn response(d: &mut Dialog, id: ReceiptId) -> Result<Vec<Action>, Box<dyn std::error::Error>> {
    Ok(d.step(
        Duration::ZERO,
        Event::Respond(
            id,
            Response {
                control_id: ControlId::new(b"fresh")?,
                timestamp: b"20261115",
            },
        ),
    )?)
}

#[test]
fn adr0003_hl7_receipt_namespace_and_ordinal_survive_valid_and_malformed_input() -> TestResult {
    for epoch in [42, 99] {
        let mut d = dialog(epoch)?;
        for (ordinal, payload) in [MESSAGE, b"malformed", MESSAGE].iter().enumerate() {
            let id = receive(&mut d, payload)?;
            assert_eq!(id.epoch(), epoch);
            assert_eq!(id.ordinal(), u64::try_from(ordinal)?);
            let out = d.step(Duration::ZERO, Event::CommitFailed(id))?;
            if *payload == MESSAGE {
                assert!(
                    out.iter()
                        .any(|a| matches!(a, Action::ResponseNeeded(i, AckCode::Ar) if *i == id))
                );
                response(&mut d, id)?;
            } else {
                assert!(out.iter().any(|a| matches!(a, Action::Parked(_, _))));
            }
        }
    }
    Ok(())
}

#[test]
fn rule7_mllp_r1_partial_input_does_not_extend_absolute_deadline() -> TestResult {
    let mut d = dialog(42)?;
    let first = d.step(Duration::ZERO, Event::Byte(11))?;
    assert!(
        first
            .iter()
            .any(|a| matches!(a, Action::Deadline(Some(t)) if *t == Duration::from_secs(5)))
    );
    let later = d.step(Duration::from_secs(4), Event::Byte(b'M'))?;
    assert!(
        later
            .iter()
            .any(|a| matches!(a, Action::Deadline(Some(t)) if *t == Duration::from_secs(5)))
    );
    let expired = d.step(Duration::from_secs(5), Event::Tick)?;
    assert!(
        expired
            .iter()
            .any(|a| matches!(a, Action::Rejected(_, raw) if raw == b"\x0bM"))
    );
    Ok(())
}

#[test]
fn adr0010_hl7_251_2_9_send_cannot_replace_active_custody_or_response() -> TestResult {
    for phase in [
        "partial",
        "commit",
        "malformed",
        "interpret",
        "respond",
        "sending",
    ] {
        let mut d = dialog(42)?;
        match phase {
            "partial" => {
                feed(&mut d, b"\x0bMSH")?;
            }
            "malformed" => {
                receive(&mut d, b"malformed")?;
            }
            "sending" => {
                d.step(Duration::ZERO, Event::Send(MESSAGE))?;
            }
            "commit" | "interpret" | "respond" => {
                let id = receive(&mut d, MESSAGE)?;
                if phase != "commit" {
                    d.step(Duration::ZERO, Event::Committed(id))?;
                }
                if phase == "respond" {
                    d.step(Duration::ZERO, Event::Interpreted(id))?;
                }
            }
            _ => unreachable!("fixed test cases"),
        }
        let refused = d.step(Duration::ZERO, Event::Send(MESSAGE))?;
        assert!(
            refused
                .iter()
                .any(|a| matches!(a, Action::Refused(Error::Busy))),
            "{phase}: {refused:?}"
        );
        assert!(!refused.iter().any(|a| matches!(a, Action::Write(_))));
    }
    Ok(())
}

fn diagnostic<'a>(segment: &'a [u8], code: &'a [u8], application_code: &'a [u8]) -> AckError<'a> {
    AckError {
        segment,
        occurrence: 1,
        field: 5,
        code,
        application_code,
    }
}

#[test]
fn adr0003_0010_hl7_251_2_9_callbacks_require_matching_receipt_and_phase() -> TestResult {
    let wrong = receive(&mut dialog(99)?, MESSAGE)?;
    for phase in ["commit", "interpret", "respond"] {
        for callback in ["committed", "failed", "interpreted", "invalid", "respond"] {
            for matching in [false, true] {
                let mut d = dialog(42)?;
                let id = receive(&mut d, MESSAGE)?;
                if phase != "commit" {
                    d.step(Duration::ZERO, Event::Committed(id))?;
                }
                if phase == "respond" {
                    d.step(Duration::ZERO, Event::Interpreted(id))?;
                }
                let reply = if matching { id } else { wrong };
                let event = match callback {
                    "committed" => Event::Committed(reply),
                    "failed" => Event::CommitFailed(reply),
                    "interpreted" => Event::Interpreted(reply),
                    "invalid" => {
                        Event::NotInterpretable(reply, diagnostic(b"OBX", b"102", b""), false)
                    }
                    "respond" => Event::Respond(
                        reply,
                        Response {
                            control_id: ControlId::new(b"fresh")?,
                            timestamp: b"20261115",
                        },
                    ),
                    _ => unreachable!("fixed test cases"),
                };
                let out = d.step(Duration::ZERO, event)?;
                let allowed = matching
                    && matches!(
                        (phase, callback),
                        ("commit", "committed" | "failed")
                            | ("interpret", "interpreted" | "invalid")
                            | ("respond", "respond")
                    );
                assert_eq!(
                    out.iter()
                        .any(|a| matches!(a, Action::Refused(Error::Stale))),
                    !allowed,
                    "{phase}/{callback}/matching={matching}: {out:?}"
                );
                if !allowed {
                    assert!(!out.iter().any(|a| matches!(
                        a,
                        Action::Write(_) | Action::Interpret(..) | Action::ResponseNeeded(..)
                    )));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn rule7_host_error_metadata_bounds_refuse_without_losing_pending_receipt() -> TestResult {
    let application = [b'X'; 65];
    for (segment, code, app, accepted) in [
        (
            b"OBX".as_slice(),
            b"102".as_slice(),
            &application[..64],
            true,
        ),
        (b"OBXX".as_slice(), b"102".as_slice(), b"".as_slice(), false),
        (b"OBX".as_slice(), b"1022".as_slice(), b"".as_slice(), false),
        (
            b"OBX".as_slice(),
            b"102".as_slice(),
            application.as_slice(),
            false,
        ),
    ] {
        let mut d = dialog(42)?;
        let id = receive(&mut d, MESSAGE)?;
        d.step(Duration::ZERO, Event::Committed(id))?;
        let out = d.step(
            Duration::ZERO,
            Event::NotInterpretable(id, diagnostic(segment, code, app), false),
        )?;
        assert_eq!(
            out.iter()
                .any(|a| matches!(a, Action::Refused(Error::Configuration))),
            !accepted
        );
        if accepted {
            assert!(
                out.iter()
                    .any(|a| matches!(a, Action::ResponseNeeded(i, AckCode::Ae) if *i == id))
            );
        } else {
            let recovered = d.step(Duration::ZERO, Event::Interpreted(id))?;
            assert!(
                recovered
                    .iter()
                    .any(|a| matches!(a, Action::ResponseNeeded(i, AckCode::Aa) if *i == id))
            );
        }
    }
    Ok(())
}
