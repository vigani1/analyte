//! Tests law tf2b 3 29 adr0003 ack choice follows durable interpretation; law tf2b b7 adr0003 failed or late store never produces acceptance.
use analyte_dialog::law::*;
use analyte_hl7::{Charset, ControlId, Options, Response, law};
use std::time::Duration;
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn options() -> Options {
    Options {
        charset: Charset::Utf8,
        ..Options::default()
    }
}
fn dialog(link: Link) -> Result<Dialog, Error> {
    Dialog::new(
        options(),
        Duration::from_secs(5),
        Duration::from_secs(2),
        link,
    )
}
fn wire(raw: &[u8]) -> Result<Vec<u8>, analyte_framing::hl7::Error> {
    analyte_framing::hl7::encode(raw, analyte_framing::hl7::Limit::new(65536)?)
}
fn feed(d: &mut Dialog, raw: &[u8]) -> Result<Vec<Action>, Error> {
    let mut actions = Vec::new();
    for byte in raw {
        actions.extend(d.step(Duration::ZERO, Event::Byte(*byte))?);
    }
    Ok(actions)
}
fn receipt(actions: &[Action]) -> Result<ReceiptId, Box<dyn std::error::Error>> {
    actions
        .iter()
        .find_map(|action| {
            if let Action::Commit(id, _) = action {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or_else(|| "synthetic receipt missing".into())
}
fn response() -> Result<Response<'static>, analyte_hl7::Error> {
    Ok(Response {
        control_id: ControlId::new(b"rsp1")?,
        timestamp: b"20260101120000+0000",
    })
}
fn result(id: &str, awos: &str, filler: &str, value: &str, status: &str) -> String {
    let mut fields = vec![""; 29];
    for (index, value) in [
        (0, "1"),
        (1, "NM"),
        (2, "SYN^Synthetic^99LAB"),
        (3, "1"),
        (4, value),
        (5, "mg/L^mg/L^UCUM"),
        (6, "0-20"),
        (7, "N^Normal^HL70078"),
        (10, status),
        (15, "operator^Synthetic"),
        (17, "instrument^99LAB~module^99LAB"),
        (18, "20260101120000"),
        (28, "RSLT"),
    ] {
        if let Some(field) = fields.get_mut(index) {
            *field = value;
        }
    }
    format!(
        "MSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|{id}|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||00012a^LAB\rOBR||{awos}|{filler}|SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|{}\r",
        fields.join("|")
    )
}

#[test]
fn law_tf2b_3_29_adr0003_ack_choice_follows_durable_interpretation() -> TestResult {
    for (outcome, expected) in [
        (Interpretation::Accepted, b"AA".as_slice()),
        (Interpretation::Conflict, b"AR"),
        (Interpretation::Failed(None), b"AE"),
        (
            Interpretation::WorkConflict(Location {
                segment: b"OBR",
                occurrence: 1,
                field: 2,
            }),
            b"AR",
        ),
    ] {
        let mut d = dialog(Link::Inbound)?;
        let raw = wire(result("r1", "\"\"", "run^ANALYZER", "12.30", "F").as_bytes())?;
        let actions = feed(&mut d, &raw)?;
        let id = receipt(&actions)?;
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Write(..) | Action::Interpret(_)))
        );
        let actions = d.step(Duration::ZERO, Event::Committed(id))?;
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Interpret(token) if *token == id))
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Write(..))));
        // A query-service reply cannot bypass the result interpretation phase.
        let wrong_phase = Reply::Response(work()?);
        let refused = d.step(Duration::ZERO, Event::Reply(id, &wrong_phase, None))?;
        assert!(
            refused
                .iter()
                .any(|a| matches!(a, Action::Refused(Error::Stale)))
        );
        assert!(!refused.iter().any(|a| matches!(a, Action::Write(..))));
        let actions = d.step(Duration::ZERO, Event::Interpreted(id, outcome))?;
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::ResponseNeeded(token) if *token == id))
        );
        let actions = d.step(Duration::ZERO, Event::Respond(id, response()?))?;
        let bytes = actions
            .iter()
            .find_map(|a| {
                if let Action::Write(bytes, _) = a {
                    Some(bytes)
                } else {
                    None
                }
            })
            .ok_or("ACK missing")?;
        let message = law::parse_message(
            analyte_framing::hl7::payload(bytes).ok_or("ACK envelope")?,
            options(),
        )?;
        assert_eq!(
            message
                .segments()
                .find(|s| s.id() == b"MSA")
                .and_then(|s| s.field(1))
                .map(analyte_hl7::Field::raw),
            Some(expected)
        );
    }
    Ok(())
}
#[test]
fn law_tf2b_b7_adr0003_failed_or_late_store_never_produces_acceptance() -> TestResult {
    for late in [false, true] {
        let mut d = dialog(Link::Inbound)?;
        let actions = feed(
            &mut d,
            &wire(result("r1", "\"\"", "run^ANALYZER", "12.30", "F").as_bytes())?,
        )?;
        let id = receipt(&actions)?;
        let actions = if late {
            d.step(Duration::from_secs(5), Event::Committed(id))?
        } else {
            d.step(Duration::ZERO, Event::CommitFailed(id))?
        };
        assert!(actions.iter().any(|a| matches!(a, Action::Refused(_))));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Write(..) | Action::Interpret(_) | Action::ResponseNeeded(_)
        )));
    }
    Ok(())
}
#[test]
fn law_tf2b_b6_independent_link_does_not_ack_unrelated_transactions() -> TestResult {
    let mut d = dialog(Link::WorkResponse)?;
    let actions = feed(
        &mut d,
        &wire(result("r1", "\"\"", "run^ANALYZER", "12.30", "F").as_bytes())?,
    )?;
    let id = receipt(&actions)?;
    let actions = d.step(Duration::ZERO, Event::Committed(id))?;
    assert!(actions.iter().any(|a| matches!(a, Action::Failed(_, _))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::NotOrl)))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Write(..) | Action::Interpret(_)))
    );
    Ok(())
}
#[test]
fn mllp_r1_1_2_law_first_byte_budget_and_disconnect_preserve_pending_bytes() -> TestResult {
    let mut d = dialog(Link::Inbound)?;
    d.step(Duration::ZERO, Event::Byte(11))?;
    d.step(Duration::from_secs(4), Event::Byte(b'M'))?;
    let actions = d.step(Duration::from_secs(5), Event::Tick)?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Rejected(_, raw) if raw == &[11, b'M']))
    );
    let mut d = dialog(Link::Inbound)?;
    let raw = wire(result("r1", "\"\"", "run^ANALYZER", "12.30", "F").as_bytes())?;
    feed(&mut d, &raw)?;
    let actions = d.step(Duration::ZERO, Event::Disconnect)?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Uncommitted(_, bytes) if *bytes == raw))
    );
    Ok(())
}
fn work() -> Result<analyte_hl7::Message, Box<dyn std::error::Error>> {
    let awos = law::Awos::new(
        b"awos1",
        b"sample&99LAB",
        b"SER^Serum^HL70487",
        b"P",
        b"tube",
        b"SYN^Synthetic^99LAB",
        law::WorkAction::New,
    )?;
    Ok(law::work_order(
        &[awos],
        None,
        law::Address {
            application: b"M",
            facility: b"MF",
            peer_application: b"A",
            peer_facility: b"AF",
        },
        response()?,
        law::Capabilities::default(),
        options(),
    )?)
}

#[test]
fn law_tf2b_b6_3_28_work_budget_begins_after_issuance_and_never_resends() -> TestResult {
    let work = work()?;
    let mut exchange = WorkExchange::new(work.clone(), Duration::from_secs(2));
    assert!(!exchange.expired(Duration::from_secs(1)));
    exchange.issued(Duration::from_secs(1), Duration::from_secs(3))?;
    assert_eq!(exchange.deadline(), Duration::from_secs(4));
    assert!(
        exchange
            .issued(Duration::from_secs(1), Duration::from_secs(3))
            .is_err()
    );
    let mut d = dialog(Link::WorkResponse)?;
    let actions = d.step(Duration::ZERO, Event::Send(&work, Duration::from_secs(2)))?;
    assert!(actions.iter().any(|a| matches!(a, Action::Write(bytes, until) if Some(work.raw()) == analyte_framing::hl7::payload(bytes) && *until == Duration::from_secs(2))));
    let actions = d.step(Duration::from_secs(1), Event::Written)?;
    assert!(actions.iter().any(|a| matches!(a, Action::WorkIssued)));
    d.step(
        Duration::from_secs(1),
        Event::WorkDeadline(exchange.deadline()),
    )?;
    let actions = d.step(Duration::from_secs(4), Event::Tick)?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Deadline(_))))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(..))));
    Ok(())
}

#[test]
fn law_tf2b_3_27_3_28_availability_precedes_same_link_followup() -> TestResult {
    let raw = b"MSH|^~\\&|A|AF|M|MF|20260101120000+0000||QBP^Q11^QBP_Q11|q1|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\rQPD|WOS^Work Order Step^IHELAW|tag|tube\rRCP|I||R^Real Time^HL70394\r";
    let message = law::parse_message(raw, options())?;
    let query = law::Query::new(&message)?;
    let rsp = law::query_response(
        query,
        law::CacheAnswer::WorkExists,
        response()?,
        analyte_hl7::Responsibility::StoredAndInterpreted,
        options(),
    )?
    .message()
    .clone();
    let work = work()?;
    let reply = Reply::Followup {
        response: rsp.clone(),
        work: work.clone(),
    };
    let mut d = dialog(Link::Inbound)?;
    let actions = feed(&mut d, &wire(raw)?)?;
    let id = receipt(&actions)?;
    let actions = d.step(Duration::ZERO, Event::Committed(id))?;
    assert!(actions.iter().any(|a| matches!(a, Action::Query(..))));
    let actions = d.step(
        Duration::ZERO,
        Event::Reply(id, &reply, Some(Duration::from_secs(3))),
    )?;
    assert!(actions.iter().any(|a| matches!(a, Action::Write(bytes, _) if analyte_framing::hl7::payload(bytes) == Some(rsp.raw()))));
    assert!(!actions.iter().any(|a| matches!(a, Action::WorkIssued)));
    let actions = d.step(Duration::from_secs(1), Event::Written)?;
    assert!(actions.iter().any(|a| matches!(a, Action::Write(bytes, _) if analyte_framing::hl7::payload(bytes) == Some(work.raw()))));
    let actions = d.step(Duration::from_secs(2), Event::Written)?;
    assert!(actions.iter().any(|a| matches!(a, Action::WorkIssued)));
    Ok(())
}
#[test]
fn law_tf2b_b7_deadline_exposes_unconsumed_byte_and_rejects_late_work() -> TestResult {
    let mut d = dialog(Link::Inbound)?;
    d.step(Duration::ZERO, Event::Byte(11))?;
    let actions = d.step(Duration::from_secs(5), Event::Byte(b'M'))?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Unconsumed(b'M')))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Rejected(_, bytes) if bytes == &[11]))
    );
    let mut d = dialog(Link::WorkResponse)?;
    let work = work()?;
    let actions = d.step(
        Duration::from_secs(2),
        Event::Send(&work, Duration::from_secs(2)),
    )?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Refused(Error::Deadline(_))))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Write(..))));
    Ok(())
}
