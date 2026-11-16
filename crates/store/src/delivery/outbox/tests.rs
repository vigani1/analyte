//! delivery/outbox regression tests kept beside the storage behavior they guard.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn adr0008_outbox_and_interpretation_rollback_together() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    // A deliberately failing trigger simulates an outbox constraint failure after
    // interpretation insertion, proving that the whole transaction rolls back.
    store.db.execute_batch("CREATE TRIGGER reject_interpretation AFTER INSERT ON interpretations BEGIN INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('x',NEW.receipt,NEW.version,'d','k',x'00',x'00',1,'invalid',0); END;")?;
    assert!(
        store
            .interpret_reconciled(metadata.id, interpretation()?, &[], &[], |_, _| Ok(
                Vec::new()
            ))
            .is_err()
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM interpretations", [], |r| r
                .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn adr0008_parked_head_blocks_correction_but_not_other_machine() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.interpret_reconciled(metadata.id, interpretation()?, &[], &[], |_, _| {
        Ok(Vec::new())
    })?;
    let first = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    let correction = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    let other = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    seed_delivery(&store, metadata.id, first, "machine-a/observation")?;
    seed_delivery(&store, metadata.id, correction, "machine-a/observation")?;
    seed_delivery(&store, metadata.id, other, "machine-b/observation")?;
    let claim = store.claim(0)?.ok_or(Error::Corruption)?;
    assert_eq!(claim.id, first);
    assert_eq!(claim.payload, vec![0, 17, 34]);
    store.attempted(
        first,
        &Attempt::Parked(Reason::new("synthetic rejection")?),
        0,
    )?;
    assert_eq!(store.claim(0)?.ok_or(Error::Corruption)?.id, other);
    assert!(store.claim(0)?.is_none());
    Ok(())
}

#[test]
fn adr0008_public_enqueue_is_atomic_and_retry_keeps_id_and_bytes() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    let delivery = Delivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new("app")?,
        ordering_key: OrderingKey {
            machine: Identifier::new("synthetic-machine")?,
            observation: match production_result(metadata.id)?.identity {
                analyte_model::IdentityStatus::Resolved(id) => id.observation,
                analyte_model::IdentityStatus::Unresolved(_) => {
                    return Err(Error::InvalidInput.into());
                }
            },
        },
        payload: b"exact wire bytes".to_vec(),
        contract_version: 1,
    };
    let id = delivery.id;
    assert_eq!(
        store
            .interpret_reconciled(metadata.id, evidence.clone(), &[false], &[], |_, _| Ok(
                vec![Delivery {
                    id: delivery.id,
                    destination: delivery.destination.clone(),
                    ordering_key: OrderingKey {
                        machine: delivery.ordering_key.machine.clone(),
                        observation: delivery.ordering_key.observation.clone()
                    },
                    payload: delivery.payload.clone(),
                    contract_version: delivery.contract_version,
                }]
            ))
            .map(|commit| commit.version)?,
        1
    );
    assert!(
        store
            .interpret_reconciled(metadata.id, evidence.clone(), &[false], &[], |_, _| Ok(
                vec![Delivery {
                    id: delivery.id,
                    destination: delivery.destination.clone(),
                    ordering_key: OrderingKey {
                        machine: delivery.ordering_key.machine.clone(),
                        observation: delivery.ordering_key.observation.clone()
                    },
                    payload: delivery.payload.clone(),
                    contract_version: delivery.contract_version,
                }]
            ))
            .map(|commit| commit.version)
            .is_err()
    );
    assert!(store.interpretation(metadata.id, 2)?.is_none());
    let first = store.claim(0)?.ok_or(Error::Corruption)?;
    store.attempted(id, &Attempt::RetryAt(10), 0)?;
    assert!(store.claim(9)?.is_none());
    let retry = store.claim(10)?.ok_or(Error::Corruption)?;
    assert_eq!(first.id, retry.id);
    assert_eq!(first.payload, retry.payload);
    assert!(
        store
            .db
            .execute("UPDATE outbox SET payload=x'00'", [])
            .is_err()
    );
    store.attempted(id, &Attempt::Done, 10)?;
    assert!(store.claim(20)?.is_none());
    Ok(())
}

#[test]
fn adr0008_event_provenance_health_ordering_and_restart_attempts() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("events.db");
    let mut store = open(&path)?;
    let destination = Identifier::new("app")?;
    let event = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: destination.clone(),
        order_id: Identifier::new("order-a")?,
        payload: b"event".to_vec(),
        contract_version: 1,
    };
    store.enqueue_order_event(&event, 10)?;
    assert!(store.enqueue_order_event(&event, 10).is_err());
    let second = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        ..event
    };
    store.enqueue_order_event(&second, 11)?;
    let other = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        order_id: Identifier::new("order-b")?,
        ..second
    };
    store.enqueue_order_event(&other, 12)?;
    let claim = store.claim(12)?.ok_or(Error::Corruption)?;
    assert_eq!(claim.kind, DeliveryKind::OrderEvent);
    assert_eq!(claim.earlier_attempts, 0);
    store.attempted(claim.id, &Attempt::RetryAt(20), 12)?;
    assert_eq!(store.claim(12)?.ok_or(Error::Corruption)?.id, other.id);
    let health = store.health(&destination, 15, 30)?;
    assert_eq!(
        (
            health.pending_count,
            health.pending_bytes,
            health.oldest_pending_age
        ),
        (3, 15, Some(5))
    );
    assert!(!health.paused);
    assert!(!health.cache_trusted);
    store.order_page(&OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: false,
        polled: 15,
        orders: vec![],
    })?;
    let health = store.health(&destination, 15, 30)?;
    assert_eq!(health.last_successful_poll, Some(15));
    assert!(!health.snapshot_complete);
    assert!(!health.cache_trusted);
    store.order_page(&OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("next")?,
        complete: true,
        polled: 16,
        orders: vec![],
    })?;
    assert!(store.health(&destination, 16, 30)?.cache_trusted);
    store.reset_orders(&destination)?;
    assert!(!store.health(&destination, 16, 30)?.cache_trusted);

    drop(store);
    let mut store = open(&path)?;
    let retry = store.claim(20)?.ok_or(Error::Corruption)?;
    assert_eq!(retry.id, event.id);
    assert_eq!(retry.earlier_attempts, 1);
    store.attempted(retry.id, &Attempt::Paused(Reason::new("credentials")?), 20)?;
    assert!(store.health(&destination, 21, 30)?.paused);
    assert!(store.claim(21)?.is_none());
    // CHECK constraints reject both kinds carrying the other's provenance.
    assert!(store.db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('bad','order_event','receipt',1,'o','app','o',x'00',x'00',1,'pending',0)",[]).is_err());
    assert!(store.db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES ('bad','result',NULL,NULL,'o','app','o',x'00',x'00',1,'pending',0)",[]).is_err());
    Ok(())
}

#[test]
fn adr0003_withdrawal_assigns_next_revision_and_retransmission_enqueues_nothing() -> TestResult {
    use analyte_model::*;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("withdrawal.db"))?;
    for (withdrawn, expected) in [
        (false, RevisionOutcome::New(Revision(0))),
        (true, RevisionOutcome::Correction(Revision(1))),
        (true, RevisionOutcome::Retransmission(Revision(1))),
    ] {
        let metadata = receipt(b"raw")?;
        store.receive(&metadata, b"raw", b"config")?;
        let mut result = production_result(metadata.id)?;
        if withdrawn {
            let mut parts = result.parts.as_slice().to_vec();
            for part in &mut parts {
                part.status = ResultStatus::Withdrawn;
                part.source_status = SourceText::new("D")?;
            }
            result.parts = NonEmptyEvidenceList::new(EvidenceList::new(parts)?)?;
        }
        let mut evidence = interpretation()?;
        evidence.outcome = Reading::Results(vec![result]);
        let commit = store.interpret_reconciled(
            metadata.id,
            evidence,
            &[false],
            &[],
            |evidence, outcomes| {
                let Reading::Results(results) = &evidence.outcome else {
                    return Err(Error::InvalidInput);
                };
                if matches!(outcomes.first(), Some(RevisionOutcome::Retransmission(_))) {
                    return Ok(Vec::new());
                }
                let Some(IdentityStatus::Resolved(identity)) = results.first().map(|r| &r.identity)
                else {
                    return Err(Error::InvalidInput);
                };
                Ok(vec![Delivery {
                    id: DeliveryId::try_from(uuid::Uuid::now_v7())
                        .map_err(|_| Error::InvalidInput)?,
                    destination: Identifier::new("app").map_err(|_| Error::InvalidInput)?,
                    ordering_key: OrderingKey {
                        machine: metadata.machine.clone(),
                        observation: identity.observation.clone(),
                    },
                    payload: format!("revision:{}", identity.revision.0).into_bytes(),
                    contract_version: 1,
                }])
            },
        )?;
        assert_eq!(commit.outcomes, vec![expected]);
    }
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        2
    );
    Ok(())
}

#[test]
fn adr0008_recovery_audits_interrupted_attempts_and_preserves_blocked_items() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("recover.db");
    let mut store = open(&path)?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    store.interpret_reconciled(metadata.id, interpretation()?, &[], &[], |_, _| {
        Ok(Vec::new())
    })?;
    let mut interrupted = None;
    for state in ["in_flight", "parked", "conflict", "paused", "done"] {
        let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
        seed_delivery(&store, metadata.id, id, state)?;
        store.db.execute(
            "UPDATE outbox SET state=?2 WHERE id=?1",
            params![key(&id)?, state],
        )?;
        if state == "in_flight" {
            interrupted = Some(id);
        }
    }
    drop(store);
    let mut store = open(&path)?;
    let audit = Audit {
        actor: Identifier::new("startup")?,
        action: Identifier::new("recover")?,
        targets: vec![],
        time: 42,
        reason: Reason::new("uncertain (interrupted); ADR 0008 idempotent receiver")?,
    };
    assert_eq!(store.recover_in_flight(42, &audit)?, 1);
    assert_eq!(store.recover_in_flight(42, &audit)?, 0);
    let id = interrupted.ok_or("missing interrupted ID")?;
    let attempts: (String, i64) = store.db.query_row(
        "SELECT outcome,time FROM attempts WHERE delivery=?1",
        [key(&id)?],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(attempts, (key(&Attempt::Interrupted)?, 42));
    let audits = store.audits(0)?;
    let (_, entry) = audits.first().ok_or("missing audit")?;
    assert_eq!(
        entry.targets,
        vec![Identifier::new(&uuid::Uuid::from(id).to_string())?]
    );
    assert!(entry.reason.as_str().contains("ADR 0008"));
    for state in ["parked", "conflict", "paused", "done"] {
        let count: i64 =
            store
                .db
                .query_row("SELECT count(*) FROM outbox WHERE state=?1", [state], |r| {
                    r.get(0)
                })?;
        assert_eq!(count, 1);
    }
    // A paused destination blocks even a recovered item until operator repair.
    assert!(store.claim(42)?.is_none());
    store
        .db
        .execute("UPDATE outbox SET state='done' WHERE state='paused'", [])?;
    let claim = store.claim(42)?.ok_or("not recovered")?;
    assert_eq!(claim.id, id);
    assert_eq!(claim.payload, [0, 17, 34]);
    assert_eq!(claim.earlier_attempts, 1);
    Ok(())
}

#[test]
fn mutation_adr0008_recovery_audit_bound_is_inclusive_and_refusal_keeps_the_claim() -> TestResult {
    for delta in [0, 1] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("recovery-audit-bound.db"))?;
        let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
        store.enqueue_order_event(
            &OrderEventDelivery {
                id,
                destination: Identifier::new("app")?,
                order_id: Identifier::new("order")?,
                payload: b"immutable bytes".to_vec(),
                contract_version: 1,
            },
            0,
        )?;
        store.claim(0)?.ok_or("missing initial claim")?;
        let audit = Audit {
            actor: Identifier::new("startup")?,
            action: Identifier::new("recover")?,
            targets: vec![Identifier::new(&uuid::Uuid::from(id).to_string())?],
            time: 42,
            reason: Reason::new("interrupted delivery")?,
        };
        store.limits.record_bytes = key(&audit)?.len().checked_sub(delta).ok_or("limit")?;
        let result = store.recover_in_flight(42, &audit);
        let state = store.delivery(id)?.ok_or("missing delivery")?.state;
        if delta == 0 {
            assert_eq!(result?, 1);
            assert_eq!(state, DeliveryState::Pending);
            let claim = store.claim(42)?.ok_or("missing recovered claim")?;
            assert_eq!(claim.id, id);
            assert_eq!(claim.payload, b"immutable bytes");
            assert_eq!(claim.earlier_attempts, 1);
            assert_eq!(store.audits(0)?.len(), 1);
        } else {
            assert!(matches!(result, Err(Error::InvalidInput)));
            assert_eq!(state, DeliveryState::InFlight);
            assert!(store.audits(0)?.is_empty());
            assert_eq!(
                store
                    .db
                    .query_row("SELECT count(*) FROM attempts", [], |r| r.get::<_, i64>(0))?,
                0
            );
        }
    }
    Ok(())
}
