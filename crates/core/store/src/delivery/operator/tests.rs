//! Tests delivery listing, audit and resend identity.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

// Plan §11; ADR 0008
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One retained-evidence scenario checks refusals and rollback alongside its successful lifecycle"
)]
fn resend_keeps_identity_bytes_and_audits_atomically() -> TestResult {
    let app = Identifier::new("app")?;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("resend.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let result = production_result(metadata.id)?;
    let observation = match &result.identity {
        analyte_model::IdentityStatus::Resolved(id) => id.observation.clone(),
        analyte_model::IdentityStatus::Unresolved(_) => return Err("fixture identity".into()),
    };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let delivery = Delivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new("app")?,
        ordering_key: OrderingKey {
            machine: metadata.machine.clone(),
            observation,
        },
        payload: b"immutable synthetic bytes".to_vec(),
        contract_version: 1,
    };
    store
        .reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence.clone(),
            ReconcileInput::standard(&[false], &[]),
            |_, _| {
                Ok(vec![Delivery {
                    id: delivery.id,
                    destination: delivery.destination.clone(),
                    ordering_key: OrderingKey {
                        machine: delivery.ordering_key.machine.clone(),
                        observation: delivery.ordering_key.observation.clone(),
                    },
                    payload: delivery.payload.clone(),
                    contract_version: delivery.contract_version,
                }])
            },
        )
        .map(|commit| commit.version)?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("delivery-resend")?,
        targets: vec![Identifier::new(&format!(
            "delivery:{}",
            uuid::Uuid::from(delivery.id)
        ))?],
        time: 50,
        reason: Reason::new("app data recovered; resend requested")?,
    };
    assert!(matches!(
        store.resend_delivery(delivery.id, false, &audit),
        Err(DeliveryError::ResendState)
    ));
    let first = store.claim_destination(0, &app)?.ok_or("missing claim")?;
    assert!(matches!(
        store.resend_delivery(delivery.id, false, &audit),
        Err(DeliveryError::ResendState)
    ));
    store.attempted(delivery.id, &Attempt::Done, 0)?;
    // An audit write failure must roll back the requeue, not just the audit.
    store.db.execute_batch("CREATE TRIGGER reject_resend_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;")?;
    {
        let refusal = store
            .resend_delivery(delivery.id, false, &audit)
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(
                &refusal,
                DeliveryError::Storage(StorageError::ConstraintViolation(_))
            ),
            "unexpected refusal: {refusal:?}"
        );
    }
    assert_eq!(
        store
            .deliveries(0)?
            .first()
            .ok_or("missing delivery")?
            .state,
        DeliveryState::Done
    );
    store
        .db
        .execute_batch("DROP TRIGGER reject_resend_audit;")?;
    for outcome in [
        Attempt::Done,
        Attempt::Parked(Reason::new("rejection")?),
        Attempt::Paused(Reason::new("setup")?),
    ] {
        store.resend_delivery(delivery.id, false, &audit)?;
        let retry = store.claim_destination(50, &app)?.ok_or("missing retry")?;
        assert_eq!(retry.id, first.id);
        assert_eq!(retry.payload, first.payload);
        store.attempted(retry.id, &outcome, 50)?;
    }
    assert_eq!(store.audits(0)?.len(), 3);
    assert_eq!(
        store.audits(0)?.first().ok_or("missing audit")?.1.targets,
        audit.targets
    );
    assert!(
        store
            .deliveries(
                store
                    .deliveries(0)?
                    .first()
                    .ok_or("missing delivery")?
                    .sequence
            )?
            .is_empty()
    );
    verify_unknown_resend_keeps_audit_count(&mut store, audit)
}

fn verify_unknown_resend_keeps_audit_count(store: &mut Store, mut audit: Audit) -> TestResult {
    let unknown = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    audit.targets = vec![Identifier::new(&format!(
        "delivery:{}",
        uuid::Uuid::from(unknown)
    ))?];
    assert!(matches!(
        store.resend_delivery(unknown, false, &audit),
        Err(DeliveryError::UnknownDelivery)
    ));
    assert_eq!(store.audits(0)?.len(), 3);
    Ok(())
}

// Plan §11; ADR 0003
#[test]
fn superseded_resend_requires_explicit_delivery() -> TestResult {
    let app = Identifier::new("app")?;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("superseded.db"))?;
    let metadata = receipt(b"raw")?;
    store.receive(&metadata, b"raw", b"config")?;
    let result = production_result(metadata.id)?;
    let observation = match &result.identity {
        analyte_model::IdentityStatus::Resolved(id) => id.observation.clone(),
        analyte_model::IdentityStatus::Unresolved(_) => return Err("fixture identity".into()),
    };
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    let delivery = Delivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new("app")?,
        ordering_key: OrderingKey {
            machine: metadata.machine.clone(),
            observation,
        },
        payload: b"old bytes".to_vec(),
        contract_version: 1,
    };
    store
        .reconcile(
            ReconcileAction::Interpret(metadata.id),
            evidence.clone(),
            ReconcileInput::standard(&[false], &[]),
            |_, _| {
                Ok(vec![Delivery {
                    id: delivery.id,
                    destination: delivery.destination.clone(),
                    ordering_key: OrderingKey {
                        machine: delivery.ordering_key.machine.clone(),
                        observation: delivery.ordering_key.observation.clone(),
                    },
                    payload: delivery.payload.clone(),
                    contract_version: delivery.contract_version,
                }])
            },
        )
        .map(|commit| commit.version)?;
    store.claim_destination(0, &app)?.ok_or("missing claim")?;
    store.attempted(delivery.id, &Attempt::Done, 0)?;
    store.db.execute(
        "INSERT INTO interpretations VALUES (?1,2,?2)",
        params![key(&metadata.id)?, document(&evidence)?],
    )?;
    // A correction need not have an outbox row to supersede the older revision.
    let identity = key(&(
        &delivery.ordering_key.machine,
        &delivery.ordering_key.observation,
    ))?;
    store.db.execute(
        "INSERT INTO observation_revisions VALUES (?1,1,?2,2,?3)",
        params![
            identity,
            key(&metadata.id)?,
            digest(b"corrected").0.as_slice()
        ],
    )?;
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("delivery-resend")?,
        targets: vec![Identifier::new(&format!(
            "delivery:{}",
            uuid::Uuid::from(delivery.id)
        ))?],
        time: 50,
        reason: Reason::new("explicit old revision recovery")?,
    };
    assert!(matches!(
        store.resend_delivery(delivery.id, false, &audit),
        Err(DeliveryError::SupersededDelivery)
    ));
    assert!(store.audits(0)?.is_empty());
    store.resend_delivery(delivery.id, true, &audit)?;
    assert_eq!(
        store
            .claim_destination(50, &app)?
            .ok_or("missing resend")?
            .payload,
        delivery.payload
    );
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

// ADR 0008; mutation-testing regression
#[test]
fn resend_requires_action_and_exact_delivery_target() -> TestResult {
    let app = Identifier::new("app")?;
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("resend-authorization.db"))?;
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
    store
        .claim_destination(0, &app)?
        .ok_or("missing initial claim")?;
    store.attempted(id, &Attempt::Done, 0)?;
    let target = Identifier::new(&format!("delivery:{}", uuid::Uuid::from(id)))?;
    for (action, target) in [
        ("wrong-action", target.clone()),
        ("delivery-resend", Identifier::new("delivery:wrong")?),
    ] {
        let audit = Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new(action)?,
            targets: vec![target],
            time: 42,
            reason: Reason::new("synthetic resend")?,
        };
        assert!(matches!(
            store.resend_delivery(id, false, &audit),
            Err(DeliveryError::AuditMismatch)
        ));
        assert_eq!(
            store
                .delivery(id)?
                .ok_or("missing retained delivery")?
                .state,
            DeliveryState::Done
        );
        assert!(store.audits(0)?.is_empty());
    }
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("delivery-resend")?,
        targets: vec![target],
        time: 42,
        reason: Reason::new("synthetic resend")?,
    };
    store.resend_delivery(id, false, &audit)?;
    let claim = store.claim_destination(42, &app)?.ok_or("missing resend")?;
    assert_eq!(claim.id, id);
    assert_eq!(claim.payload, b"immutable bytes");
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

// Owner request 2026-11-20 (admin restyle b5): the Deliveries page lists each
// delivery with when it was queued, its machines and, for a result, its result
// count and sample IDs as stored; the undelivered filter keeps every state but
// done, and an unrecorded queue time stays unknown.
#[test]
fn listed_deliveries_say_what_each_carries_and_filter_undelivered() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = open(&temp.path().join("listed.db"))?;
    assert!(store.listed_deliveries(0, DeliveryFilter::All)?.is_empty());
    assert_eq!(store.delivery_counts()?, DeliveryCounts::default());
    let metadata = receipt(b"raw")?;
    let mut store = store;
    store.receive(&metadata, b"raw", b"config")?;
    let mut result = production_result(metadata.id)?;
    if let Some(sample) = &mut result.sample {
        sample.id = SampleId::new("000123")?;
    }
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![result]);
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        rusqlite::params![key(&metadata.id)?, document(&evidence)?],
    )?;
    let done = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    let pending = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    for (id, state, backfilled) in [(done, "done", 0), (pending, "pending", 1)] {
        store.db.execute(
            "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at,enqueue_time_backfilled) VALUES (?1,'result',?2,1,'app','k',x'00',x'00',1,?3,0,100,?4)",
            rusqlite::params![key(&id)?, key(&metadata.id)?, state, backfilled],
        )?;
    }
    store.db.execute(
        "INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','order-1','second-machine','1','Sent',1,1,'x')",
        [],
    )?;
    let event = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    store.db.execute(
        "INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,'order_event','order-1','app','order-1',x'00',x'00',1,'paused',0,200)",
        [key(&event)?],
    )?;
    let all = store.listed_deliveries(0, DeliveryFilter::All)?;
    let ids: Vec<DeliveryId> = all.iter().map(|row| row.delivery.id).collect();
    assert_eq!(ids, [done, pending, event]);
    let first = all.first().ok_or("first delivery")?;
    assert_eq!(first.kind, DeliveryKind::Result);
    assert_eq!(first.queued_at, Some(100));
    assert_eq!(first.machines, std::slice::from_ref(&metadata.machine));
    assert_eq!(
        first.reading,
        Some(RecentOutcome::Results {
            results: 1,
            samples: vec![SampleId::new("000123")?],
        })
    );
    assert_eq!(all.get(1).ok_or("second delivery")?.queued_at, None);
    let order = all.get(2).ok_or("order message")?;
    assert_eq!(order.kind, DeliveryKind::OrderEvent);
    assert_eq!(order.machines, [Identifier::new("second-machine")?]);
    assert_eq!(order.reading, None);
    assert_eq!(order.delivery.state, DeliveryState::Paused);
    let open: Vec<DeliveryId> = store
        .listed_deliveries(0, DeliveryFilter::Undelivered)?
        .iter()
        .map(|row| row.delivery.id)
        .collect();
    assert_eq!(open, [pending, event]);
    assert_eq!(
        store.delivery_counts()?,
        DeliveryCounts {
            all: 3,
            undelivered: 2,
        }
    );
    // Paged after the cursor and bounded by one page.
    let after = first.delivery.sequence;
    assert_eq!(
        store.listed_deliveries(after, DeliveryFilter::All)?.len(),
        2
    );
    store.limits.page_rows = 1;
    assert_eq!(store.listed_deliveries(0, DeliveryFilter::All)?.len(), 1);
    Ok(())
}
