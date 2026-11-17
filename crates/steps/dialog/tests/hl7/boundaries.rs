//! Tests HL7 custody callbacks and absolute deadlines.
//! HL7 2.5.1 §§2.9.2–2.9.3 define response responsibility. Receipt ordinals,
//! callback phases, a non-extending timeout and diagnostic byte ceilings are
//! Analyte host policies; MLLP R1 does not prescribe a timeout duration.
use analyte_dialog::hl7::{Action, Config, Dialog, Error, Event, ExchangeId, Expect};
use analyte_hl7::{AckCode, AckError, ControlId, Options, Response};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;
// HL7 2.5.1 §2.15.9.9 and §2.A.44 require the message structure component.
const MESSAGE: &[u8] = b"MSH|^~\\&|A|B|C|D|20261115||ORU^R01^ORU_R01|001|P|2.5.1\r";

fn dialog(epoch: u64) -> Result<Dialog, Error> {
    Ok(Dialog::new(
        Config::new(Options::default(), Duration::from_secs(5), 1)?,
        analyte_dialog::Epoch(u128::from(epoch)),
    ))
}
fn feed(d: &mut Dialog, bytes: &[u8]) -> Result<Vec<Action>, Error> {
    let mut out = Vec::new();
    for byte in bytes {
        out.extend(d.step(Duration::ZERO, Event::Byte(*byte))?);
    }
    Ok(out)
}
fn receive(d: &mut Dialog, payload: &[u8]) -> Result<ExchangeId, Box<dyn std::error::Error>> {
    let raw = [b"\x0b".as_slice(), payload, b"\x1c\r"].concat();
    feed(d, &raw)?
        .iter()
        .find_map(|a| {
            if let Action::Store(id, bytes) = a {
                assert_eq!(bytes, &raw);
                Some(*id)
            } else {
                None
            }
        })
        .ok_or_else(|| "expected commit".into())
}
fn response(d: &mut Dialog, id: ExchangeId) -> Result<Vec<Action>, Box<dyn std::error::Error>> {
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
            assert_eq!(id.epoch(), u128::from(epoch));
            assert_eq!(id.ordinal(), u64::try_from(ordinal)?);
            let out = d.step(Duration::ZERO, Event::StoreFailed(id))?;
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
                d.step(Duration::ZERO, Event::Send(MESSAGE, Expect::Acknowledgment))?;
            }
            "commit" | "interpret" | "respond" => {
                let id = receive(&mut d, MESSAGE)?;
                if phase != "commit" {
                    d.step(Duration::ZERO, Event::Stored(id))?;
                }
                if phase == "respond" {
                    d.step(Duration::ZERO, Event::Interpreted(id))?;
                }
            }
            _ => unreachable!("fixed test cases"),
        }
        let refused = d.step(Duration::ZERO, Event::Send(MESSAGE, Expect::Acknowledgment))?;
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
                    d.step(Duration::ZERO, Event::Stored(id))?;
                }
                if phase == "respond" {
                    d.step(Duration::ZERO, Event::Interpreted(id))?;
                }
                let reply = if matching { id } else { wrong };
                let event = match callback {
                    "committed" => Event::Stored(reply),
                    "failed" => Event::StoreFailed(reply),
                    "interpreted" => Event::Interpreted(reply),
                    "invalid" => Event::NotInterpretable(
                        reply,
                        diagnostic(b"OBX", b"102", b""),
                        analyte_dialog::hl7::AckChoice::ContentError,
                    ),
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
        d.step(Duration::ZERO, Event::Stored(id))?;
        let out = d.step(
            Duration::ZERO,
            Event::NotInterpretable(
                id,
                diagnostic(segment, code, app),
                analyte_dialog::hl7::AckChoice::ContentError,
            ),
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

// HL7 2.5.1 §4.4.1–§4.4.2 define ORM^O01 and its ORR^O02 response.
const ORDER: &[u8] = b"MSH|^~\\&|A|B|C|D|20261115||ORM^O01^ORM_O01|o1|P|2.5.1\rORC|NW|p1\r";
const ORDER_RESPONSE: &[u8] =
    b"MSH|^~\\&|C|D|A|B|20261115||ORR^O02^ORR_O02|r1|P|2.5.1\rMSA|AA|o1\r";

fn writes(actions: &[Action]) -> Vec<&analyte_dialog::Write> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Write(write) = a {
                Some(write)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn hl7_251_4_4_2_rule8_order_send_issues_once_within_its_response_budget() -> TestResult {
    let mut d = dialog(42)?;
    d.enable_orders(analyte_hl7::Version::V251)?;
    let wait = Duration::from_secs(2);
    let sent = d.step(
        Duration::from_secs(1),
        Event::Send(ORDER, Expect::OrderResponse(wait)),
    )?;
    let framed = [b"\x0b".as_slice(), ORDER, b"\x1c\r"].concat();
    let issued = writes(&sent);
    assert_eq!(issued.len(), 1, "{sent:?}");
    assert_eq!(
        issued.first().map(|w| w.bytes.as_slice()),
        Some(framed.as_slice())
    );
    assert_eq!(
        issued.first().and_then(|w| w.until),
        Some(Duration::from_secs(3))
    );
    assert!(
        sent.iter()
            .any(|a| matches!(a, Action::Deadline(Some(d)) if *d == Duration::from_secs(3)))
    );
    assert!(d.orders_busy());
    let again = d.step(
        Duration::from_secs(1),
        Event::Send(ORDER, Expect::OrderResponse(wait)),
    )?;
    assert!(
        again
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Busy)))
    );
    assert!(writes(&again).is_empty());

    let raw = [b"\x0b".as_slice(), ORDER_RESPONSE, b"\x1c\r"].concat();
    let mut received = Vec::new();
    for byte in &raw {
        received.extend(d.step(Duration::from_secs(1), Event::Byte(*byte))?);
    }
    let token = received
        .iter()
        .find_map(|a| {
            if let Action::Store(id, bytes) = a
                && *bytes == raw
            {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or("order response not stored first")?;
    let reported = d.step(Duration::from_secs(2), Event::Stored(token))?;
    assert!(reported.iter().any(|a| matches!(a, Action::OrderResponse { receipt, message } if *receipt == token && message.raw() == ORDER_RESPONSE)));
    assert!(writes(&reported).is_empty());
    assert!(!d.orders_busy());
    Ok(())
}

#[test]
fn hl7_251_4_4_2_rule8_order_response_timeout_never_resends() -> TestResult {
    let mut d = dialog(42)?;
    d.enable_orders(analyte_hl7::Version::V251)?;
    d.step(
        Duration::ZERO,
        Event::Send(ORDER, Expect::OrderResponse(Duration::from_secs(2))),
    )?;
    let early = d.step(Duration::from_millis(1_999), Event::Tick)?;
    assert!(
        !early
            .iter()
            .any(|a| matches!(a, Action::OrderResponseTimeout))
    );
    let expired = d.step(Duration::from_secs(2), Event::Tick)?;
    assert_eq!(
        expired
            .iter()
            .filter(|a| matches!(a, Action::OrderResponseTimeout))
            .count(),
        1
    );
    assert!(writes(&expired).is_empty());
    assert!(!d.orders_busy());
    assert!(writes(&d.step(Duration::from_secs(9), Event::Tick)?).is_empty());
    Ok(())
}

#[test]
fn hl7_order_send_refuses_disabled_zero_budget_and_partial_input_without_writing() -> TestResult {
    let mut disabled = dialog(42)?;
    let refused = disabled.step(
        Duration::ZERO,
        Event::Send(ORDER, Expect::OrderResponse(Duration::from_secs(2))),
    )?;
    assert!(
        refused
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Configuration)))
    );
    assert!(writes(&refused).is_empty());

    let mut zero = dialog(42)?;
    zero.enable_orders(analyte_hl7::Version::V251)?;
    let refused = zero.step(
        Duration::ZERO,
        Event::Send(ORDER, Expect::OrderResponse(Duration::ZERO)),
    )?;
    assert!(
        refused
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Configuration)))
    );
    assert!(writes(&refused).is_empty());
    assert!(!zero.orders_busy());

    let mut partial = dialog(42)?;
    partial.enable_orders(analyte_hl7::Version::V251)?;
    feed(&mut partial, b"\x0bMSH")?;
    let refused = partial.step(
        Duration::ZERO,
        Event::Send(ORDER, Expect::OrderResponse(Duration::from_secs(2))),
    )?;
    assert!(
        refused
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Busy)))
    );
    assert!(writes(&refused).is_empty());
    Ok(())
}
