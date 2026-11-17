//! Tests POCT reviewer conversations and timeouts.
//! Kept whole: reviewer state transitions share custody and startup helpers.
use analyte_dialog::poct::{
    Action, CommitId, Config, Dialog, Error as DialogError, Event, StoreOutcome,
};
use analyte_fixtures::ConfigEdit as _;
use analyte_fixtures::poct as peer_messages;
use analyte_poct::{ApplicationError, ControlId, Message, Outbound};
use std::time::Duration;

fn dialog(capacity: usize) -> Result<Dialog, DialogError> {
    Dialog::new(
        Config {
            depth: 32,
            pending_messages: capacity,
            message_bytes: 65536,
            response_timeout: Duration::from_secs(180),
            unannounced_ack_timeout: Duration::from_secs(20),
        },
        ControlId::new("session-A")?,
    )
}

fn received(raw: &str) -> Result<Event, DialogError> {
    Ok(Event::Received(Box::new(Message::parse(
        raw.as_bytes(),
        peer_messages::limits(),
    )?)))
}

fn step(dialog: &mut Dialog, event: Event) -> Result<Vec<Action>, DialogError> {
    dialog.step(Duration::ZERO, event)
}

fn ticket(actions: &[Action]) -> Result<CommitId, DialogError> {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Store { ticket, .. } = a {
                Some(ticket.clone())
            } else {
                None
            }
        })
        .ok_or(DialogError::Commit)
}

fn sends(actions: &[Action]) -> Vec<&Outbound> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Write { body, .. } = a {
                Some(body)
            } else {
                None
            }
        })
        .collect()
}

fn complete(
    dialog: &mut Dialog,
    ticket: CommitId,
    outcome: StoreOutcome,
) -> Result<Vec<Action>, DialogError> {
    step(dialog, Event::Stored { ticket, outcome })
}

fn startup(dialog: &mut Dialog) -> Result<(), DialogError> {
    let inspect = step(dialog, received(&peer_messages::hello("hello"))?)?;
    assert!(matches!(inspect.first(), Some(Action::InspectHello { .. })));
    assert!(sends(&inspect).is_empty());
    let store = step(dialog, Event::HelloDecision(true))?;
    assert!(sends(&store).is_empty());
    let ack = complete(dialog, ticket(&store)?, StoreOutcome::Stored)?;
    assert!(matches!(sends(&ack).as_slice(),[Outbound::Accept(id)] if id.as_str() == "hello"));
    let store = step(dialog, received(&peer_messages::status("status", "2"))?)?;
    assert!(sends(&store).is_empty());
    let ack = complete(dialog, ticket(&store)?, StoreOutcome::Stored)?;
    assert!(
        matches!(sends(&ack).as_slice(),[Outbound::Accept(id),Outbound::RequestObservations] if id.as_str() == "status")
    );
    Ok(())
}

fn observation(id: &str) -> String {
    peer_messages::observation(id, true, "<OBS.value V=\"7.20\" U=\"unit\"/>")
}

fn eot() -> String {
    peer_messages::message(
        "EOT.R01",
        "eot",
        &format!("<EOT>{}</EOT>", peer_messages::field("EOT.topic_cd", "OBS")),
    )
}

#[test]
fn poct01_b_4_1_2_startup_and_rejection_in_sequence() -> Result<(), DialogError> {
    startup(&mut dialog(2)?)?;
    let mut rejected = dialog(2)?;
    step(&mut rejected, received(&peer_messages::hello("h"))?)?;
    let actions = step(&mut rejected, Event::HelloDecision(false))?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::ApplicationError { reference, error: ApplicationError::UnsupportedValue }] if reference.as_str() == "h"
    ));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Close { normal: false, .. }))
    );
    let mut early_status = dialog(2)?;
    let actions = step(
        &mut early_status,
        received(&peer_messages::status("s", "0"))?,
    )?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [
            Outbound::Escape { .. },
            Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)
        ]
    ));
    Ok(())
}

#[test]
fn poct01_b_3_2_adr0003_commit_gate_orders_asynchronous_acks() -> Result<(), DialogError> {
    let mut dialog = dialog(3)?;
    startup(&mut dialog)?;
    let first = ticket(&step(&mut dialog, received(&observation("o1"))?)?)?;
    let second = ticket(&step(&mut dialog, received(&observation("o2"))?)?)?;
    let ended = step(&mut dialog, received(&eot())?)?;
    assert!(!ended.iter().any(|a| matches!(a, Action::TopicComplete)));
    assert!(sends(&complete(&mut dialog, second, StoreOutcome::Stored)?).is_empty());
    let actions = complete(&mut dialog, first, StoreOutcome::Stored)?;
    assert!(
        matches!(sends(&actions).as_slice(),[Outbound::Accept(a),Outbound::Accept(b)] if a.as_str() == "o1" && b.as_str() == "o2")
    );
    assert!(matches!(actions.last(), Some(Action::TopicComplete)));
    Ok(())
}

#[test]
fn poct01_b_9_bounded_backpressure_returns_unconsumed_input() -> Result<(), DialogError> {
    let mut dialog = dialog(1)?;
    startup(&mut dialog)?;
    let store = step(&mut dialog, received(&observation("o1"))?)?;
    assert!(
        store
            .iter()
            .any(|a| matches!(a, Action::ReadEnabled(false)))
    );
    let deferred = step(&mut dialog, received(&observation("o2"))?)?;
    assert!(
        matches!(deferred.first(),Some(Action::Deferred(m)) if m.header().control_id.as_str() == "o2")
    );
    let actions = complete(&mut dialog, ticket(&store)?, StoreOutcome::Stored)?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ReadEnabled(true)))
    );
    Ok(())
}

#[test]
fn poct01_b_3_4_error_ack_keeps_topic_but_storage_failure_escapes() -> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    startup(&mut dialog)?;
    let t = ticket(&step(&mut dialog, received(&observation("bad"))?)?)?;
    let actions = complete(
        &mut dialog,
        t,
        StoreOutcome::ApplicationError(ApplicationError::UnsupportedValue),
    )?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::ApplicationError { reference, error: ApplicationError::UnsupportedValue }] if reference.as_str() == "bad"
    ));
    let t = ticket(&step(&mut dialog, received(&observation("retry"))?)?)?;
    let actions = step(&mut dialog, Event::StoreFailed(t))?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [
            Outbound::Escape { reference, reason: analyte_poct::EscapeReason::CannotComplete },
            Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)
        ] if reference.as_str() == "retry"
    ));
    Ok(())
}

#[test]
fn poct01_b_3_6_no_resend_on_application_timeout() -> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    startup(&mut dialog)?;
    let t = ticket(&step(&mut dialog, received(&observation("o"))?)?)?;
    let actions = dialog.step(Duration::from_millis(12500), Event::Tick)?;
    assert!(sends(&actions).is_empty());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Close{normal:false,unresolved} if unresolved.contains(&t)))
    );
    assert!(
        sends(&dialog.step(
            Duration::from_secs(13),
            Event::Stored {
                ticket: t,
                outcome: StoreOutcome::Stored
            }
        )?)
        .is_empty()
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_3_zero_observations_repeated_topic_and_normal_termination()
-> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    startup(&mut dialog)?;
    assert!(
        step(&mut dialog, received(&eot())?)?
            .iter()
            .any(|a| matches!(a, Action::TopicComplete))
    );
    assert!(matches!(
        sends(&step(&mut dialog, Event::RequestObservations)?).as_slice(),
        [Outbound::RequestObservations]
    ));
    // Each repeated topic has its own message ID (B 5.13), even when empty.
    let repeated_eot = eot().rewrite("HDR.control_id V=\"eot\"", "HDR.control_id V=\"eot-again\"");
    assert!(
        step(&mut dialog, received(&repeated_eot)?)?
            .iter()
            .any(|a| matches!(a, Action::TopicComplete))
    );
    let finish = step(&mut dialog, Event::Finish)?;
    let id = finish
        .iter()
        .find_map(|a| {
            if let Action::Write {
                control_id,
                body: Outbound::Terminate(analyte_poct::TerminationReason::Normal),
            } = a
            {
                Some(control_id.clone())
            } else {
                None
            }
        })
        .ok_or(DialogError::State)?;
    let raw = peer_messages::message(
        "ACK.R01",
        "final-ack",
        &format!(
            "<ACK>{}{}</ACK>",
            peer_messages::field("ACK.type_cd", "AA"),
            peer_messages::field("ACK.ack_control_id", id.as_str())
        ),
    );
    assert!(
        step(&mut dialog, received(&raw)?)?
            .iter()
            .any(|a| matches!(a, Action::Close { normal: true, .. }))
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_11_remote_end_waits_for_prior_custody() -> Result<(), DialogError> {
    let mut dialog = dialog(3)?;
    startup(&mut dialog)?;
    let t = ticket(&step(&mut dialog, received(&observation("o"))?)?)?;
    let end = peer_messages::message(
        "END.R01",
        "end",
        &format!(
            "<TRM>{}</TRM>",
            peer_messages::field("TRM.reason_cd", "USR")
        ),
    );
    let end = ticket(&step(&mut dialog, received(&end)?)?)?;
    assert!(sends(&complete(&mut dialog, end, StoreOutcome::Stored)?).is_empty());
    let actions = complete(&mut dialog, t, StoreOutcome::Stored)?;
    assert!(
        matches!(sends(&actions).as_slice(),[Outbound::Accept(a),Outbound::Accept(b)] if a.as_str() == "o" && b.as_str() == "end")
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Close { normal: true, .. }))
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_2_minimum_capability_peer_and_hello_deadline() -> Result<(), DialogError> {
    let mut minimal = dialog(2)?;
    let raw = peer_messages::message(
        "HEL.R01",
        "minimal",
        &format!(
            "<DEV>{}</DEV>",
            peer_messages::field("DEV.device_id", "01-02-03-04-05-06-07-08")
        ),
    );
    step(&mut minimal, received(&raw)?)?;
    let store = step(&mut minimal, Event::HelloDecision(true))?;
    let actions = complete(&mut minimal, ticket(&store)?, StoreOutcome::Stored)?;
    assert!(matches!(sends(&actions).as_slice(), [Outbound::Accept(_)]));
    let mut slow = dialog(2)?;
    step(&mut slow, received(&peer_messages::hello("h"))?)?;
    let store = slow.step(Duration::from_secs(10), Event::HelloDecision(true))?;
    let actions = slow.step(
        Duration::from_millis(12500),
        Event::Stored {
            ticket: ticket(&store)?,
            outcome: StoreOutcome::Stored,
        },
    )?;
    assert!(sends(&actions).is_empty());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Close { normal: false, .. }))
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_12_escape_correlation_and_final_ack_correlation() -> Result<(), DialogError> {
    let mut valid = dialog(2)?;
    step(&mut valid, received(&peer_messages::hello("h"))?)?;
    let store = step(&mut valid, Event::HelloDecision(true))?;
    complete(&mut valid, ticket(&store)?, StoreOutcome::Stored)?;
    let store = step(&mut valid, received(&peer_messages::status("s", "0"))?)?;
    let actions = complete(&mut valid, ticket(&store)?, StoreOutcome::Stored)?;
    let request = actions
        .iter()
        .find_map(|a| {
            if let Action::Write {
                control_id,
                body: Outbound::RequestObservations,
            } = a
            {
                Some(control_id.clone())
            } else {
                None
            }
        })
        .ok_or(DialogError::State)?;
    let escape = peer_messages::message(
        "ESC.R01",
        "escape",
        &format!(
            "<ESC>{}{}</ESC>",
            peer_messages::field("ESC.esc_control_id", request.as_str()),
            peer_messages::field("ESC.detail_cd", "CNC")
        ),
    );
    assert!(
        step(&mut valid, received(&escape)?)?
            .iter()
            .any(|a| matches!(a, Action::TopicComplete))
    );
    step(&mut valid, Event::Finish)?;
    let ack = peer_messages::message(
        "ACK.R01",
        "wrong-ack",
        &format!(
            "<ACK>{}{}</ACK>",
            peer_messages::field("ACK.type_cd", "AA"),
            peer_messages::field("ACK.ack_control_id", "not-the-end")
        ),
    );
    assert!(
        step(&mut valid, received(&ack)?)?
            .iter()
            .any(|a| matches!(a, Action::Close { normal: false, .. }))
    );
    Ok(())
}

#[test]
fn poct01_project_disconnect_wrong_session_and_reversed_time_do_not_ack() -> Result<(), DialogError>
{
    let mut left = dialog(2)?;
    startup(&mut left)?;
    let store = step(&mut left, received(&observation("o"))?)?;
    let t = ticket(&store)?;
    let mut fresh = Dialog::new(
        Config {
            depth: 32,
            pending_messages: 2,
            message_bytes: 65536,
            response_timeout: Duration::from_secs(180),
            unannounced_ack_timeout: Duration::from_secs(20),
        },
        ControlId::new("different-session")?,
    )?;
    assert!(matches!(
        complete(&mut fresh, t.clone(), StoreOutcome::Stored),
        Err(DialogError::Commit)
    ));
    let actions = step(&mut left, Event::Disconnected)?;
    assert!(sends(&actions).is_empty());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Close{unresolved,..} if unresolved.contains(&t)))
    );
    fresh.step(Duration::from_secs(1), Event::Tick)?;
    assert!(matches!(
        fresh.step(Duration::ZERO, Event::Tick),
        Err(DialogError::TimeReversed)
    ));
    Ok(())
}

// POCT01-A2 B 4.1.2 p.78: semantic Hello failure sends error ACK then END, not close.
#[test]
fn poct01_b4_1_2_invalid_capabilities_are_retained_and_termination_has_deadline()
-> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    let raw = peer_messages::hello("bad").rewrite(
        "<DSC.max_message_sz NULL=\"PINF\"/>",
        "<DSC.max_message_sz V=\"0\"/>",
    );
    step(&mut dialog, received(&raw)?)?;
    let actions = step(&mut dialog, Event::HelloDecision(true))?;
    assert!(
        matches!(actions.first(), Some(Action::Park { message, .. }) if message.document().raw() == raw.as_bytes())
    );
    assert!(
        matches!(sends(&actions).as_slice(), [Outbound::ApplicationError { reference, error: ApplicationError::UnsupportedValue }, Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)] if reference.as_str() == "bad")
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Close { .. })));
    let expired = dialog.step(Duration::from_secs(180), Event::Tick)?;
    assert!(matches!(
        expired.last(),
        Some(Action::Close { normal: false, .. })
    ));
    Ok(())
}

// POCT01-A2 B 5.13 p.99, 3.4.2 p.74: every incoming message shares the ID namespace.
#[test]
fn poct01_b5_13_reused_id_before_commit_is_retained_not_accepted() -> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    startup(&mut dialog)?;
    let first = step(&mut dialog, received(&observation("same"))?)?;
    let repeated = observation("same").rewrite("7.20", "9.20");
    let actions = step(&mut dialog, received(&repeated)?)?;
    assert!(
        matches!(actions.first(), Some(Action::Park { message, .. }) if message.document().raw() == repeated.as_bytes())
    );
    assert!(
        matches!(sends(&actions).as_slice(), [Outbound::Escape { reference, .. }, Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)] if reference.as_str() == "same")
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Store { .. })));
    assert!(
        sends(&complete(
            &mut dialog,
            ticket(&first)?,
            StoreOutcome::Stored
        )?)
        .is_empty()
    );
    Ok(())
}

// POCT01-A2 B 5.13 p.99; AGENTS.md rule 7: no silent eviction when the budget fills.
#[test]
fn poct01_b5_13_control_id_budget_ends_conversation_without_forgetting_ids()
-> Result<(), DialogError> {
    let mut config = Config {
        depth: 32,
        pending_messages: 2,
        message_bytes: 1024,
        response_timeout: Duration::from_secs(180),
        unannounced_ack_timeout: Duration::from_secs(20),
    };
    for (overflow, expected_reason) in [
        (
            observation(&"z".repeat(256)),
            analyte_poct::EscapeReason::CannotComplete,
        ),
        (observation("hello"), analyte_poct::EscapeReason::Other),
    ] {
        let mut full = Dialog::new(config.clone(), ControlId::new("bounded")?)?;
        startup(&mut full)?;
        for n in 0..4 {
            let id = format!("{n}{}", "x".repeat(245));
            let actions = step(&mut full, received(&observation(&id))?)?;
            complete(&mut full, ticket(&actions)?, StoreOutcome::Stored)?;
        }
        let actions = step(&mut full, received(&overflow)?)?;
        assert!(
            matches!(actions.first(), Some(Action::Park { message, .. }) if message.document().raw() == overflow.as_bytes())
        );
        assert!(matches!(
            sends(&actions).as_slice(),
            [
                Outbound::Escape {
                    reason,
                    ..
                },
                Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)
            ] if *reason == expected_reason
        ));
        // Even at capacity the correlated, unique terminal ACK must be usable.
        let end_id = actions
            .iter()
            .find_map(|a| {
                if let Action::Write {
                    control_id,
                    body: Outbound::Terminate(analyte_poct::TerminationReason::Abnormal),
                } = a
                {
                    Some(control_id.as_str())
                } else {
                    None
                }
            })
            .ok_or(DialogError::State)?;
        let ack = peer_messages::message(
            "ACK.R01",
            &"a".repeat(256),
            &format!(
                "<ACK><ACK.type_cd V=\"AA\"/>{}</ACK>",
                peer_messages::field("ACK.ack_control_id", end_id)
            ),
        );
        let closed = step(&mut full, received(&ack)?)?;
        assert!(matches!(
            closed.last(),
            Some(Action::Close { normal: true, .. })
        ));
    }
    // A fresh conversation may reuse IDs from the previous one.
    config.message_bytes = 65536;
    startup(&mut Dialog::new(config, ControlId::new("new-session")?)?)?;
    Ok(())
}

// POCT01-A2 B 4.1.2 p.78 and 5.13 p.99: a recovered rejected Hello ID is still used.
#[test]
fn poct01_b5_13_rejected_hello_reference_cannot_be_reused_by_terminal_ack()
-> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    let rejected = step(
        &mut dialog,
        Event::InvalidHello {
            reference: ControlId::new("rejected-hello")?,
            error: ApplicationError::DataType,
        },
    )?;
    let end_id = rejected
        .iter()
        .find_map(|a| {
            if let Action::Write {
                control_id,
                body: Outbound::Terminate(analyte_poct::TerminationReason::Abnormal),
            } = a
            {
                Some(control_id.as_str())
            } else {
                None
            }
        })
        .ok_or(DialogError::State)?;
    let ack = peer_messages::message(
        "ACK.R01",
        "rejected-hello",
        &format!(
            "<ACK><ACK.type_cd V=\"AA\"/>{}</ACK>",
            peer_messages::field("ACK.ack_control_id", end_id)
        ),
    );
    let actions = step(&mut dialog, received(&ack)?)?;
    assert!(matches!(actions.first(), Some(Action::Park { .. })));
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::Escape { .. }]
    ));
    assert!(matches!(
        actions.last(),
        Some(Action::Close { normal: false, .. })
    ));
    Ok(())
}

// POCT01-A2 Appendix B 5.12 Table 29 and 3.4.2.
#[test]
fn poct01_b_5_12_unexpected_ack_escape_references_incoming_header() -> Result<(), DialogError> {
    let mut dialog = dialog(4)?;
    startup(&mut dialog)?;
    let raw = peer_messages::message(
        "ACK.R01",
        "unexpected-ack",
        "<ACK><ACK.type_cd V=\"AA\"/><ACK.ack_control_id V=\"prior-request\"/></ACK>",
    );
    let actions = step(&mut dialog, received(&raw)?)?;
    assert!(
        matches!(sends(&actions).as_slice(), [Outbound::Escape { reference, reason: analyte_poct::EscapeReason::Other }, Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)] if reference.as_str() == "unexpected-ack")
    );
    for action in &actions {
        if let Action::Write { control_id, body } = action {
            let encoded = analyte_poct::encode(
                &analyte_poct::Header {
                    control_id: control_id.clone(),
                    created: analyte_poct::Timestamp::parse("2026-06-09T08:00:00+02:00")
                        .map_err(analyte_poct::MessageError::from)?,
                },
                body,
                peer_messages::limits(),
            )?;
            let raw = std::str::from_utf8(encoded.document().raw()).expect("encoder emits UTF-8");
            if matches!(body, Outbound::Escape { .. }) {
                assert!(raw.contains("<ESC.esc_control_id V=\"unexpected-ack\"/>"));
                assert!(raw.contains("<ESC.detail_cd V=\"OTH\"/>"));
            } else {
                assert!(raw.contains("<TRM.reason_cd V=\"ABN\"/>"));
            }
        }
    }
    Ok(())
}

#[test]
fn adr_0001_poct_reports_absolute_custody_deadline_without_polling() -> Result<(), DialogError> {
    let mut dialog = Dialog::new(
        Config {
            depth: 32,
            pending_messages: 2,
            message_bytes: 65536,
            response_timeout: Duration::from_secs(180),
            unannounced_ack_timeout: Duration::from_secs(20),
        },
        ControlId::new("deadline-session")?,
    )?;
    assert_eq!(dialog.next_deadline(), Some(Duration::from_secs(180)));
    let hello = Message::parse(
        peer_messages::hello("hello").as_bytes(),
        peer_messages::limits(),
    )?;
    dialog.step(Duration::from_secs(5), Event::Received(Box::new(hello)))?;
    let actions = dialog.step(Duration::from_secs(5), Event::HelloDecision(true))?;
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Store { .. }))
    );
    // The synthetic Hello advertises a 12.5-second ACK budget (POCT01 B.4.1.2).
    assert_eq!(dialog.next_deadline(), Some(Duration::from_millis(17500)));
    dialog.step(Duration::from_secs(17), Event::Tick)?;
    assert_eq!(dialog.next_deadline(), Some(Duration::from_millis(17500)));
    let actions = dialog.step(Duration::from_millis(17500), Event::Tick)?;
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Close { normal: false, .. }))
    );
    assert_eq!(dialog.next_deadline(), None);
    Ok(())
}

#[test]
fn poct01_b3_2_journaled_bytes_keep_root_and_incomplete_evidence() -> Result<(), DialogError> {
    let mut dialog = dialog(1)?;
    let raw = peer_messages::hello("byte-hello");
    let mut actions = Vec::new();
    for byte in raw.bytes() {
        actions.extend(step(&mut dialog, Event::Byte(byte))?);
    }
    assert!(matches!(actions.as_slice(), [Action::Parse(wire)] if wire == raw.as_bytes()));
    assert!(sends(&actions).is_empty());
    let inspect = step(&mut dialog, received(&raw)?)?;
    assert!(matches!(inspect.first(), Some(Action::InspectHello { .. })));
    for byte in b"<unfinished" {
        assert!(step(&mut dialog, Event::Byte(*byte))?.is_empty());
    }
    assert_eq!(dialog.take_input(), b"<unfinished");
    assert!(dialog.take_input().is_empty());
    Ok(())
}

#[test]
fn poct01_b3_6_incomplete_tokens_do_not_postpone_deadline() -> Result<(), DialogError> {
    let mut dialog = dialog(1)?;
    for byte in b"<unfinished" {
        assert!(
            dialog
                .step(Duration::from_secs(179), Event::Byte(*byte))?
                .is_empty()
        );
    }
    let actions = dialog.step(Duration::from_secs(180), Event::Byte(b'>'))?;
    assert!(matches!(actions.first(), Some(Action::ParseDeadline(raw)) if raw == b"<unfinished>"));
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Close { normal: false, .. }))
    );
    assert!(dialog.take_input().is_empty());
    Ok(())
}
