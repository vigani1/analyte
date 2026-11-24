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
    queue_order_event(&mut store, &event, 2)?;
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
    queue_order_event(store, &event, now)?;
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

/// When each lab app last accepted a delivery, as schema 16 read it: every
/// accepted attempt joined to its delivery. Kept as the reference.
fn last_delivered_scanned(
    store: &Store,
) -> Result<Vec<(Identifier, i64)>, Box<dyn std::error::Error>> {
    let rows: Vec<(String, i64)> = store
        .db
        .prepare("SELECT o.destination,max(a.time) FROM attempts a JOIN outbox o ON o.id=a.delivery WHERE a.outcome=?1 GROUP BY o.destination ORDER BY o.destination")?
        .query_map([document(&Attempt::Done)?], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|(destination, at)| Ok((Identifier::new(&destination)?, at)))
        .collect()
}

/// The failing lab apps as schema 16 read them: every attempt joined to its
/// delivery. Kept as the reference.
fn failing_scanned(store: &Store) -> Result<Vec<FailingDestination>, Box<dyn std::error::Error>> {
    let rows: Vec<(String, i64, i64, u64, Option<i64>)> = store
        .db
        .prepare(concat!(
            "WITH done AS (SELECT o.destination AS destination,max(a.sequence) AS last ",
            "FROM attempts a JOIN outbox o ON o.id=a.delivery WHERE a.outcome=?1 ",
            "GROUP BY o.destination) ",
            "SELECT o.destination,min(a.time),max(a.time),count(*),",
            "(SELECT min(p.next_attempt) FROM outbox p WHERE p.destination=o.destination AND p.state=?2) ",
            "FROM attempts a JOIN outbox o ON o.id=a.delivery ",
            "LEFT JOIN done d ON d.destination=o.destination ",
            "WHERE json_type(a.outcome,'$.RetryAt')='integer' AND a.sequence>coalesce(d.last,0) ",
            "GROUP BY o.destination ",
            "HAVING EXISTS(SELECT 1 FROM outbox w WHERE w.destination=o.destination AND w.state IN (?2,?3)) ",
            "ORDER BY o.destination",
        ))?
        .query_map(
            rusqlite::params![
                document(&Attempt::Done)?,
                DeliveryState::Pending,
                DeliveryState::InFlight
            ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?.try_into().unwrap_or(u64::MAX), r.get(4)?)),
        )?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|(destination, since, last, tries, next)| {
            Ok(FailingDestination {
                destination: Identifier::new(&destination)?,
                since,
                last,
                tries,
                next,
            })
        })
        .collect()
}

// Owner decision 2026-11-20 (admin read speed, item 4): "Last delivered" and
// the failing lab apps are read through the accepted-delivery helpers and
// indexes, and say exactly what the scan over every attempt said: with a
// delivery accepted again under an earlier clock, a lab app that never
// accepted anything, a try and a success in the same second, an order
// message, a purged message, and again after the delivery worker writes.
#[test]
fn delivery_reads_match_the_scan_over_every_attempt() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = mixed_store(&temp.path().join("mixed.db"))?;
    let main = Identifier::new("main-lab")?;
    let outpatient = Identifier::new("outpatient")?;
    let silent = Identifier::new("silent-lab")?;
    let delivered = store.last_delivered()?;
    assert_eq!(delivered.at, last_delivered_scanned(&store)?);
    assert_eq!(
        delivered.at,
        [(main.clone(), 10_140), (outpatient.clone(), 10_305)],
        "the latest accepted time, never the latest accepted row"
    );
    assert!(delivered.purged);
    let failing = store.failing_destinations()?;
    assert_eq!(failing, failing_scanned(&store)?);
    assert_eq!(
        failing
            .iter()
            .map(|failing| (failing.destination.clone(), failing.since, failing.tries))
            .collect::<Vec<_>>(),
        [
            (main.clone(), 10_130, 1),
            (outpatient.clone(), 10_320, 1),
            (silent.clone(), 10_110, 2)
        ]
    );
    // The delivery worker accepts main-lab's waiting item and fails again on
    // the silent lab app's.
    for (destination, outcome, now) in [
        (&main, Attempt::Done, 20_010),
        (&silent, Attempt::RetryAt(20_100), 20_020),
    ] {
        let claim = store
            .claim_destination(now, destination)?
            .ok_or("a waiting item")?;
        store.attempted(claim.id, &outcome, now)?;
        assert_eq!(store.last_delivered()?.at, last_delivered_scanned(&store)?);
        assert_eq!(store.failing_destinations()?, failing_scanned(&store)?);
    }
    assert_eq!(
        store
            .failing_destinations()?
            .iter()
            .map(|failing| failing.destination.clone())
            .collect::<Vec<_>>(),
        [outpatient, silent]
    );
    Ok(())
}

// Owner decision 2026-11-20 (admin read speed, item 3): the newest message
// per machine is read through the index by machine and received time, and is
// the same as the latest of every stored message: fractions of a second
// decide within a second, a purged message no longer counts, and a machine
// with no message is left out. Data keeping's oldest kept message is
// unchanged.
#[test]
fn newest_message_per_machine_matches_every_stored_message() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = mixed_store(&temp.path().join("mixed.db"))?;
    let machines = ["m-a", "m-b", "m-quiet", "no-such-machine"]
        .map(Identifier::new)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut walked = std::collections::BTreeMap::<String, jiff::Timestamp>::new();
    let mut after = String::new();
    loop {
        let page = store.receipts(&after)?;
        let Some(last) = page.last() else { break };
        after = serde_json::to_string(&last.id)?;
        for receipt in page {
            let newest = walked
                .entry(receipt.machine.as_str().to_owned())
                .or_insert(receipt.received_at);
            *newest = (*newest).max(receipt.received_at);
        }
    }
    let newest = store.newest_receipts(&machines)?;
    assert_eq!(
        newest,
        machines
            .iter()
            .filter_map(|machine| walked
                .get(machine.as_str())
                .map(|at| (machine.clone(), *at)))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        newest,
        [
            (
                Identifier::new("m-a")?,
                jiff::Timestamp::new(10_050, 700_000_000)?
            ),
            (
                Identifier::new("m-b")?,
                jiff::Timestamp::from_second(10_300)?
            ),
            (
                Identifier::new("m-quiet")?,
                jiff::Timestamp::from_second(9_000)?
            ),
        ]
    );
    assert_eq!(store.oldest_retained_receipt()?, Some(9_000));
    Ok(())
}

/// The count of unresolved items parked about machine `?1` before it found
/// machine orders by their key: every order was compared with every target.
const PARKED_FOR_MACHINE_SCANNED: &str = concat!(
    "SELECT count(*) FROM parked p WHERE p.resolution IS NULL AND EXISTS(",
    "SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=?1 ",
    "OR EXISTS(SELECT 1 FROM receipts r WHERE r.id=json_quote(t.value) ",
    "AND json_extract(r.metadata,'$.machine')=?1) ",
    "OR EXISTS(SELECT 1 FROM outbox o JOIN receipts r ON r.id=o.receipt ",
    "WHERE o.id=json_quote(t.value) AND json_extract(r.metadata,'$.machine')=?1) ",
    "OR EXISTS(SELECT 1 FROM transfers x JOIN receipts r ON r.id=x.receipt ",
    "WHERE x.id=t.value AND json_extract(r.metadata,'$.machine')=?1) ",
    "OR EXISTS(SELECT 1 FROM machine_orders m WHERE CAST(m.id AS TEXT)=t.value AND m.machine=?1))"
);

// Owner decision 2026-11-20 (admin reads that stop, item 3); AGENTS.md safety
// rule 2: what is unfinished for a machine finds its parked items' orders by
// key and counts exactly what comparing every order with every target
// counted. Targets that only look like an order number (a leading zero, a
// fraction, a number or `true` in place of text, spaces) match the same
// orders, or none, as before; a resolved item is never counted.
#[test]
fn unfinished_counts_what_the_scan_over_every_order_counted() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = mixed_store(&temp.path().join("mixed.db"))?;
    for targets in [
        r#"["1"]"#,
        r#"["2"]"#,
        r#"["3"]"#,
        "[3]",
        "[2.0]",
        r#"["02"]"#,
        r#"["3.0"]"#,
        r#"[" 3"]"#,
        r#"["1e0"]"#,
        "[true]",
        "[null]",
        r#"["99"]"#,
        r#"["99999999999999999999"]"#,
        r#"["m-b","3"]"#,
        r#"["no-such-machine","2"]"#,
    ] {
        store.db.execute(
            "INSERT INTO parked(evidence,parked_at) VALUES (?1,10000)",
            [format!(
                r#"{{"kind":"mapping_finding","reason":"synthetic","targets":{targets},"context":null}}"#
            )],
        )?;
    }
    store.db.execute(
        "INSERT INTO parked(evidence,parked_at,resolution) VALUES (?1,10000,?2)",
        [
            r#"{"kind":"mapping_finding","reason":"synthetic","targets":["3"],"context":null}"#,
            r#"{"actor":"admin-session-1","action":"parked-resolve","targets":["9"],"time":10500,"reason":"synthetic"}"#,
        ],
    )?;
    let mut counted = Vec::new();
    for machine in ["m-a", "m-b", "m-quiet", "no-such-machine"] {
        let flagged = store.unfinished(&Identifier::new(machine)?)?.flagged;
        let scanned: i64 = store
            .db
            .query_row(PARKED_FOR_MACHINE_SCANNED, [machine], |r| r.get(0))?;
        assert_eq!(i64::try_from(flagged)?, scanned, "{machine}");
        counted.push(flagged);
    }
    // m-a: the fixture's item about order 1, then "1", "2", `true` (JSON reads
    // it as 1) and the item naming another machine with order 2. m-b: "3", 3
    // and its name with "3". The unknown machine: its name.
    assert_eq!(counted, [5, 3, 0, 1]);
    Ok(())
}
