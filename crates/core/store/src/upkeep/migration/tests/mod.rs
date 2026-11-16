//! Tests schema migration and retained evidence.
use crate::test_support::*;
use crate::*;
use analyte_model::Reason;

fn verify_plan11_v1_evidence(
    migrated: &mut Store,
    receipt_id: ReceiptId,
    id: DeliveryId,
    tables: &[&str],
    rows: Vec<i64>,
    triggers: Vec<String>,
) -> TestResult {
    for (table, count) in tables.iter().zip(rows) {
        assert_eq!(
            migrated
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
            count
        );
    }
    assert_eq!(migrated.raw(receipt_id)?, Some(b"raw".to_vec()));
    assert_eq!(migrated.frames(&Identifier::new("transfer")?, 0)?.len(), 1);
    assert!(migrated.interpretation(receipt_id, 1)?.is_some());
    let claim = migrated.claim(10)?.ok_or(Error::Corruption)?;
    assert_eq!(claim.id, id);
    assert_eq!(claim.payload, vec![0, 17, 34]);
    assert_eq!(claim.earlier_attempts, 1);
    let backfilled: bool =
        migrated
            .db
            .query_row("SELECT enqueue_time_backfilled FROM outbox", [], |r| {
                r.get(0)
            })?;
    assert!(backfilled);
    for trigger in triggers {
        assert!(
            migrated.db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name=?1",
                [trigger],
                |r| r.get::<_, i64>(0)
            )? == 1
        );
    }
    for table in [
        "receipts",
        "frames",
        "interpretations",
        "attempts",
        "audit",
        "outbox",
    ] {
        assert!(
            migrated
                .db
                .execute(&format!("DELETE FROM {table}"), [])
                .is_err()
        );
    }
    assert!(
        migrated
            .db
            .execute("UPDATE outbox SET payload=x'00'", [])
            .is_err()
    );
    Ok(())
}

use crate::sql_types::SqlOrderState;
use crate::test_support::{TestResult, open, receipt};
use analyte_model::{Identifier, OrderState};
use rusqlite::{Connection, params, types::Value};
use std::{collections::BTreeMap, num::NonZeroU64, path::Path};

fn legacy(path: &Path) -> Result<Connection, Error> {
    let mut db = Connection::open(path)?;
    db.execute_batch(include_str!("../../schema.sql"))?;
    Store::migrate_v2(&mut db)?;
    Store::migrate_v3(&mut db, 65_536)?;
    db.execute_batch(include_str!("../../migration_v4.sql"))?;
    db.execute_batch(include_str!("../../migration_v5.sql"))?;
    db.execute_batch(include_str!("../../migration_v6.sql"))?;
    Ok(db)
}

type Snapshot = BTreeMap<String, Vec<Vec<Value>>>;

fn snapshot(db: &Connection) -> Result<Snapshot, Error> {
    let names = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    names
        .into_iter()
        .map(|name| {
            let mut statement = db.prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                name.replace('"', "\"\"")
            ))?;
            let rows = statement
                .query_map([], |row| {
                    (0..row.as_ref().column_count())
                        .map(|column| row.get(column))
                        .collect::<Result<Vec<Value>, _>>()
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name, rows))
        })
        .collect()
}

fn version8(path: &Path) -> Result<Connection, Error> {
    let mut db = legacy(path)?;
    Store::migrate_v7(&mut db, 65_536)?;
    db.execute_batch(include_str!("../../migration_v8.sql"))?;
    Ok(db)
}

fn verify_adr0003_migrated_custody_reconciliation(store: &mut Store, guards: &[i64]) -> TestResult {
    let machine = Identifier::new("autolyser")?;
    let intent = store
        .unconfirmed_poll_intent(&machine)?
        .ok_or("missing migrated blocking guard")?;
    assert_eq!(Some(&intent.parked), guards.first());
    assert_eq!(intent.state, PollIntentState::Uncertain);
    let reason = Reason::new("explicit operator cursor reconciliation")?;
    assert!(matches!(
        store.begin_poll_intent(&machine, &reason),
        Err(Error::PollIntentUnresolved)
    ));
    assert!(
        store
            .unconfirmed_poll_intent(&Identifier::new("resolved-only")?)?
            .is_none()
    );
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("parked-resolve")?,
        targets: vec![Identifier::new(&intent.parked.to_string())?],
        time: 100,
        reason: reason.clone(),
    };
    store.resolve(intent.parked, &audit)?;
    assert!(store.unconfirmed_poll_intent(&machine)?.is_some());
    let other = Identifier::new("other")?;
    let shared = store
        .unconfirmed_poll_intent(&other)?
        .ok_or("missing shared machine intent")?;
    let individual = Audit {
        actor: Identifier::new("host")?,
        action: Identifier::new("poll-confirmation")?,
        targets: vec![Identifier::new(&format!("poll:{}", shared.id))?],
        time: 101,
        reason: reason.clone(),
    };
    store.record_poll_outcome(shared.id, PollOutcome::Confirmed, &individual)?;
    assert!(store.unconfirmed_poll_intent(&other)?.is_none());
    assert!(store.unconfirmed_poll_intent(&machine)?.is_some());
    assert!(
        store
            .parked_item(shared.parked)?
            .ok_or("lost shared evidence")?
            .resolution
            .is_none()
    );
    store.resolve(*guards.get(2).ok_or("missing shared guard")?, &audit)?;
    assert!(store.unconfirmed_poll_intent(&machine)?.is_none());
    assert!(
        store
            .unconfirmed_poll_intent(&Identifier::new("other")?)?
            .is_none()
    );
    store.begin_poll_intent(&machine, &reason)?;
    assert_eq!(
        store
            .unconfirmed_poll_intent(&machine)?
            .ok_or("missing new poll")?
            .parked,
        901
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM parked WHERE id<901", [], |row| row
                .get::<_, i64>(0))?,
        134
    );
    assert!(store.db.execute("DELETE FROM poll_intents", []).is_err());
    Ok(())
}

fn verify_adr0003_v6_retained_columns(before: Snapshot, after: &Snapshot) -> TestResult {
    let mut retained = after.clone();
    assert_empty_retention_tables(&mut retained);
    let after = &retained;
    assert_eq!(
        after
            .keys()
            .filter(|name| !before.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["poll_intent_history".to_owned(), "poll_intents".to_owned()]
    );
    for (table, rows) in before {
        let actual = after.get(&table).ok_or("missing retained table")?;
        assert_eq!(rows.len(), actual.len(), "{table} row count");
        for (mut old, new) in rows.into_iter().zip(actual) {
            // Only active lifecycle encodings and canonical parked evidence change.
            let state_column = match table.as_str() {
                "machine_orders" => Some(5),
                "machine_order_items" => Some(6),
                _ => None,
            };
            if let Some(column) = state_column {
                let Value::Text(encoded) = old.get(column).ok_or("missing old state")? else {
                    return Err("state was not text".into());
                };
                let decoded: String = serde_json::from_str(encoded)?;
                *old.get_mut(column).ok_or("missing state")? = Value::Text(decoded);
            }
            if table == "parked" {
                assert_eq!(new.get(3), old.get(1), "original parked JSON is exact");
                *old.get_mut(1).ok_or("missing evidence")? =
                    new.get(1).ok_or("missing canonical evidence")?.clone();
            }
            assert_eq!(
                Some(old.as_slice()),
                new.get(..old.len()),
                "{table} original columns"
            );
        }
    }
    Ok(())
}

type LegacyOrder = (
    i64,
    String,
    OrderState,
    &'static str,
    Option<&'static str>,
    Option<(&'static str, &'static str)>,
);

fn seed_adr0003_v6_order_states(
    db: &Connection,
) -> Result<(Vec<LegacyOrder>, i64), Box<dyn std::error::Error>> {
    let states = [
        OrderState::Sending,
        OrderState::Sent,
        OrderState::Accepted,
        OrderState::Done,
        OrderState::Rejected,
        OrderState::CancelRequested,
        OrderState::CancelConfirmed,
        OrderState::Uncertain,
    ];
    // Include namespace colons and the old cancellation suffix convention. An empty
    // suffix never matched any valid owner, but its version still prevented replay.
    let owners = [
        ("peer-evidence", None, None),
        ("host-reserved", None, None),
        ("host-reserved:connection", Some("connection"), None),
        (
            "host-reserved:connection:part",
            Some("connection:part"),
            None,
        ),
        ("app-cancel:1:connection", None, Some(("connection", "1"))),
        (
            "app-cancel:18446744073709551615:connection:part",
            None,
            Some(("part", "18446744073709551615")),
        ),
        ("app-cancel:7:connection:", None, Some(("", "7"))),
    ];
    let mut expected_orders = Vec::new();
    for state in states {
        for (code, reservation_owner, cancellation) in owners {
            let order = format!("order-{}", expected_orders.len());
            db.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('app',?1,'machine','1',?2,100,101,?3)", params![order, key(&state)?, code])?;
            let id = db.last_insert_rowid();
            let history = serde_json::json!({"id":id,"key":{"destination":"app","order":order,"machine":"machine"},"version":1,"state":state,"created_at":100,"updated_at":101,"source_code":code}).to_string();
            db.execute(
                "INSERT INTO machine_order_history(machine_order,evidence) VALUES (?1,?2)",
                params![id, history],
            )?;
            db.execute(
                "INSERT INTO machine_order_items VALUES (?1,1,'[]',0,?2,NULL,?3)",
                params![id, format!("{id}-1"), key(&state)?],
            )?;
            let item = serde_json::json!({"machine_order":id,"index":1,"tests":[],"coverage_known":false,"placer":format!("{id}-1"),"state":state,"updated_at":101,"source_code":code}).to_string();
            db.execute("INSERT INTO machine_order_item_history(machine_order,item_index,evidence) VALUES (?1,1,?2)",params![id,item])?;
            expected_orders.push((id, order, state, code, reservation_owner, cancellation));
        }
    }
    db.execute(
        "UPDATE sqlite_sequence SET seq=1000 WHERE name='machine_orders'",
        [],
    )?;

    Ok((expected_orders, i64::try_from(owners.len())?))
}

fn verify_adr0003_v6_order_states(
    store: &mut Store,
    expected_orders: Vec<LegacyOrder>,
) -> TestResult {
    for (id, order, state, code, reservation_owner, cancellation) in expected_orders {
        let key = MachineOrderKey {
            destination: Identifier::new("app")?,
            order: Identifier::new(&order)?,
            machine: Identifier::new("machine")?,
        };
        let retained = store.machine_order(&key)?.ok_or("missing reservation")?;
        assert_eq!(retained.state, state);
        assert_eq!(retained.source_code.as_str(), code);
        assert_eq!(retained.items.first().ok_or("missing item")?.state, state);
        let sql_state: SqlOrderState =
            store
                .db
                .query_row("SELECT state FROM machine_orders WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        assert_eq!(sql_state.0, state);
        let item_state: SqlOrderState = store.db.query_row(
            "SELECT state FROM machine_order_items WHERE machine_order=?1",
            [id],
            |r| r.get(0),
        )?;
        assert_eq!(item_state.0, state);
        for owner in ["connection", "connection:part", "part", "other"] {
            let identifier = Identifier::new(owner)?;
            assert_eq!(
                retained.reserved_by(&identifier),
                state == OrderState::Sending && reservation_owner == Some(owner)
            );
            assert_eq!(
                retained.cancel_requested_by(&identifier),
                state == OrderState::CancelRequested
                    && cancellation.is_some_and(|(connection, _)| connection == owner)
            );
        }
        let version: Option<String> = store.db.query_row(
            "SELECT cancel_version FROM machine_order_history WHERE machine_order=?1",
            [id],
            |r| r.get(0),
        )?;
        assert_eq!(version.as_deref(), cancellation.map(|(_, version)| version));
        assert!(matches!(
            store.reserve_send(
                &OrderReservation {
                    identity: &key,
                    version: NonZeroU64::MIN,
                    now: 102,
                    freshness: 30
                },
                ReserveOptions::default()
            )?,
            Err(SendRefusal::AlreadyReserved)
        ));
    }
    Ok(())
}

type LegacyParking = (
    i64,
    String,
    Option<&'static str>,
    &'static str,
    String,
    &'static str,
    serde_json::Value,
);

fn seed_adr0003_v6_parking(
    db: &Connection,
    receipt_id: ReceiptId,
    expected_deliveries: &[LegacyDelivery],
) -> Result<Vec<LegacyParking>, Box<dyn std::error::Error>> {
    let encoded_target = key(&receipt_id)?;
    let target = serde_json::from_str::<String>(&encoded_target)?;
    let encoded_delivery_target = key(&expected_deliveries
        .first()
        .ok_or("missing delivery fixture")?
        .0)?;
    let delivery_target = serde_json::from_str::<String>(&encoded_delivery_target)?;
    let parked = [
        (
            serde_json::json!([
                "interpretation_failure",
                encoded_target,
                "legacy parse failure"
            ]),
            "interpretation_failure",
            target.as_str(),
            "legacy parse failure",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!(["result_reconciliation", encoded_target, 1, "Unresolved"]),
            "result_reconciliation",
            target.as_str(),
            "result reconciliation: Unresolved",
            serde_json::json!({"Result":{"version":null,"index":1}}),
        ),
        (
            serde_json::json!([
                "interpretation_failure",
                encoded_target,
                "invalid syntax",
                3
            ]),
            "interpretation_failure",
            target.as_str(),
            "invalid syntax",
            serde_json::json!({"Interpretation":{"version":3}}),
        ),
        (
            serde_json::json!(["result_reconciliation", encoded_target, 2, "Conflict", 4]),
            "result_reconciliation",
            target.as_str(),
            "result reconciliation: Conflict",
            serde_json::json!({"Result":{"version":4,"index":2}}),
        ),
        (
            serde_json::json!(["parked", encoded_delivery_target, "receiver refused"]),
            "parked",
            delivery_target.as_str(),
            "receiver refused",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!(["conflict", encoded_delivery_target, "same identity changed"]),
            "conflict",
            delivery_target.as_str(),
            "same identity changed",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!(["incomplete_transfer", "transfer", "incomplete exact reason"]),
            "incomplete_transfer",
            "transfer",
            "incomplete exact reason",
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"kind":"autolyser_poll_custody","reason":"cursor outcome unknown","targets":["machine"]}),
            "autolyser_poll_custody",
            "machine",
            "cursor outcome unknown",
            serde_json::Value::Null,
        ),
    ];
    let mut expected_parking = Vec::new();
    for (original, kind, target, reason, context) in parked {
        // Both open and already resolved rows must survive in the same format.
        for resolution in [None, Some("unchanged resolution evidence")] {
            let encoded = original.to_string();
            db.execute(
                "INSERT INTO parked(evidence,resolution) VALUES (?1,?2)",
                params![encoded, resolution],
            )?;
            expected_parking.push((
                db.last_insert_rowid(),
                encoded,
                resolution,
                kind,
                target.to_owned(),
                reason,
                context.clone(),
            ));
        }
    }
    Ok(expected_parking)
}

fn seed_adr0003_v6_deliveries(
    db: &Connection,
    receipt_id: ReceiptId,
) -> Result<Vec<LegacyDelivery>, Box<dyn std::error::Error>> {
    let mut expected_deliveries = Vec::new();
    for (name, state) in [
        ("pending", DeliveryState::Pending),
        ("in_flight", DeliveryState::InFlight),
        ("done", DeliveryState::Done),
        ("parked", DeliveryState::Parked),
        ("paused", DeliveryState::Paused),
        ("conflict", DeliveryState::Conflict),
    ] {
        for (kind_name, kind) in [
            ("result", DeliveryKind::Result),
            ("order_event", DeliveryKind::OrderEvent),
        ] {
            let id = DeliveryId::try_from(uuid::Uuid::now_v7())?;
            let (receipt, version, order) = if kind == DeliveryKind::Result {
                (Some(key(&receipt_id)?), Some(1), None)
            } else {
                (None, None, Some("order"))
            };
            db.execute("INSERT INTO outbox(id,kind,receipt,version,order_id,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,'app','order',x'001122',?6,1,?7,99,100)",params![key(&id)?,kind_name,receipt,version,order, digest(&[0,17,34]).0.as_slice(),name])?;
            expected_deliveries.push((id, state, kind));
        }
    }

    Ok(expected_deliveries)
}

type LegacyDelivery = (DeliveryId, DeliveryState, DeliveryKind);

mod custody;

mod ownership;

mod legacy;

fn assert_empty_retention_tables(snapshot: &mut Snapshot) {
    for table in [
        "retention_key",
        "retention_holds",
        "retention_tombstones",
        "retention_deliveries",
        "retention_results",
    ] {
        assert_eq!(
            snapshot.remove(table),
            Some(Vec::new()),
            "new {table} must be empty"
        );
    }
}
