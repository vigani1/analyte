//! Tests restart recovery and reconciliation of interrupted orders whose
//! machine is no longer in the program file.
use super::*;
use analyte_fixtures::ConfigEdit as _;

const ORDER: &[u8] = include_bytes!("../../../../docs/contract/vectors/order.json");

fn order(id: &str) -> Result<analyte_contract::Order, Error> {
    let text = std::str::from_utf8(ORDER)?.rewrite("\"order-1\"", &format!("\"{id}\""));
    Ok(analyte_contract::decode(text.as_bytes())?)
}

/// Reserves `order` for the `astm` machine as the engine does, with the model
/// of its definition folder (`test/astm`), and leaves it Sending.
fn reserve(store: &mut analyte_store::Store, order: &str) -> Result<i64, Error> {
    let identity = analyte_store::MachineOrderKey {
        destination: analyte_model::Identifier::new("demo")?,
        order: analyte_model::Identifier::new(order)?,
        machine: analyte_model::Identifier::new("astm")?,
    };
    let reserved = store
        .reserve_send(
            &analyte_store::OrderReservation {
                identity: &identity,
                model: &analyte_model::Identifier::new("test/astm")?,
                version: std::num::NonZeroU64::MIN,
                now: jiff::Timestamp::now().as_second(),
                freshness: 3600,
            },
            analyte_store::ReserveOptions::default(),
        )?
        .map_err(|refusal| format!("fixture reservation refused: {refusal}"))?;
    Ok(reserved.id)
}

fn states(path: &Path) -> Result<Vec<(i64, String, String)>, Error> {
    let db = database(path)?;
    let mut statement =
        db.prepare("SELECT id,state,source_code FROM machine_orders ORDER BY id")?;
    Ok(statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?)
}

fn parked_model_unknown(path: &Path) -> Result<Vec<String>, Error> {
    let db = database(path)?;
    let mut statement = db.prepare("SELECT json_extract(evidence,'$.targets[0]') FROM parked WHERE json_extract(evidence,'$.kind')='order_event_model_unknown' ORDER BY id")?;
    Ok(statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?)
}

fn event_payloads(path: &Path) -> Result<Vec<serde_json::Value>, Error> {
    let db = database(path)?;
    let mut statement =
        db.prepare("SELECT payload FROM outbox WHERE kind='order_event' ORDER BY sequence")?;
    let payloads = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(payloads
        .iter()
        .map(|payload| serde_json::from_slice(payload))
        .collect::<Result<_, _>>()?)
}

// Safety rule 8 and ADR 0003 (a timeout never proves nothing happened), ADR
// 0033: an order interrupted while it was sent to a machine that has since
// been removed from the program file still becomes uncertain at restart, is
// listed for the operator and reaches the lab app with the model stored at
// reservation. An order stored before models were kept becomes uncertain too;
// its event is parked with the reason, never sent with a guessed model. Both
// can be reconciled, and nothing is sent to a machine again.
#[tokio::test]
async fn interrupted_orders_of_a_removed_machine_become_uncertain_and_reconcile()
-> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let reference = Reference::start(
        &directory.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![order("order-1")?, order("order-2")?])
        .await?;
    // First run with the ASTM machine, to cache both orders.
    let with_astm = config(directory.path(), true)?;
    let running = Running::start(&with_astm, &reference.url, "synthetic-api").await?;
    wait_feed(&with_astm).await?;
    wait_database_count(&with_astm, "SELECT count(*) FROM orders", [], 2).await?;
    running.stop().await?;
    assert!(states(&with_astm)?.is_empty(), "nothing was reserved yet");
    // The ASTM machine was sent both orders when the program stopped.
    let (kept, legacy) = {
        let mut store = analyte_store::Store::open(
            &directory.path().join("data/analyte.db"),
            analyte_fixtures::store_limits(),
            Box::new(Space),
        )?;
        (
            reserve(&mut store, "order-1")?,
            reserve(&mut store, "order-2")?,
        )
    };
    // What the store's step 19 leaves on an order kept before it.
    database(&with_astm)?.execute("UPDATE machine_orders SET model=NULL WHERE id=?1", [legacy])?;
    // The lab then removes the ASTM machine and restarts.
    let without_astm = config(directory.path(), false)?;
    let running = Running::start(&without_astm, &reference.url, "synthetic-api").await?;
    // The lab app accepts the one event that can name its model.
    wait_state(&without_astm, "done", 1).await?;
    running.stop().await?;
    assert_eq!(
        states(&without_astm)?,
        [
            (kept, "Uncertain".to_owned(), "host-restart".to_owned()),
            (legacy, "Uncertain".to_owned(), "host-restart".to_owned()),
        ],
        "both interrupted orders are uncertain and listed"
    );
    let events = event_payloads(&without_astm)?;
    assert_eq!(events.len(), 1, "no event for the legacy order");
    let event = events.first().ok_or("event missing")?;
    assert_eq!(event["order_id"], "order-1");
    assert_eq!(event["state"], "uncertain");
    assert_eq!(event["source"]["analyzer"], "astm");
    assert_eq!(event["source"]["model"], "test/astm");
    assert_eq!(
        event_payloads(&without_astm)?.len(),
        1,
        "no event for the legacy order"
    );
    assert_eq!(parked_model_unknown(&without_astm)?, [legacy.to_string()]);
    // `orders reconcile` works for both without the machine configured.
    let program = Program::load(&without_astm)?;
    for id in [kept, legacy] {
        analyte::orders::run(
            &program,
            analyte_model::Identifier::new("operator")?,
            analyte::orders::Command::Reconcile {
                id,
                outcome: analyte::orders::Outcome::Accepted,
                reason: "checked the machine's work list".into(),
            },
        )
        .await?;
    }
    let after = states(&without_astm)?;
    assert_eq!(
        after
            .iter()
            .map(|(id, state, _)| (*id, state.as_str()))
            .collect::<Vec<_>>(),
        [(kept, "Accepted"), (legacy, "Accepted")]
    );
    let events = event_payloads(&without_astm)?;
    let reconciled = events.last().ok_or("reconciled event missing")?;
    assert_eq!(events.len(), 2);
    assert_eq!(reconciled["state"], "accepted");
    assert_eq!(reconciled["source"]["model"], "test/astm");
    assert_eq!(
        parked_model_unknown(&without_astm)?,
        [legacy.to_string(), legacy.to_string()]
    );
    // Rule 8: no new reservation and no Sending snapshot after the restart.
    assert_eq!(
        database(&without_astm)?.query_row(
            "SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='Sending'",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        2,
        "only the two original reservations were ever Sending"
    );
    reference.close().await?;
    Ok(())
}
