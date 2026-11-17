//! Tests delivery listing, audit and resend identity.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One retained-evidence scenario checks refusals and rollback alongside its successful lifecycle"
)]
fn plan11_adr0008_resend_preserves_identity_bytes_and_atomic_audit() -> TestResult {
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
    let first = store.claim(0)?.ok_or("missing claim")?;
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
        let retry = store.claim(50)?.ok_or("missing retry")?;
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

#[test]
fn plan11_adr0003_superseded_resend_requires_explicit_delivery() -> TestResult {
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
    store.claim(0)?.ok_or("missing claim")?;
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
        store.claim(50)?.ok_or("missing resend")?.payload,
        delivery.payload
    );
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

#[test]
fn mutation_adr0008_resend_requires_both_action_and_exact_delivery_target() -> TestResult {
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
    store.claim(0)?.ok_or("missing initial claim")?;
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
    let claim = store.claim(42)?.ok_or("missing resend")?;
    assert_eq!(claim.id, id);
    assert_eq!(claim.payload, b"immutable bytes");
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}
