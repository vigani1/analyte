//! Tests mutation adr0003 query parked receipt requires one retained unresolved target; mutation adr0003 query connection lookup requires interpretation and exact scope.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

#[test]
fn mutation_adr0003_query_parked_receipt_requires_one_retained_unresolved_target() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let first = receipt(b"first")?;
    let second = receipt(b"second")?;
    store.receive(&first, b"first", b"config")?;
    store.receive(&second, b"second", b"config")?;
    let target = |id: ReceiptId| Identifier::new(&uuid::Uuid::from(id).to_string());
    assert!(matches!(
        store.parked_receipt(999),
        Err(Error::ReprocessParked)
    ));
    for (targets, expected) in [
        (vec![], None),
        (vec![Identifier::new("not-a-receipt")?], None),
        (vec![target(receipt(b"missing")?.id)?], None),
        (vec![target(first.id)?], Some(first.id)),
        (vec![target(first.id)?, target(second.id)?], None),
    ] {
        let id = store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new("query regression")?,
            targets,
        })?;
        match expected {
            Some(expected) => assert_eq!(store.parked_receipt(id)?, expected),
            None => assert!(matches!(
                store.parked_receipt(id),
                Err(Error::ReprocessParked)
            )),
        }
    }
    let id = store.park(&Parked {
        kind: Identifier::new("synthetic")?,
        reason: Reason::new("query regression")?,
        targets: vec![target(first.id)?],
    })?;
    store.resolve(
        id,
        &Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("resolve")?,
            targets: vec![],
            time: 1,
            reason: Reason::new("reviewed")?,
        },
    )?;
    assert!(matches!(
        store.parked_receipt(id),
        Err(Error::ReprocessParked)
    ));
    assert_eq!(store.raw(first.id)?, Some(b"first".to_vec()));
    Ok(())
}

#[test]
fn mutation_adr0003_query_connection_lookup_requires_interpretation_and_exact_scope() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let first = receipt(b"uninterpreted")?;
    let second = receipt(b"interpreted")?;
    store.receive(&first, b"uninterpreted", b"config")?;
    assert!(
        store
            .first_interpreted_connection_receipt(&first.machine, &first.connection)?
            .is_none()
    );
    store.receive(&second, b"interpreted", b"config")?;
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![key(&second.id)?, key(&interpretation()?)?],
    )?;
    let found = store
        .first_interpreted_connection_receipt(&first.machine, &first.connection)?
        .ok_or("interpreted receipt missing")?;
    assert_eq!(found.metadata.id, second.id);
    assert_eq!(found.raw, b"interpreted");
    assert_eq!(found.config, b"config");
    assert!(
        store
            .first_interpreted_connection_receipt(&Identifier::new("other")?, &first.connection)?
            .is_none()
    );
    assert!(
        store
            .first_interpreted_connection_receipt(&first.machine, &Identifier::new("other")?)?
            .is_none()
    );
    Ok(())
}

#[test]
fn mutation_adr0003_query_receipt_checks_each_retained_identity_and_digest() -> TestResult {
    for field in ["id", "raw_digest", "config_digest"] {
        let temp = tempfile::tempdir()?;
        let mut store = open(&temp.path().join("store.db"))?;
        let metadata = receipt(b"raw")?;
        store.receive(&metadata, b"raw", b"config")?;
        let found = store.receipt(metadata.id)?.ok_or("receipt missing")?;
        assert_eq!(found.metadata.id, metadata.id);
        assert_eq!(found.raw, b"raw");
        assert_eq!(found.config, b"config");
        assert_eq!(
            store
                .receipt_metadata(metadata.id)?
                .ok_or("metadata missing")?
                .id,
            metadata.id
        );
        let mut corrupt = metadata.clone();
        match field {
            "id" => corrupt.id = receipt(b"different")?.id,
            "raw_digest" => corrupt.raw_digest = digest(b"different"),
            "config_digest" => corrupt.config_digest = digest(b"different"),
            _ => unreachable!(),
        }
        // Deliberate on-disk corruption, bypassing the immutable-evidence guard.
        store
            .db
            .execute_batch("DROP TRIGGER immutable_receipts_update")?;
        store.db.execute(
            "UPDATE receipts SET metadata=?1 WHERE id=?2",
            params![key(&corrupt)?, key(&metadata.id)?],
        )?;
        assert!(
            matches!(store.receipt(metadata.id), Err(Error::Corruption)),
            "{field}"
        );
        if field == "id" {
            assert!(matches!(
                store.receipt_metadata(metadata.id),
                Err(Error::Corruption)
            ));
        }
    }
    Ok(())
}

#[test]
fn mutation_rule8_query_cached_order_checks_identity_without_authorizing_resend() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let app = Identifier::new("app")?;
    let order = CachedOrder {
        id: Identifier::new("order")?,
        sample: SampleId::new("001")?,
        document: "retained".into(),
        tombstone: true,
        expiry: 100,
    };
    store.order_page(&OrderPage {
        destination: app.clone(),
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 1,
        orders: vec![serde_json::from_value(serde_json::to_value(&order)?)?],
    })?;
    let found = store
        .cached_order(&app, &order.id)?
        .ok_or("missing order")?;
    assert_eq!(found.id, order.id);
    assert_eq!(found.sample.as_str(), "001");
    assert!(found.tombstone);
    assert_eq!(found.document, "retained");
    assert!(
        store
            .cached_order(&app, &Identifier::new("missing")?)?
            .is_none()
    );
    let mut corrupt = order;
    corrupt.id = Identifier::new("different")?;
    store.db.execute(
        "UPDATE orders SET evidence=?1 WHERE destination=?2 AND id=?3",
        params![key(&corrupt)?, app.as_str(), found.id.as_str()],
    )?;
    assert!(matches!(
        store.cached_order(&app, &found.id),
        Err(Error::Corruption)
    ));
    Ok(())
}

#[test]
fn mutation_adr0003_query_pages_preserve_evidence_and_resume_after_the_cursor() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let first = receipt(b"first")?;
    let second = receipt(b"second")?;
    store.receive(&first, b"first", b"config")?;
    store.receive(&second, b"second", b"config")?;
    let transfer = Identifier::new("transfer")?;
    store.frame(&transfer, FrameSequence::new(1, 7)?, b"frame one")?;
    store.frame(&transfer, FrameSequence::new(2, 7)?, b"frame two")?;
    let a = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("review")?,
        targets: vec![],
        time: 1,
        reason: Reason::new("first review")?,
    };
    store.audit(&a)?;
    let b = Audit {
        time: 2,
        reason: Reason::new("second review")?,
        ..a
    };
    store.audit(&b)?;
    for reason in ["first parked", "second parked"] {
        store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new(reason)?,
            targets: vec![],
        })?;
    }
    let evidence = interpretation()?;
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![key(&first.id)?, key(&evidence)?],
    )?;
    assert_eq!(
        key(&store
            .interpretation(first.id, 1)?
            .ok_or("missing interpretation")?)?,
        key(&evidence)?
    );
    assert!(store.interpretation(first.id, 2)?.is_none());
    store.limits.page_rows = 1;
    let frames = store.frames(&transfer, 0)?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].number, 1);
    assert_eq!(frames[0].raw, b"frame one");
    let frames = store.frames(&transfer, frames[0].sequence)?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].number, 2);
    assert_eq!(frames[0].raw, b"frame two");
    assert!(store.frames(&transfer, frames[0].sequence)?.is_empty());
    let rows = store.receipts("")?;
    assert_eq!(rows.len(), 1);
    let next = store.receipts(&key(&rows[0].id)?)?;
    assert_eq!(next.len(), 1);
    assert_ne!(next[0].id, rows[0].id);
    assert!([first.id, second.id].contains(&rows[0].id));
    assert!([first.id, second.id].contains(&next[0].id));
    assert!(store.receipts(&key(&next[0].id)?)?.is_empty());
    verify_parked_and_audit_pages(&store)?;
    Ok(())
}

#[test]
fn mutation_rule8_query_order_events_are_deliveries_and_reset_invalidates_only_the_cursor()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let app = Identifier::new("app")?;
    assert!(store.cursor(&app)?.is_none());
    store.order_page(&OrderPage {
        destination: app.clone(),
        cursor: Identifier::new("actual-cursor")?,
        complete: true,
        polled: 1,
        orders: vec![],
    })?;
    assert_eq!(store.cursor(&app)?.as_deref(), Some("actual-cursor"));
    let event = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: app.clone(),
        order_id: Identifier::new("order")?,
        payload: b"original event".to_vec(),
        contract_version: 1,
    };
    store.enqueue_order_event(&event, 2)?;
    let delivery = store.claim(2)?.ok_or("event was not queued")?;
    assert_eq!(delivery.id, event.id);
    assert_eq!(delivery.payload, b"original event");
    assert_eq!(delivery.kind, DeliveryKind::OrderEvent);
    let count: i64 = store
        .db
        .query_row("SELECT count(*) FROM interpretations", [], |r| r.get(0))?;
    assert_eq!(count, 0);
    store.reset_orders(&app)?;
    assert!(store.cursor(&app)?.is_none());
    Ok(())
}

fn verify_parked_and_audit_pages(store: &Store) -> TestResult {
    let rows = store.parked(0)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].details()?.reason.as_str(), "first parked");
    let next = store.parked(rows[0].id)?;
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].details()?.reason.as_str(), "second parked");
    assert!(store.parked(next[0].id)?.is_empty());
    let rows = store.audits(0)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1.reason.as_str(), "first review");
    let next = store.audits(rows[0].0)?;
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].1.reason.as_str(), "second review");
    assert!(store.audits(next[0].0)?.is_empty());
    Ok(())
}
