//! Tests order poll cursors and reconciliation.
use super::*;
use crate::test_support::assert_constraint;
use crate::test_support::{TestResult, open};

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
        Err(Error::AuditMismatch)
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
    verify_poll_evidence_is_immutable_and_has_no_deliveries(&store, id)
}

fn verify_poll_evidence_is_immutable_and_has_no_deliveries(
    store: &Store,
    id: PollIntentId,
) -> TestResult {
    for table in ["outbox", "machine_orders"] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
    }
    assert_constraint(store.db.execute(
        "UPDATE poll_intents SET state='pending' WHERE id=?1",
        [id.0],
    ));
    assert_constraint(store.db.execute("DELETE FROM poll_intents", []));
    assert_constraint(
        store
            .db
            .execute("UPDATE poll_intent_history SET reason='changed'", []),
    );
    assert_constraint(store.db.execute("DELETE FROM poll_intent_history", []));
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
    {
        let refusal = store
            .begin_poll_intent(&machine, &reason)
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, Error::ConstraintViolation(_)),
            "unexpected refusal: {refusal:?}"
        );
    }
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
        {
            let refusal = store
                .record_poll_outcome(id, outcome, &evidence)
                .map(|_| ())
                .expect_err("input must be refused");
            assert!(
                matches!(&refusal, Error::ConstraintViolation(_)),
                "unexpected refusal: {refusal:?}"
            );
        }
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
    {
        let refusal = store
            .resolve(parked, &evidence)
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(&refusal, crate::OperatorError::ConstraintViolation(_)),
            "unexpected refusal: {refusal:?}"
        );
    }
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
        Err(crate::OperatorError::Conflict)
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
        crate::test_support::limits(),
        Box::new(crate::test_support::Space(0)),
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
        crate::test_support::limits(),
        Box::new(crate::test_support::Space(0)),
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
        Err(crate::OperatorError::Capacity)
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

#[test]
fn mutation_adr0003_poll_state_reason_and_repeated_evidence_are_independent() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("poll-idempotence.db");
    let mut store = open(&path)?;
    let machine = Identifier::new("synthetic-machine")?;
    let initial = Reason::new("admitted cursor movement")?;
    let id = store.begin_poll_intent(&machine, &initial)?;
    let stored: i64 = store.db.query_row(
        "SELECT id FROM poll_intents WHERE machine=?1",
        [machine.as_str()],
        |r| r.get(0),
    )?;
    assert_eq!(id.to_string(), stored.to_string());
    let mut evidence = audit(id, initial.as_str())?;
    let uncertain = store.record_poll_outcome(id, PollOutcome::Uncertain, &evidence)?;
    assert_eq!(uncertain.state, PollIntentState::Uncertain);
    evidence.reason = Reason::new("new disconnect evidence")?;
    let updated = store.record_poll_outcome(id, PollOutcome::Uncertain, &evidence)?;
    assert_eq!(updated.reason, evidence.reason);
    store.record_poll_outcome(id, PollOutcome::Uncertain, &evidence)?;
    assert_eq!(store.audits(0)?.len(), 2);
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM poll_intent_history", [], |r| r
                .get::<_, i64>(0))?,
        3
    );
    let confirmed = store.record_poll_outcome(id, PollOutcome::Confirmed, &evidence)?;
    assert_eq!(confirmed.state, PollIntentState::Confirmed);
    assert!(
        store
            .parked_item(confirmed.parked)?
            .ok_or("missing custody evidence")?
            .resolution
            .is_some()
    );
    assert_eq!(store.audits(0)?.len(), 3);
    drop(store);
    let mut store = open(&path)?;
    assert!(store.unconfirmed_poll_intent(&machine)?.is_none());
    let retained =
        store.record_poll_outcome(id, PollOutcome::Uncertain, &audit(id, "late timeout")?)?;
    assert_eq!(retained.state, PollIntentState::Confirmed);
    assert_eq!(retained.reason.as_str(), "new disconnect evidence");
    assert_eq!(store.audits(0)?.len(), 3);
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM poll_intent_history", [], |r| r
                .get::<_, i64>(0))?,
        4
    );
    Ok(())
}

#[test]
fn mutation_adr0003_poll_reconciliation_preserves_the_disk_reserve_for_its_audit() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("poll-reserve.db"))?;
    let machine = Identifier::new("synthetic-machine")?;
    let id = store.begin_poll_intent(&machine, &Reason::new("admitted")?)?;
    let parked = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("missing intent")?
        .parked;
    let evidence = audit(id, "retained operator evidence")?;
    assert!(crate::document(&evidence)?.len() > 2);
    // Two available bytes above the reserve cannot hold even one retained audit.
    store.space = Box::new(crate::test_support::Space(
        store.limits.reserve_bytes.checked_add(2).ok_or("space")?,
    ));
    for outcome in [PollOutcome::Uncertain, PollOutcome::Confirmed] {
        assert!(matches!(
            store.record_poll_outcome(id, outcome, &evidence),
            Err(Error::Capacity)
        ));
    }
    assert!(matches!(
        store.resolve(parked, &evidence),
        Err(crate::OperatorError::Capacity)
    ));
    assert_eq!(
        store
            .unconfirmed_poll_intent(&machine)?
            .ok_or("missing intent")?
            .state,
        PollIntentState::Pending
    );
    assert!(
        store
            .parked_item(parked)?
            .ok_or("missing parked")?
            .resolution
            .is_none()
    );
    assert!(store.audits(0)?.is_empty());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM poll_intent_history", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    Ok(())
}
