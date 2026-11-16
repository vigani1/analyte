//! Tests adr0003 v6 every state owner and parking shape migrates without losing meaning; adr0003 v8 custody migration keeps every parked row and reconciles typed machine intents.
use super::*;

#[test]
fn adr0003_v6_every_state_owner_and_parking_shape_migrates_without_losing_meaning() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("v6.db");
    let db = legacy(&path)?;
    let (expected_orders, owner_count) = seed_adr0003_v6_order_states(&db)?;
    let metadata = receipt(b"original bytes")?;
    db.execute(
        "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
        params![
            key(&metadata.id)?,
            key(&metadata)?,
            b"original bytes",
            b"config"
        ],
    )?;
    db.execute(
        "INSERT INTO interpretations VALUES (?1,1,'original interpretation')",
        [key(&metadata.id)?],
    )?;
    let expected_deliveries = seed_adr0003_v6_deliveries(&db, metadata.id)?;
    let expected_parking = seed_adr0003_v6_parking(&db, metadata.id, &expected_deliveries)?;
    let before = snapshot(&db)?;
    drop(db);
    let mut store = open(&path)?;
    let after = snapshot(&store.db)?;
    verify_adr0003_v6_retained_columns(before, &after)?;
    verify_adr0003_v6_order_states(&mut store, expected_orders)?;
    for (id, state, kind) in expected_deliveries {
        assert_eq!(store.delivery(id)?.ok_or("missing delivery")?.state, state);
        assert_eq!(
            store
                .db
                .query_row("SELECT kind FROM outbox WHERE id=?1", [key(&id)?], |r| {
                    r.get::<_, DeliveryKind>(0)
                })?,
            kind
        );
    }
    for (id, original, resolution, kind, target, reason, context) in expected_parking {
        let row = store.parked_item(id)?.ok_or("missing parked row")?;
        assert_eq!(row.resolution.as_deref(), resolution);
        let details = row.details()?;
        assert_eq!(details.kind.as_str(), kind);
        assert_eq!(details.reason.as_str(), reason);
        assert_eq!(details.targets, vec![Identifier::new(&target)?]);
        let canonical: serde_json::Value = serde_json::from_str(&row.evidence)?;
        assert!(canonical.is_object());
        assert_eq!(canonical.get("context"), Some(&context));
        assert_eq!(
            store.db.query_row(
                "SELECT original_evidence FROM parked WHERE id=?1",
                [id],
                |r| r.get::<_, String>(0)
            )?,
            original
        );
    }
    assert_eq!(
        store.db.query_row(
            "SELECT value FROM alert_counts WHERE kind='uncertain'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        owner_count
    );
    assert_eq!(
        store.db.query_row(
            "SELECT value FROM alert_counts WHERE kind='parked'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        8
    );
    store.db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app','new-order','machine','1',?1,102,102,'new')",[SqlOrderState(OrderState::Sending)])?;
    assert_eq!(store.db.last_insert_rowid(), 1001);
    assert!(
        store
            .db
            .execute("UPDATE machine_order_history SET owner_id='changed'", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("UPDATE parked SET original_evidence='changed'", [])
            .is_err()
    );
    assert!(store.db.execute("DELETE FROM parked", []).is_err());
    drop(store);
    assert_eq!(
        open(&path)?
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        SCHEMA_VERSION
    );
    Ok(())
}

#[test]
fn adr0003_v8_custody_migration_keeps_every_parked_row_and_reconciles_typed_machine_intents()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("custody-v8.db");
    let mut db = version8(&path)?;
    let tx = db.transaction()?;
    for _ in 0..130 {
        tx.execute("INSERT INTO parked(evidence) VALUES (?1)",[r#"{"kind":"other","reason":"unrelated retained evidence","targets":[],"context":null}"#])?;
    }
    let fixtures = [
        (vec!["autolyser"], "unconfirmed original R", None),
        (
            vec!["autolyser"],
            "original resolved guard",
            Some("exact legacy resolution evidence"),
        ),
        (
            vec!["autolyser", "other", "autolyser"],
            "second unresolved poll",
            None,
        ),
        (
            vec!["resolved-only"],
            "confirmed legacy cursor",
            Some("second exact resolution"),
        ),
    ];
    let mut expected = Vec::new();
    let mut guards = Vec::new();
    for (targets, reason, resolution) in fixtures {
        let evidence = serde_json::json!({"kind":"autolyser_poll_custody","reason":reason,"targets":targets,"context":null}).to_string();
        let original = format!("  {evidence}  ");
        tx.execute(
            "INSERT INTO parked(evidence,resolution,original_evidence) VALUES (?1,?2,?3)",
            params![evidence, resolution, original],
        )?;
        let parked = tx.last_insert_rowid();
        guards.push(parked);
        let state = if resolution.is_some() {
            PollIntentState::Confirmed
        } else {
            PollIntentState::Uncertain
        };
        let mut seen = std::collections::HashSet::new();
        for target in targets {
            if seen.insert(target) {
                expected.push((
                    target.to_owned(),
                    parked,
                    state,
                    reason.to_owned(),
                    resolution.map(str::to_owned),
                ));
            }
        }
    }
    tx.execute("UPDATE sqlite_sequence SET seq=900 WHERE name='parked'", [])?;
    tx.commit()?;
    let before = snapshot(&db)?;
    drop(db);
    let mut limits = limits();
    limits.page_rows = 1; // The first custody row lies beyond the old 128-page scan cap.
    let mut store = Store::open(&path, limits, Box::new(Space(u64::MAX)))?;
    let mut after = snapshot(&store.db)?;
    assert_eq!(
        after
            .remove("poll_intents")
            .ok_or("missing typed intents")?
            .len(),
        5
    );
    assert_eq!(
        after
            .remove("poll_intent_history")
            .ok_or("missing typed history")?
            .len(),
        5
    );
    assert_eq!(after, before);
    let actual = store.db.prepare("SELECT p.machine,p.parked,p.state,p.reason,h.audit FROM poll_intents p JOIN poll_intent_history h ON h.intent=p.id ORDER BY p.id")?
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,PollIntentState>(2)?,row.get::<_,String>(3)?,row.get::<_,Option<String>>(4)?)))?
        .collect::<Result<Vec<_>,_>>()?;
    assert_eq!(actual, expected);
    verify_adr0003_migrated_custody_reconciliation(&mut store, &guards)
}

#[test]
fn adr0003_v9_missing_custody_machine_refuses_migration_without_guessing_or_dropping_rows()
-> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("unknown-custody.db");
    let db = version8(&path)?;
    db.execute("INSERT INTO parked(evidence) VALUES (?1)",[r#"{"kind":"autolyser_poll_custody","reason":"machine identity missing","targets":[],"context":null}"#])?;
    let before = snapshot(&db)?;
    drop(db);
    assert!(matches!(
        open(&path),
        Err(Error::MigrationEvidence {
            table: "parked",
            row: 1,
            ..
        })
    ));
    let db = Connection::open(&path)?;
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
        8
    );
    assert_eq!(snapshot(&db)?, before);
    Ok(())
}

#[test]
fn mutation_adr0003_migration_canonical_parking_limit_is_inclusive_and_refusal_rolls_back()
-> TestResult {
    for delta in [0, 1] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("canonical-bound.db");
        let mut db = legacy(&path)?;
        let original = r#"["incomplete_transfer","synthetic-transfer","exact retained reason"]"#;
        db.execute("INSERT INTO parked(evidence) VALUES (?1)", [original])?;
        let expected = serde_json::json!({"kind":"incomplete_transfer","reason":"exact retained reason","targets":["synthetic-transfer"],"context":null});
        let limit = expected
            .to_string()
            .len()
            .checked_sub(delta)
            .ok_or("limit")?;
        assert!(original.len() < limit);
        let before = snapshot(&db)?;
        let result = Store::migrate_v7(&mut db, limit);
        if delta == 0 {
            result?;
            let (canonical, retained): (String, String) =
                db.query_row("SELECT evidence,original_evidence FROM parked", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&canonical)?,
                expected
            );
            assert_eq!(retained, original);
        } else {
            assert!(
                matches!(result,Err(Error::MigrationEvidence{table:"parked",row:1,cause}) if matches!(*cause,Error::InvalidInput))
            );
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
                6
            );
            assert_eq!(snapshot(&db)?, before);
        }
    }
    Ok(())
}
