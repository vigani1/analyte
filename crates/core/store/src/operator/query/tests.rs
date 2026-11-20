//! Tests protected evidence queries.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

// ADR 0003; mutation-testing regression
#[test]
fn parked_receipt_query_needs_one_unresolved_target() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let first = receipt(b"first")?;
    let second = receipt(b"second")?;
    store.receive(&first, b"first", b"config")?;
    store.receive(&second, b"second", b"config")?;
    let target = |id: ReceiptId| Identifier::new(&uuid::Uuid::from(id).to_string());
    assert!(matches!(
        store.parked_receipt(999),
        Err(OperatorError::ReprocessParked)
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
                Err(OperatorError::ReprocessParked)
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
        Err(OperatorError::ReprocessParked)
    ));
    assert_eq!(store.raw(first.id)?, Some(b"first".to_vec()));
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn connection_lookup_needs_interpretation_and_exact_scope() -> TestResult {
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
        params![key(&second.id)?, document(&interpretation()?)?],
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

// ADR 0003; mutation-testing regression
#[test]
fn receipt_query_checks_identity_and_each_digest() -> TestResult {
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
            params![document(&corrupt)?, key(&metadata.id)?],
        )?;
        assert!(
            matches!(store.receipt(metadata.id), Err(OperatorError::Corruption)),
            "{field}"
        );
        if field == "id" {
            assert!(matches!(
                store.receipt_metadata(metadata.id),
                Err(OperatorError::Corruption)
            ));
        }
    }
    Ok(())
}

// Safety rule 8 (ADR 0003); mutation-testing regression
#[test]
fn cached_order_query_never_authorizes_resend() -> TestResult {
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
        params![document(&corrupt)?, app.as_str(), found.id.as_str()],
    )?;
    assert!(matches!(
        store.cached_order(&app, &found.id),
        Err(OperatorError::Corruption)
    ));
    Ok(())
}

// ADR 0003; mutation-testing regression
#[test]
fn query_pages_keep_evidence_and_resume_after_cursor() -> TestResult {
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
        params![key(&first.id)?, document(&evidence)?],
    )?;
    assert_eq!(
        document(
            &store
                .interpretation(first.id, 1)?
                .ok_or("missing interpretation")?
        )?,
        document(&evidence)?
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

// Safety rule 8 (ADR 0003); mutation-testing regression
#[test]
fn order_events_are_deliveries_reset_clears_only_cursor() -> TestResult {
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
    let delivery = store
        .claim_destination(2, &app)?
        .ok_or("event was not queued")?;
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

// Owner request 2026-11-18 (remove a machine); AGENTS.md safety rules 2 and 8:
// before a machine stops being served, the store counts what is still open for it
// (undelivered results and order events, open orders, unread receipts, unresolved
// parked items linked to it) without changing anything, and ignores other machines.
#[test]
fn unfinished_work_is_counted_per_machine() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let machine = Identifier::new("synthetic-machine")?;
    assert_eq!(store.unfinished(&machine)?, Unfinished::default());
    assert!(!store.has_history(&machine)?);
    let parked = seed_machine_work(&mut store, &machine)?;
    let counted = store.unfinished(&machine)?;
    assert_eq!(
        counted,
        Unfinished {
            undelivered: 2,
            open_orders: 1,
            unread: 1,
            flagged: 4,
        }
    );
    assert!(!counted.is_empty());
    assert!(store.has_history(&machine)?);
    assert!(!store.has_history(&Identifier::new("never-used")?)?);
    // Counting changed nothing: every parked item is still listed and open.
    assert_eq!(store.parked(0)?.len(), parked);
    assert_eq!(store.unfinished(&machine)?, counted);
    Ok(())
}

// Owner request 2026-11-19 (results waiting after a removal); AGENTS.md safety
// rule 2: per destination, the store counts what the app has not accepted (sent by
// a worker, or held for an operator) and names the machines it came from; a
// delivery-only worker stops only when nothing but accepted items is left.
#[test]
fn undelivered_is_counted_per_destination() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let app = Identifier::new("app")?;
    assert!(store.undelivered_by_destination()?.is_empty());
    assert_eq!(store.undelivered(&app)?, 0);
    seed_machine_work(&mut store, &Identifier::new("synthetic-machine")?)?;
    assert_eq!(
        store.undelivered_by_destination()?,
        [Undelivered {
            destination: app.clone(),
            sending: 2,
            held: 1,
            machines: vec![
                Identifier::new("other-machine")?,
                Identifier::new("synthetic-machine")?
            ],
        }]
    );
    assert_eq!(store.undelivered(&app)?, 3);
    assert_eq!(store.undelivered(&Identifier::new("elsewhere")?)?, 0);
    store.db.execute("UPDATE outbox SET state='done'", [])?;
    assert!(store.undelivered_by_destination()?.is_empty());
    assert_eq!(store.undelivered(&app)?, 0);
    Ok(())
}

/// Stores open and finished work for `machine` and another machine; returns how
/// many parked items exist.
fn seed_machine_work(
    store: &mut Store,
    machine: &Identifier,
) -> Result<usize, Box<dyn std::error::Error>> {
    let unread = receipt(b"unread")?;
    let read = receipt(b"read")?;
    let mut other = receipt(b"other machine")?;
    other.machine = Identifier::new("other-machine")?;
    for (metadata, raw) in [
        (&unread, "unread"),
        (&read, "read"),
        (&other, "other machine"),
    ] {
        store.receive(metadata, raw.as_bytes(), b"config")?;
    }
    for id in [read.id, other.id] {
        store.db.execute(
            "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
            params![key(&id)?, document(&interpretation()?)?],
        )?;
    }
    let delivery = |receipt: ReceiptId,
                    state: &str|
     -> Result<DeliveryId, Box<dyn std::error::Error>> {
        let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
        store.db.execute(
            "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,'result',?2,1,'app','k',x'00',x'00',1,?3,0)",
            params![key(&id)?, key(&receipt)?, state],
        )?;
        Ok(id)
    };
    let pending = delivery(read.id, "pending")?;
    delivery(read.id, "done")?;
    delivery(other.id, "pending")?;
    for (order, owner, state) in [
        ("open", "synthetic-machine", "Uncertain"),
        ("finished", "synthetic-machine", "Done"),
        ("elsewhere", "other-machine", "Sent"),
    ] {
        store.db.execute(
            "INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app',?1,?2,'1',?3,1,1,'x')",
            params![order, owner, state],
        )?;
    }
    let open_order: i64 = store.db.query_row(
        "SELECT id FROM machine_orders WHERE order_id='open'",
        [],
        |r| r.get(0),
    )?;
    store.db.execute(
        "INSERT INTO outbox(id,kind,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,'order_event','open','app','open',x'00',x'00',1,'paused',0)",
        params![key(&DeliveryId::try_from(uuid::Uuid::now_v7())?)?],
    )?;
    let bare = |value: String| Identifier::new(&value);
    let flagged = [
        bare(uuid::Uuid::from(unread.id).to_string())?,
        machine.clone(),
        bare(uuid::Uuid::from(pending).to_string())?,
        bare(open_order.to_string())?,
    ];
    let unrelated = [
        bare(uuid::Uuid::from(other.id).to_string())?,
        Identifier::new("no-machine")?,
    ];
    let mut parked = Vec::new();
    for target in flagged.iter().chain(&unrelated) {
        parked.push(store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new("unfinished work")?,
            targets: vec![target.clone()],
        })?);
    }
    // A resolved item is finished, even though it names the machine.
    let resolved = store.park(&Parked {
        kind: Identifier::new("synthetic")?,
        reason: Reason::new("reviewed")?,
        targets: vec![machine.clone()],
    })?;
    store.resolve(
        resolved,
        &Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("resolve")?,
            targets: vec![],
            time: 1,
            reason: Reason::new("reviewed")?,
        },
    )?;
    Ok(parked.len().saturating_add(1))
}

/// Queues one order message for `destination` at `now` and records `outcome` for it.
fn attempt(
    store: &mut Store,
    destination: &Identifier,
    order: &str,
    outcome: &Attempt,
    now: i64,
) -> TestResult {
    let event = OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: destination.clone(),
        order_id: Identifier::new(order)?,
        payload: b"event".to_vec(),
        contract_version: 1,
    };
    store.enqueue_order_event(&event, now)?;
    let claim = store
        .claim_destination(now, destination)?
        .ok_or(DeliveryError::Corruption)?;
    store.attempted(claim.id, outcome, now)?;
    Ok(())
}

// Owner request 2026-11-19 (overview restyle): "Last delivered" is the latest
// accepted attempt per lab app; retries, refusals and other lab apps do not count,
// and an empty store says nothing was delivered.
#[test]
fn last_delivered_is_the_latest_accepted_attempt_per_destination() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    assert_eq!(store.last_delivered()?, LastDelivered::default());
    let main = Identifier::new("main-lab")?;
    let other = Identifier::new("outpatient")?;
    attempt(&mut store, &main, "order-a", &Attempt::Done, 100)?;
    attempt(&mut store, &main, "order-b", &Attempt::Done, 250)?;
    attempt(&mut store, &main, "order-c", &Attempt::RetryAt(900), 400)?;
    attempt(
        &mut store,
        &other,
        "order-d",
        &Attempt::Parked(Reason::new("refused")?),
        300,
    )?;
    let delivered = store.last_delivered()?;
    assert_eq!(delivered.at, vec![(main.clone(), 250)]);
    assert!(!delivered.purged);
    attempt(&mut store, &other, "order-e", &Attempt::Done, 320)?;
    assert_eq!(store.last_delivered()?.at, vec![(main, 250), (other, 320)]);
    Ok(())
}

// ADR 0013: data keeping removes delivered attempts and keeps no destination,
// so the query says older deliveries may exist instead of "never delivered".
#[test]
fn last_delivered_says_when_data_keeping_purged_deliveries() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = open(&temp.path().join("store.db"))?;
    store.db.execute(
        "INSERT INTO retention_orders VALUES (1,zeroblob(32),'m',zeroblob(32),NULL,NULL,1,2,3)",
        [],
    )?;
    store.db.execute(
        "INSERT INTO retention_order_deliveries VALUES ('purged',1,2)",
        [],
    )?;
    let delivered = store.last_delivered()?;
    assert!(delivered.at.is_empty());
    assert!(delivered.purged);
    Ok(())
}

/// Claims `destination`'s next due item at `now` and records `outcome` for it.
fn retry(store: &mut Store, destination: &Identifier, outcome: &Attempt, now: i64) -> TestResult {
    let claim = store
        .claim_destination(now, destination)?
        .ok_or(DeliveryError::Corruption)?;
    store.attempted(claim.id, outcome, now)?;
    Ok(())
}

// Owner request 2026-11-19 (overview: delivery failing): a lab app is failing
// while items wait and a try failed after its last accepted delivery; the
// facts are the first and last failed try, how many, and the next due try.
// A success after the failures, even in the same second, clears it, and a lab
// app with nothing waiting is never failing.
#[test]
fn failing_destinations_are_waiting_items_with_failed_tries_since_the_last_delivery() -> TestResult
{
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    assert_eq!(store.failing_destinations()?, Vec::new());
    let main = Identifier::new("main-lab")?;
    let other = Identifier::new("outpatient")?;
    let refused = Identifier::new("refusing")?;
    attempt(&mut store, &main, "order-a", &Attempt::Done, 100)?;
    attempt(&mut store, &main, "order-b", &Attempt::RetryAt(260), 200)?;
    retry(&mut store, &main, &Attempt::RetryAt(320), 260)?;
    attempt(&mut store, &other, "order-c", &Attempt::RetryAt(400), 300)?;
    attempt(&mut store, &other, "order-d", &Attempt::Done, 300)?;
    attempt(
        &mut store,
        &refused,
        "order-e",
        &Attempt::Parked(Reason::new("refused")?),
        300,
    )?;
    assert_eq!(
        store.failing_destinations()?,
        vec![FailingDestination {
            destination: main.clone(),
            since: 200,
            last: 260,
            tries: 2,
            next: Some(320),
        }]
    );
    // While its only waiting item is being tried, no next try is scheduled.
    let claim = store
        .claim_destination(320, &main)?
        .ok_or(DeliveryError::Corruption)?;
    assert_eq!(
        store
            .failing_destinations()?
            .first()
            .map(|failing| failing.next),
        Some(None)
    );
    store.attempted(claim.id, &Attempt::Done, 330)?;
    assert_eq!(store.failing_destinations()?, Vec::new());
    // A new failure after that delivery counts from itself.
    attempt(&mut store, &main, "order-f", &Attempt::RetryAt(400), 340)?;
    assert_eq!(
        store.failing_destinations()?,
        vec![FailingDestination {
            destination: main,
            since: 340,
            last: 340,
            tries: 1,
            next: Some(400),
        }]
    );
    Ok(())
}

// Owner request 2026-11-19 (admin restyle b1): the Needs review filter shows
// real counts of open and resolved items, and each filter pages through only its
// own items in item order. Reading changes nothing, and the open count is the
// one the parked-items alert reads.
#[test]
fn parked_items_are_counted_and_paged_as_open_or_resolved() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    assert_eq!(store.parked_counts()?, ParkedCounts::default());
    let mut ids = Vec::new();
    for reason in ["first", "second", "third", "fourth"] {
        ids.push(store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new(reason)?,
            targets: vec![],
        })?);
    }
    let resolve = |store: &mut Store, id: i64| -> TestResult {
        Ok(store.resolve(
            id,
            &Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("parked-resolve")?,
                targets: vec![Identifier::new(&id.to_string())?],
                time: 5,
                reason: Reason::new("settled")?,
            },
        )?)
    };
    resolve(&mut store, ids[1])?;
    resolve(&mut store, ids[3])?;
    let counts = store.parked_counts()?;
    assert_eq!(
        counts,
        ParkedCounts {
            open: 2,
            resolved: 2
        }
    );
    assert_eq!(
        store.alert_value(&AlertKey::ParkedItems, 0, &[])?,
        Some(counts.open)
    );
    store.limits.page_rows = 1;
    for (filter, expected) in [
        (ParkedFilter::Open, [ids[0], ids[2]]),
        (ParkedFilter::Resolved, [ids[1], ids[3]]),
    ] {
        let mut after = 0;
        let mut seen = Vec::new();
        loop {
            let page = store.parked_filtered(after, filter)?;
            let Some(last) = page.last() else { break };
            assert_eq!(page.len(), 1);
            assert_eq!(last.resolution.is_some(), filter == ParkedFilter::Resolved);
            after = last.id;
            seen.push(last.id);
        }
        assert_eq!(seen, expected, "{filter:?}");
    }
    assert_eq!(store.parked_counts()?, counts);
    Ok(())
}

// Owner review 2026-11-20 (PR 275): Needs review lists the items resolved
// today under the open ones: only those resolved inside the day asked for,
// newest first and bounded; open items and resolutions that cannot be read
// are left out, and reading changes nothing.
#[test]
fn resolved_items_of_one_day_come_newest_first_and_bounded() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let mut ids = Vec::new();
    for reason in ["a", "b", "c", "d", "e", "f"] {
        ids.push(store.park(&Parked {
            kind: Identifier::new("synthetic")?,
            reason: Reason::new(reason)?,
            targets: vec![],
        })?);
    }
    // Resolved at 99 (the day before), 100, 150, 150, 199, and 200 (the next day).
    for (id, time) in ids.iter().zip([99, 150, 100, 150, 199, 200]) {
        store.resolve(
            *id,
            &Audit {
                actor: Identifier::new("operator")?,
                action: Identifier::new("parked-resolve")?,
                targets: vec![Identifier::new(&id.to_string())?],
                time,
                reason: Reason::new("settled")?,
            },
        )?;
    }
    let open = store.park(&Parked {
        kind: Identifier::new("synthetic")?,
        reason: Reason::new("open")?,
        targets: vec![],
    })?;
    let unreadable = store.park(&Parked {
        kind: Identifier::new("synthetic")?,
        reason: Reason::new("unreadable")?,
        targets: vec![],
    })?;
    store.db.execute(
        "UPDATE parked SET resolution='resolved by hand' WHERE id=?1",
        [unreadable],
    )?;
    let counts = store.parked_counts()?;
    let day = |store: &Store, limit| -> Result<Vec<i64>, Box<dyn std::error::Error>> {
        Ok(store
            .parked_resolved_between(100, 200, limit)?
            .iter()
            .map(|item| item.id)
            .collect())
    };
    // Newest first; the same time comes in the newer item's order first.
    let all = day(&store, 10)?;
    assert_eq!(all, [ids[4], ids[3], ids[1], ids[2]]);
    assert!(!all.contains(&open) && !all.contains(&unreadable));
    assert_eq!(day(&store, 2)?, [ids[4], ids[3]]);
    store.limits.page_rows = 1;
    assert_eq!(day(&store, 10)?, [ids[4]]);
    assert!(store.parked_resolved_between(300, 400, 10)?.is_empty());
    assert_eq!(store.parked_counts()?, counts);
    Ok(())
}

// Owner request 2026-11-19 (admin restyle b1): an item's history reads the
// audit entries that name it with one action, oldest first; other actions,
// other items and entries that only mention the name in a reason are left out,
// and only the newest page is kept with a flag for older ones.
#[test]
fn audits_naming_a_target_keep_the_newest_page_oldest_first() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let target = Identifier::new("parked:7")?;
    let action = Identifier::new("receipt-reprocess")?;
    let entry = |action: &str,
                 target: &str,
                 time: i64,
                 reason: &str|
     -> Result<Audit, Box<dyn std::error::Error>> {
        Ok(Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new(action)?,
            targets: vec![Identifier::new("receipt")?, Identifier::new(target)?],
            time,
            reason: Reason::new(reason)?,
        })
    };
    let (none, older) = store.audits_naming(&target, &action)?;
    assert!(none.is_empty() && !older);
    for audit in [
        entry("receipt-reprocess", "parked:7", 1, "first")?,
        entry("admin-raw-view", "parked:7", 2, "a view")?,
        entry("receipt-reprocess", "parked:70", 3, "another item")?,
        entry("receipt-reprocess", "parked:8", 4, "mentions \"parked:7\"")?,
        entry("receipt-reprocess", "parked:7", 5, "second")?,
        entry("receipt-reprocess", "parked:7", 6, "third")?,
    ] {
        store.audit(&audit)?;
    }
    let reasons = |rows: &[Audit]| {
        rows.iter()
            .map(|a| a.reason.as_str().to_owned())
            .collect::<Vec<_>>()
    };
    let (rows, older) = store.audits_naming(&target, &action)?;
    assert_eq!(reasons(&rows), ["first", "second", "third"]);
    assert!(!older);
    store.limits.page_rows = 2;
    let (rows, older) = store.audits_naming(&target, &action)?;
    assert_eq!(reasons(&rows), ["second", "third"]);
    assert!(older);
    Ok(())
}

// Owner request 2026-11-20 (admin restyle b2, machine page): a machine's page
// lists its newest stored messages first, bounded, each with what was stored:
// the latest reading (results with their sample IDs as stored, a failed
// reading, or none yet), its deliveries by state over every reading and the
// first open Needs review item naming it. Other machines' messages, resolved
// items and every value stay out, and reading changes nothing.
#[test]
fn recent_receipts_of_one_machine_come_newest_first_with_what_was_stored() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let machine = Identifier::new("synthetic-machine")?;
    assert!(store.recent_receipts(&machine, 20)?.is_empty());
    let Recent {
        unread,
        read,
        failed,
        open_failed,
        open_read,
    } = seed_recent(&mut store)?;
    let parked_before = store.parked(0)?.len();

    let recent = store.recent_receipts(&machine, 20)?;
    assert_eq!(
        recent,
        [
            RecentReceipt {
                metadata: failed.clone(),
                reading: Some(RecentReading {
                    version: 1,
                    outcome: RecentOutcome::Failed,
                }),
                deliveries: vec![],
                review: Some(open_failed),
            },
            RecentReceipt {
                metadata: read.clone(),
                reading: Some(RecentReading {
                    version: 2,
                    outcome: RecentOutcome::Results {
                        results: 3,
                        samples: vec![SampleId::new("000123")?, SampleId::new("0042")?],
                    },
                }),
                deliveries: vec![
                    (DeliveryState::Parked, 1),
                    (DeliveryState::Done, 2),
                    (DeliveryState::Pending, 1),
                ],
                review: Some(open_read),
            },
            RecentReceipt {
                metadata: unread,
                reading: None,
                deliveries: vec![],
                review: None,
            },
        ]
    );
    // Bounded by the caller and by one page.
    let newest: Vec<ReceiptId> = store
        .recent_receipts(&machine, 2)?
        .iter()
        .map(|recent| recent.metadata.id)
        .collect();
    assert_eq!(newest, [failed.id, read.id]);
    let mut paged = open(&temp.path().join("paged.db"))?;
    paged.limits.page_rows = 1;
    for raw in [b"one".as_slice(), b"two"] {
        paged.receive(&receipt(raw)?, raw, b"config")?;
    }
    assert_eq!(paged.recent_receipts(&machine, 20)?.len(), 1);
    assert!(
        store
            .recent_receipts(&Identifier::new("never-used")?, 20)?
            .is_empty()
    );
    // Reading changed nothing.
    assert_eq!(store.parked(0)?.len(), parked_before);
    assert_eq!(store.recent_receipts(&machine, 20)?, recent);
    Ok(())
}

/// The messages [`seed_recent`] stored for `synthetic-machine` and the open items naming them.
struct Recent {
    unread: ReceiptMetadata,
    read: ReceiptMetadata,
    failed: ReceiptMetadata,
    open_failed: i64,
    open_read: i64,
}

/// Stores, oldest first, an unread message, one read twice (a failed reading,
/// then three results from two samples) with deliveries from both readings, and
/// one that could not be read; another machine's message; and open and resolved
/// Needs review items naming them.
fn seed_recent(store: &mut Store) -> Result<Recent, Box<dyn std::error::Error>> {
    let unread = receipt(b"unread")?;
    let read = receipt(b"read")?;
    let failed = receipt(b"failed")?;
    let mut other = receipt(b"other")?;
    other.machine = Identifier::new("other-machine")?;
    for (metadata, raw) in [
        (&unread, "unread"),
        (&read, "read"),
        (&failed, "failed"),
        (&other, "other"),
    ] {
        store.receive(metadata, raw.as_bytes(), b"config")?;
    }
    let reading = |outcome: Reading| -> Result<String, Box<dyn std::error::Error>> {
        let mut evidence = interpretation()?;
        evidence.outcome = outcome;
        Ok(document(&evidence)?)
    };
    let mut first = production_result(read.id)?;
    let mut second = production_result(read.id)?;
    let mut third = production_result(read.id)?;
    for (result, sample) in [
        (&mut first, "000123"),
        (&mut second, "0042"),
        (&mut third, "000123"),
    ] {
        if let Some(identity) = &mut result.sample {
            identity.id = SampleId::new(sample)?;
        }
    }
    third.sample = None;
    for (id, version, outcome) in [
        (
            read.id,
            1,
            Reading::Failed(Reason::new("first reading failed")?),
        ),
        (read.id, 2, Reading::Results(vec![first, second, third])),
        (failed.id, 1, Reading::Failed(Reason::new("not a message")?)),
        (other.id, 1, Reading::Results(vec![])),
    ] {
        store.db.execute(
            "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,?2,?3)",
            params![key(&id)?, version, reading(outcome)?],
        )?;
    }
    for (id, version, state) in [
        (read.id, 1, "parked"),
        (read.id, 2, "done"),
        (read.id, 2, "done"),
        (read.id, 2, "pending"),
        (other.id, 1, "pending"),
    ] {
        store.db.execute(
            "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,'result',?2,?3,'app','k',x'00',x'00',1,?4,0)",
            params![key(&DeliveryId::try_from(uuid::Uuid::now_v7())?)?, key(&id)?, version, state],
        )?;
    }
    let target = |id: ReceiptId| Identifier::new(&uuid::Uuid::from(id).to_string());
    let park =
        |store: &mut Store, targets: Vec<Identifier>| -> Result<i64, Box<dyn std::error::Error>> {
            Ok(store.park(&Parked {
                kind: Identifier::new("synthetic")?,
                reason: Reason::new("recent messages")?,
                targets,
            })?)
        };
    let settled = park(store, vec![target(failed.id)?])?;
    store.resolve(
        settled,
        &Audit {
            actor: Identifier::new("operator")?,
            action: Identifier::new("resolve")?,
            targets: vec![],
            time: 1,
            reason: Reason::new("reviewed")?,
        },
    )?;
    let open_failed = park(
        store,
        vec![Identifier::new("elsewhere")?, target(failed.id)?],
    )?;
    park(store, vec![target(failed.id)?])?;
    let open_read = park(store, vec![target(read.id)?])?;
    park(store, vec![target(other.id)?])?;
    Ok(Recent {
        unread,
        read,
        failed,
        open_failed,
        open_read,
    })
}
