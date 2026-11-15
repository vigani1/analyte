//! ADR 0003 rules 2/3/8: cursor custody is retained before transmission and never replayed.
use super::*;
use crate::tests::{TestResult, open};

fn audit(id: PollIntentId, reason: &str) -> Result<Audit, analyte_model::TextError> {
    Ok(Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("poll-reconciliation")?,
        targets: vec![Identifier::new(&format!("poll:{id}"))?],
        time: 100,
        reason: Reason::new(reason)?,
    })
}

#[test]
fn adr0003_poll_intent_pending_uncertain_confirmed_is_retained_and_blocks_cursor_replay()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("polls.db");
    let machine = Identifier::new("machine")?;
    let original = Reason::new("cursor may advance after admission")?;
    let mut store = open(&path)?;
    let id = store.begin_poll_intent(&machine, &original)?;
    let pending = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("missing admission")?;
    assert_eq!(pending.id, id);
    assert_eq!(pending.state, PollIntentState::Pending);
    assert_eq!(pending.reason, original);
    assert!(
        store
            .parked_item(pending.parked)?
            .ok_or("missing operator evidence")?
            .resolution
            .is_none()
    );
    assert!(matches!(
        store.begin_poll_intent(&machine, &original),
        Err(Error::PollIntentUnresolved)
    ));
    let other = Identifier::new("other")?;
    store.begin_poll_intent(&other, &original)?;
    let mut unrelated = audit(id, "unrelated operator target")?;
    unrelated.targets.clear();
    assert!(matches!(
        store.record_poll_outcome(id, PollOutcome::Confirmed, &unrelated),
        Err(Error::InvalidInput)
    ));
    let uncertain = store.record_poll_outcome(
        id,
        PollOutcome::Uncertain,
        &audit(id, "no reliable response before disconnect")?,
    )?;
    assert_eq!(uncertain.state, PollIntentState::Uncertain);
    assert_eq!(
        uncertain.reason.as_str(),
        "no reliable response before disconnect"
    );
    assert_eq!(
        store.db.query_row(
            "SELECT reason FROM poll_intent_history WHERE intent=?1 ORDER BY sequence LIMIT 1",
            [id.0],
            |r| r.get::<_, String>(0)
        )?,
        original.as_str()
    );
    drop(store);
    let mut store = open(&path)?;
    let retained = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("lost unresolved custody")?;
    assert_eq!(retained.id, id);
    assert_eq!(retained.state, PollIntentState::Uncertain);
    assert!(matches!(
        store.begin_poll_intent(&machine, &original),
        Err(Error::PollIntentUnresolved)
    ));
    let confirmed = store.record_poll_outcome(
        id,
        PollOutcome::Confirmed,
        &audit(id, "response is durably stored")?,
    )?;
    assert_eq!(confirmed.state, PollIntentState::Confirmed);
    assert!(store.unconfirmed_poll_intent(&machine)?.is_none());
    assert!(store.unconfirmed_poll_intent(&other)?.is_some());
    assert!(
        store
            .parked_item(confirmed.parked)?
            .ok_or("lost original operator evidence")?
            .resolution
            .is_some()
    );
    let late_cleanup = store.record_poll_outcome(
        id,
        PollOutcome::Uncertain,
        &audit(id, "late timeout after committed confirmation")?,
    )?;
    assert_eq!(late_cleanup.state, PollIntentState::Confirmed);
    assert_eq!(late_cleanup.reason.as_str(), "response is durably stored");
    store.record_poll_outcome(
        id,
        PollOutcome::Confirmed,
        &audit(id, "repeated confirmation after lost reply")?,
    )?;
    assert_eq!(
        store.db.query_row(
            "SELECT count(*) FROM poll_intent_history WHERE intent=?1",
            [id.0],
            |r| r.get::<_, i64>(0)
        )?,
        3
    );
    assert_eq!(store.audits(0)?.len(), 2);
    let next = store.begin_poll_intent(&machine, &original)?;
    assert_ne!(next, id);
    for table in ["outbox", "machine_orders"] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    assert!(
        store
            .db
            .execute(
                "UPDATE poll_intents SET state='pending' WHERE id=?1",
                [id.0]
            )
            .is_err()
    );
    assert!(store.db.execute("DELETE FROM poll_intents", []).is_err());
    assert!(
        store
            .db
            .execute("UPDATE poll_intent_history SET reason='changed'", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("DELETE FROM poll_intent_history", [])
            .is_err()
    );
    Ok(())
}

#[test]
fn adr0003_poll_admission_outcome_and_operator_resolution_roll_back_all_evidence_on_failure()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("atomic.db"))?;
    let machine = Identifier::new("machine")?;
    let reason = Reason::new("cursor may move")?;
    store.db.execute_batch("CREATE TRIGGER fail_poll_history BEFORE INSERT ON poll_intent_history BEGIN SELECT RAISE(ABORT,'synthetic history failure'); END;")?;
    assert!(store.begin_poll_intent(&machine, &reason).is_err());
    for table in ["parked", "poll_intents", "poll_intent_history"] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    store.db.execute_batch("DROP TRIGGER fail_poll_history")?;
    let id = store.begin_poll_intent(&machine, &reason)?;
    let parked = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("missing intent")?
        .parked;
    let evidence = audit(id, "operator reconciled cursor custody")?;
    store.db.execute_batch("CREATE TRIGGER fail_poll_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    for outcome in [PollOutcome::Uncertain, PollOutcome::Confirmed] {
        assert!(store.record_poll_outcome(id, outcome, &evidence).is_err());
        assert_eq!(
            store
                .unconfirmed_poll_intent(&machine)?
                .ok_or("lost intent")?
                .state,
            PollIntentState::Pending
        );
        assert!(
            store
                .parked_item(parked)?
                .ok_or("lost parked evidence")?
                .resolution
                .is_none()
        );
    }
    assert!(store.resolve(parked, &evidence).is_err());
    assert!(
        store
            .parked_item(parked)?
            .ok_or("lost parked evidence")?
            .resolution
            .is_none()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM poll_intent_history", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    store.db.execute_batch("DROP TRIGGER fail_poll_audit")?;
    store.resolve(parked, &evidence)?;
    assert!(store.unconfirmed_poll_intent(&machine)?.is_none());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM poll_intent_history", [], |r| r
                .get::<_, i64>(0))?,
        2
    );
    assert_eq!(store.audits(0)?.len(), 1);
    assert!(matches!(
        store.resolve(parked, &evidence),
        Err(Error::Conflict)
    ));
    Ok(())
}

#[test]
fn adr0003_poll_capacity_refusal_never_admits_or_reconciles_a_cursor_movement() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("capacity.db");
    let machine = Identifier::new("machine")?;
    let reason = Reason::new("poll may move cursor")?;
    let mut store = Store::open(
        &path,
        crate::tests::limits(),
        Box::new(crate::tests::Space(0)),
    )?;
    assert!(matches!(
        store.begin_poll_intent(&machine, &reason),
        Err(Error::Capacity)
    ));
    assert!(store.unconfirmed_poll_intent(&machine)?.is_none());
    assert!(store.parked(0)?.is_empty());
    drop(store);
    let mut store = open(&path)?;
    let id = store.begin_poll_intent(&machine, &reason)?;
    let parked = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("lost pending poll")?
        .parked;
    drop(store);
    let mut store = Store::open(
        &path,
        crate::tests::limits(),
        Box::new(crate::tests::Space(0)),
    )?;
    let audit = audit(id, "operator evidence")?;
    for outcome in [PollOutcome::Uncertain, PollOutcome::Confirmed] {
        assert!(matches!(
            store.record_poll_outcome(id, outcome, &audit),
            Err(Error::Capacity)
        ));
    }
    assert!(matches!(
        store.resolve(parked, &audit),
        Err(Error::Capacity)
    ));
    assert_eq!(
        store
            .unconfirmed_poll_intent(&machine)?
            .ok_or("lost pending poll")?
            .state,
        PollIntentState::Pending
    );
    assert!(
        store
            .parked_item(parked)?
            .ok_or("lost operator evidence")?
            .resolution
            .is_none()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM poll_intent_history", [], |row| row
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}
