//! Tests the machine model kept with each machine order and the parked event
//! of an order stored before it was kept.
use super::*;

fn reserve(
    store: &mut Store,
    model: &Identifier,
) -> Result<MachineOrder, Box<dyn std::error::Error>> {
    store.order_page(&page(1, "requested", false, 200)?)?;
    Ok(store
        .reserve_send(
            &OrderReservation {
                model,
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions::default(),
        )?
        .map_err(std::io::Error::other)?)
}

fn count(store: &Store, sql: &str) -> Result<i64, rusqlite::Error> {
    store.db.query_row(sql, [], |row| row.get(0))
}

/// The open parked items' kind, target and reason, oldest first.
fn parked(store: &Store) -> Result<Vec<(String, String, String)>, rusqlite::Error> {
    let mut statement = store.db.prepare("SELECT json_extract(evidence,'$.kind'),json_extract(evidence,'$.targets[0]'),json_extract(evidence,'$.reason') FROM parked WHERE resolution IS NULL ORDER BY id")?;
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect()
}

// ADR 0033: the model an order was reserved for is kept with it, so its
// events can name it after the machine leaves the program file.
#[test]
fn the_reserved_model_is_kept_through_restart() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("orders.db");
    let model = Identifier::new("roche/cobas-e411")?;
    let mut store = open(&path)?;
    let reserved = reserve(&mut store, &model)?;
    assert_eq!(reserved.model(), Some(&model));
    drop(store);
    let mut store = open(&path)?;
    let mut built = None;
    let uncertain = store.transition_order(
        reserved.id,
        OrderState::Uncertain,
        SourceText::new("host-restart")?,
        102,
        |record| {
            built = record.model().cloned();
            event(record)
        },
    )?;
    assert_eq!(uncertain.model(), Some(&model));
    assert_eq!(
        built,
        Some(model),
        "the event builder sees the stored model"
    );
    assert_eq!(count(&store, "SELECT count(*) FROM outbox")?, 1);
    assert!(parked(&store)?.is_empty());
    Ok(())
}

// Safety rules 2 and 8 (ADR 0003, ADR 0033): an order stored before its model
// was kept still changes state and stays visible; its event is parked with
// the reason, never built with a guessed model, also after reconciliation.
#[test]
fn an_order_without_a_stored_model_parks_its_events() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("orders.db"))?;
    let reserved = reserve(&mut store, &test_support::order_model())?;
    // What step 19 leaves on an order stored before it.
    store
        .db
        .execute("UPDATE machine_orders SET model=NULL", [])?;
    let never = |_: &MachineOrder| -> Result<OrderEventDelivery, OrderError> {
        Err(OrderError::InvalidOrderInput)
    };
    let uncertain = store.transition_order(
        reserved.id,
        OrderState::Uncertain,
        SourceText::new("host-restart")?,
        102,
        never,
    )?;
    assert_eq!(uncertain.state, OrderState::Uncertain);
    assert_eq!(uncertain.model(), None);
    assert_eq!(
        store
            .machine_orders(&MachineOrderFilter {
                after: 0,
                machine: None,
                state: Some(OrderState::Uncertain),
            })?
            .len(),
        1,
        "the uncertain order is listed for the operator"
    );
    assert_eq!(count(&store, "SELECT count(*) FROM outbox")?, 0);
    let expected = (
        ParkedKind::OrderEventModelUnknown.as_str().to_owned(),
        reserved.id.to_string(),
        MODEL_UNKNOWN_REASON.to_owned(),
    );
    assert_eq!(parked(&store)?, std::slice::from_ref(&expected));
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("orders-reconcile")?,
        targets: vec![Identifier::new(&reserved.id.to_string())?],
        time: 103,
        reason: analyte_model::Reason::new("checked the machine's work list")?,
    };
    let accepted = store.reconcile_order(reserved.id, OrderState::Accepted, &audit, never)?;
    assert_eq!(accepted.state, OrderState::Accepted);
    assert_eq!(count(&store, "SELECT count(*) FROM outbox")?, 0);
    assert_eq!(parked(&store)?, [expected.clone(), expected]);
    assert_eq!(store.audits(0)?.len(), 1);
    Ok(())
}

// ADR 0033: the column holds only a `vendor/model` folder key.
#[test]
fn the_store_refuses_a_malformed_model() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("orders.db"))?;
    reserve(&mut store, &test_support::order_model())?;
    for malformed in [
        "Roche/e411",
        "roche",
        "roche/e411/x",
        "/e411",
        "roche/",
        "roche/e 411",
    ] {
        assert_constraint(
            store
                .db
                .execute("UPDATE machine_orders SET model=?1", [malformed]),
        );
    }
    Ok(())
}
