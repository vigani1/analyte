//! Appendix B sections 3, 4 and 9 deterministic reviewer conversation tests.
mod common;
use analyte_poct::{
    ApplicationError, CommitId, CommitOutcome, ControlId, Dialog, DialogAction, DialogConfig,
    DialogError, DialogEvent, Message, Outbound,
};
use std::time::Duration;

fn dialog(capacity: usize) -> Result<Dialog, DialogError> {
    Dialog::new(
        DialogConfig {
            pending_messages: capacity,
            message_bytes: 65536,
            response_timeout: Duration::from_secs(180),
            unannounced_ack_timeout: Duration::from_secs(20),
        },
        ControlId::new("session-A")?,
    )
}

fn received(raw: &str) -> Result<DialogEvent, DialogError> {
    Ok(DialogEvent::Received(Box::new(Message::parse(
        raw.as_bytes(),
        common::limits(),
    )?)))
}

fn step(dialog: &mut Dialog, event: DialogEvent) -> Result<Vec<DialogAction>, DialogError> {
    dialog.step(Duration::ZERO, event)
}

fn ticket(actions: &[DialogAction]) -> Result<CommitId, DialogError> {
    actions
        .iter()
        .find_map(|a| {
            if let DialogAction::Store { ticket, .. } = a {
                Some(ticket.clone())
            } else {
                None
            }
        })
        .ok_or(DialogError::Commit)
}

fn sends(actions: &[DialogAction]) -> Vec<&Outbound> {
    actions
        .iter()
        .filter_map(|a| {
            if let DialogAction::Send { body, .. } = a {
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
    outcome: CommitOutcome,
) -> Result<Vec<DialogAction>, DialogError> {
    step(dialog, DialogEvent::Committed { ticket, outcome })
}

fn startup(dialog: &mut Dialog) -> Result<(), DialogError> {
    let inspect = step(dialog, received(&common::hello("hello"))?)?;
    assert!(matches!(
        inspect.first(),
        Some(DialogAction::InspectHello { .. })
    ));
    assert!(sends(&inspect).is_empty());
    let store = step(dialog, DialogEvent::HelloDecision(true))?;
    assert!(sends(&store).is_empty());
    let ack = complete(dialog, ticket(&store)?, CommitOutcome::Stored)?;
    assert!(matches!(sends(&ack).as_slice(),[Outbound::Accept(id)] if id.as_str() == "hello"));
    let store = step(dialog, received(&common::status("status", "2"))?)?;
    assert!(sends(&store).is_empty());
    let ack = complete(dialog, ticket(&store)?, CommitOutcome::Stored)?;
    assert!(
        matches!(sends(&ack).as_slice(),[Outbound::Accept(id),Outbound::RequestObservations] if id.as_str() == "status")
    );
    Ok(())
}

fn observation(id: &str) -> String {
    common::observation(id, true, "<OBS.value V=\"7.20\" U=\"unit\"/>")
}

fn eot() -> String {
    common::message(
        "EOT.R01",
        "eot",
        &format!("<EOT>{}</EOT>", common::field("EOT.topic_cd", "OBS")),
    )
}

#[test]
fn poct01_b_4_1_2_startup_and_rejection_in_sequence() -> Result<(), DialogError> {
    startup(&mut dialog(2)?)?;
    let mut rejected = dialog(2)?;
    step(&mut rejected, received(&common::hello("h"))?)?;
    let actions = step(&mut rejected, DialogEvent::HelloDecision(false))?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::ApplicationError { .. }]
    ));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DialogAction::Close { normal: false, .. }))
    );
    let mut early_status = dialog(2)?;
    let actions = step(&mut early_status, received(&common::status("s", "0"))?)?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::Escape { .. }, Outbound::Terminate(_)]
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
    assert!(
        !ended
            .iter()
            .any(|a| matches!(a, DialogAction::TopicComplete))
    );
    assert!(sends(&complete(&mut dialog, second, CommitOutcome::Stored)?).is_empty());
    let actions = complete(&mut dialog, first, CommitOutcome::Stored)?;
    assert!(
        matches!(sends(&actions).as_slice(),[Outbound::Accept(a),Outbound::Accept(b)] if a.as_str() == "o1" && b.as_str() == "o2")
    );
    assert!(matches!(actions.last(), Some(DialogAction::TopicComplete)));
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
            .any(|a| matches!(a, DialogAction::ReadEnabled(false)))
    );
    let deferred = step(&mut dialog, received(&observation("o2"))?)?;
    assert!(
        matches!(deferred.first(),Some(DialogAction::Deferred(m)) if m.header().control_id.as_str() == "o2")
    );
    let actions = complete(&mut dialog, ticket(&store)?, CommitOutcome::Stored)?;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DialogAction::ReadEnabled(true)))
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
        CommitOutcome::ApplicationError(ApplicationError::UnsupportedValue),
    )?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::ApplicationError { .. }]
    ));
    let t = ticket(&step(&mut dialog, received(&observation("retry"))?)?)?;
    let actions = complete(&mut dialog, t, CommitOutcome::Failed)?;
    assert!(matches!(
        sends(&actions).as_slice(),
        [Outbound::Escape { .. }, Outbound::Terminate(_)]
    ));
    Ok(())
}

#[test]
fn poct01_b_3_6_no_resend_on_application_timeout() -> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    startup(&mut dialog)?;
    let t = ticket(&step(&mut dialog, received(&observation("o"))?)?)?;
    let actions = dialog.step(Duration::from_millis(12500), DialogEvent::Tick)?;
    assert!(sends(&actions).is_empty());
    assert!(actions.iter().any(
        |a| matches!(a,DialogAction::Close{normal:false,unresolved} if unresolved.contains(&t))
    ));
    assert!(
        sends(&dialog.step(
            Duration::from_secs(13),
            DialogEvent::Committed {
                ticket: t,
                outcome: CommitOutcome::Stored
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
            .any(|a| matches!(a, DialogAction::TopicComplete))
    );
    assert!(matches!(
        sends(&step(&mut dialog, DialogEvent::RequestObservations)?).as_slice(),
        [Outbound::RequestObservations]
    ));
    // Each repeated topic has its own message ID (B 5.13), even when empty.
    let repeated_eot = eot().replace("HDR.control_id V=\"eot\"", "HDR.control_id V=\"eot-again\"");
    assert!(
        step(&mut dialog, received(&repeated_eot)?)?
            .iter()
            .any(|a| matches!(a, DialogAction::TopicComplete))
    );
    let finish = step(&mut dialog, DialogEvent::Finish)?;
    let id = finish
        .iter()
        .find_map(|a| {
            if let DialogAction::Send {
                control_id,
                body: Outbound::Terminate(_),
            } = a
            {
                Some(control_id.clone())
            } else {
                None
            }
        })
        .ok_or(DialogError::State)?;
    let raw = common::message(
        "ACK.R01",
        "final-ack",
        &format!(
            "<ACK>{}{}</ACK>",
            common::field("ACK.type_cd", "AA"),
            common::field("ACK.ack_control_id", id.as_str())
        ),
    );
    assert!(
        step(&mut dialog, received(&raw)?)?
            .iter()
            .any(|a| matches!(a, DialogAction::Close { normal: true, .. }))
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_11_remote_end_waits_for_prior_custody() -> Result<(), DialogError> {
    let mut dialog = dialog(3)?;
    startup(&mut dialog)?;
    let t = ticket(&step(&mut dialog, received(&observation("o"))?)?)?;
    let end = common::message(
        "END.R01",
        "end",
        &format!("<TRM>{}</TRM>", common::field("TRM.reason_cd", "USR")),
    );
    let end = ticket(&step(&mut dialog, received(&end)?)?)?;
    assert!(sends(&complete(&mut dialog, end, CommitOutcome::Stored)?).is_empty());
    let actions = complete(&mut dialog, t, CommitOutcome::Stored)?;
    assert!(
        matches!(sends(&actions).as_slice(),[Outbound::Accept(a),Outbound::Accept(b)] if a.as_str() == "o" && b.as_str() == "end")
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DialogAction::Close { normal: true, .. }))
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_2_minimum_capability_peer_and_hello_deadline() -> Result<(), DialogError> {
    let mut minimal = dialog(2)?;
    let raw = common::message(
        "HEL.R01",
        "minimal",
        &format!(
            "<DEV>{}</DEV>",
            common::field("DEV.device_id", "synthetic-device")
        ),
    );
    step(&mut minimal, received(&raw)?)?;
    let store = step(&mut minimal, DialogEvent::HelloDecision(true))?;
    let actions = complete(&mut minimal, ticket(&store)?, CommitOutcome::Stored)?;
    assert!(matches!(sends(&actions).as_slice(), [Outbound::Accept(_)]));
    let mut slow = dialog(2)?;
    step(&mut slow, received(&common::hello("h"))?)?;
    let store = slow.step(Duration::from_secs(10), DialogEvent::HelloDecision(true))?;
    let actions = slow.step(
        Duration::from_millis(12500),
        DialogEvent::Committed {
            ticket: ticket(&store)?,
            outcome: CommitOutcome::Stored,
        },
    )?;
    assert!(sends(&actions).is_empty());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, DialogAction::Close { normal: false, .. }))
    );
    Ok(())
}

#[test]
fn poct01_b_4_1_12_escape_correlation_and_final_ack_correlation() -> Result<(), DialogError> {
    let mut valid = dialog(2)?;
    step(&mut valid, received(&common::hello("h"))?)?;
    let store = step(&mut valid, DialogEvent::HelloDecision(true))?;
    complete(&mut valid, ticket(&store)?, CommitOutcome::Stored)?;
    let store = step(&mut valid, received(&common::status("s", "0"))?)?;
    let actions = complete(&mut valid, ticket(&store)?, CommitOutcome::Stored)?;
    let request = actions
        .iter()
        .find_map(|a| {
            if let DialogAction::Send {
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
    let escape = common::message(
        "ESC.R01",
        "escape",
        &format!(
            "<ESC>{}{}</ESC>",
            common::field("ESC.esc_control_id", request.as_str()),
            common::field("ESC.detail_cd", "CNC")
        ),
    );
    assert!(
        step(&mut valid, received(&escape)?)?
            .iter()
            .any(|a| matches!(a, DialogAction::TopicComplete))
    );
    step(&mut valid, DialogEvent::Finish)?;
    let ack = common::message(
        "ACK.R01",
        "wrong-ack",
        &format!(
            "<ACK>{}{}</ACK>",
            common::field("ACK.type_cd", "AA"),
            common::field("ACK.ack_control_id", "not-the-end")
        ),
    );
    assert!(
        step(&mut valid, received(&ack)?)?
            .iter()
            .any(|a| matches!(a, DialogAction::Close { normal: false, .. }))
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
        DialogConfig {
            pending_messages: 2,
            message_bytes: 65536,
            response_timeout: Duration::from_secs(180),
            unannounced_ack_timeout: Duration::from_secs(20),
        },
        ControlId::new("different-session")?,
    )?;
    assert!(matches!(
        complete(&mut fresh, t.clone(), CommitOutcome::Stored),
        Err(DialogError::Commit)
    ));
    let actions = step(&mut left, DialogEvent::Disconnected)?;
    assert!(sends(&actions).is_empty());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,DialogAction::Close{unresolved,..} if unresolved.contains(&t)))
    );
    fresh.step(Duration::from_secs(1), DialogEvent::Tick)?;
    assert!(matches!(
        fresh.step(Duration::ZERO, DialogEvent::Tick),
        Err(DialogError::TimeReversed)
    ));
    Ok(())
}

// POCT01-A2 B 4.1.2 p.78: semantic Hello failure sends error ACK then END, not close.
#[test]
fn poct01_b4_1_2_invalid_capabilities_are_retained_and_termination_has_deadline()
-> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    let raw = common::hello("bad").replace(
        "<DSC.max_message_sz NULL=\"PINF\"/>",
        "<DSC.max_message_sz V=\"0\"/>",
    );
    step(&mut dialog, received(&raw)?)?;
    let actions = step(&mut dialog, DialogEvent::HelloDecision(true))?;
    assert!(
        matches!(actions.first(), Some(DialogAction::Park { message, .. }) if message.document().raw() == raw.as_bytes())
    );
    assert!(
        matches!(sends(&actions).as_slice(), [Outbound::ApplicationError { reference, error: ApplicationError::UnsupportedValue }, Outbound::Terminate(analyte_poct::TerminationReason::Abnormal)] if reference.as_str() == "bad")
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, DialogAction::Close { .. }))
    );
    let expired = dialog.step(Duration::from_secs(180), DialogEvent::Tick)?;
    assert!(matches!(
        expired.last(),
        Some(DialogAction::Close { normal: false, .. })
    ));
    Ok(())
}

// POCT01-A2 B 5.13 p.99, 3.4.2 p.74: every incoming message shares the ID namespace.
#[test]
fn poct01_b5_13_reused_id_before_commit_is_retained_not_accepted() -> Result<(), DialogError> {
    let mut dialog = dialog(2)?;
    startup(&mut dialog)?;
    let first = step(&mut dialog, received(&observation("same"))?)?;
    let repeated = observation("same").replace("7.20", "9.20");
    let actions = step(&mut dialog, received(&repeated)?)?;
    assert!(
        matches!(actions.first(), Some(DialogAction::Park { message, .. }) if message.document().raw() == repeated.as_bytes())
    );
    assert!(
        matches!(sends(&actions).as_slice(), [Outbound::Escape { reference, .. }, Outbound::Terminate(_)] if reference.as_str() == "same")
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, DialogAction::Store { .. }))
    );
    assert!(
        sends(&complete(
            &mut dialog,
            ticket(&first)?,
            CommitOutcome::Stored
        )?)
        .is_empty()
    );
    Ok(())
}

// POCT01-A2 B 5.13 p.99; AGENTS.md rule 7: no silent eviction when the budget fills.
#[test]
fn poct01_b5_13_control_id_budget_ends_conversation_without_forgetting_ids()
-> Result<(), DialogError> {
    let mut config = DialogConfig {
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
            complete(&mut full, ticket(&actions)?, CommitOutcome::Stored)?;
        }
        let actions = step(&mut full, received(&overflow)?)?;
        assert!(
            matches!(actions.first(), Some(DialogAction::Park { message, .. }) if message.document().raw() == overflow.as_bytes())
        );
        assert!(matches!(
            sends(&actions).as_slice(),
            [
                Outbound::Escape {
                    reason,
                    ..
                },
                Outbound::Terminate(_)
            ] if *reason == expected_reason
        ));
        // Even at capacity the correlated, unique terminal ACK must be usable.
        let end_id = actions
            .iter()
            .find_map(|a| {
                if let DialogAction::Send {
                    control_id,
                    body: Outbound::Terminate(_),
                } = a
                {
                    Some(control_id.as_str())
                } else {
                    None
                }
            })
            .ok_or(DialogError::State)?;
        let ack = common::message(
            "ACK.R01",
            &"a".repeat(256),
            &format!(
                "<ACK><ACK.type_cd V=\"AA\"/>{}</ACK>",
                common::field("ACK.ack_control_id", end_id)
            ),
        );
        let closed = step(&mut full, received(&ack)?)?;
        assert!(matches!(
            closed.last(),
            Some(DialogAction::Close { normal: true, .. })
        ));
    }
    // A fresh conversation may reuse IDs from the previous one.
    config.message_bytes = 65536;
    startup(&mut Dialog::new(config, ControlId::new("new-session")?)?)?;
    Ok(())
}
