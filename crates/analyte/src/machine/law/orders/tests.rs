//! Exercises superseded LAW admission against a temporary durable cache.
use super::*;
use crate::Error;

// ADR 0004
#[tokio::test]
async fn superseded_mapping_rejection_continues_idle_admission() -> Result<(), Error> {
    let definition = format!(
        "{}\n{}",
        include_str!("../../../../../core/config/examples/law.toml"),
        include_str!("../../../../../core/config/examples/law_orders.toml")
    );
    let snapshot = analyte_config::Draft::load(
        ("machine", &definition),
        (
            "site",
            include_str!("../../../../../core/config/examples/site.toml"),
        ),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    let machine = Machine {
        capture: false,
        destination_limits: crate::runtime::delivery::limits(
            &snapshot.effective().site().destination,
        ),
        hooks: crate::installation::config::MachineHooks::default(),
        id: Identifier::new("synthetic")?,
        status: crate::machine::link::status::Reporter::new(
            crate::machine::link::status::Link::of(snapshot.effective().stack()),
        ),
        snapshot,
        listener: None,
        definition: std::path::PathBuf::new(),
    };
    let temp = tempfile::tempdir()?;
    let mut store = Handle::start(
        temp.path().join("orders.db"),
        analyte_fixtures::store_limits(),
        Box::new(analyte_fixtures::UnlimitedSpace),
        4,
        crate::test_startup_deadline()?,
    )
    .await?;
    let mut document: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../docs/contract/vectors/order.json"
    ))?;
    let stale = analyte_store::CachedOrder {
        id: Identifier::new("order-1")?,
        sample: SampleId::new("000Ab12")?,
        document: serde_json::to_string(&document)?,
        tombstone: false,
        expiry: i64::MAX,
    };
    document["version"] = serde_json::json!(2);
    let current = analyte_store::CachedOrder {
        document: serde_json::to_string(&document)?,
        id: stale.id.clone(),
        sample: stale.sample.clone(),
        tombstone: false,
        expiry: i64::MAX,
    };
    let page = analyte_store::OrderPage {
        destination: Identifier::new("demo")?,
        cursor: Identifier::new("updated")?,
        complete: true,
        polled: jiff::Timestamp::now().as_second(),
        orders: vec![current],
    };
    store
        .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
            store.order_page(&page)?;
            Ok(())
        })
        .await?;
    let mut link = LawOrderLink::new()?;
    let analyte_config::Stack::Law(stack) = machine.snapshot.effective().stack() else {
        return Err("LAW fixture".into());
    };
    let settings = Settings {
        stack,
        orders: stack.orders().ok_or("LAW orders fixture")?,
        options: Options::default(),
    };
    // Missing specimen mapping rejects version 1, but version 2 already owns the cache.
    let prepared = link
        .reserve(&store, &machine, settings, vec![stale], deadline()?, true)
        .await?;
    assert!(prepared.is_empty());
    assert!(link.exchange.is_none());
    let key = reservation::key(&machine, Identifier::new("order-1")?);
    assert!(
        store
            .request(deadline()?, move |store| store.machine_order(&key))
            .await?
            .is_none()
    );
    store.shutdown(deadline()?).await?;
    Ok(())
}
