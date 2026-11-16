//! Tests order-change feed storage and pagination.
use crate::test_support::*;
use crate::*;

#[test]
fn adr0004_order_cache_distinguishes_empty_stale_reset_and_tombstones() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let destination = Identifier::new("app")?;
    let sample = SampleId::new("00012")?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NotTrusted
    ));
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: false,
        polled: 10,
        orders: vec![CachedOrder {
            id: Identifier::new("order")?,
            sample: sample.clone(),
            document: "synthetic-v1".into(),
            tombstone: false,
            expiry: 30,
        }],
    };
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NotTrusted
    ));
    page.complete = true;
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::Orders(_)
    ));
    assert!(matches!(
        store.orders(&destination, &sample, 15, 5)?,
        Lookup::NotTrusted
    ));
    assert!(matches!(
        store.orders(&destination, &sample, 9, 5)?,
        Lookup::NotTrusted
    ));
    for order in &mut page.orders {
        order.tombstone = true;
    }
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NoOrders
    ));
    store.reset_orders(&destination)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NotTrusted
    ));
    Ok(())
}

#[test]
fn adr0004_reset_snapshot_excludes_orders_absent_from_replacement() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let destination = Identifier::new("app")?;
    let sample = SampleId::new("00012")?;
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("first")?,
        complete: true,
        polled: 10,
        orders: vec![CachedOrder {
            id: Identifier::new("old")?,
            sample: sample.clone(),
            document: "old".into(),
            tombstone: false,
            expiry: 100,
        }],
    };
    store.order_page(&page)?;
    store.reset_orders(&destination)?;
    page.orders.clear();
    page.cursor = Identifier::new("replacement")?;
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 10, 5)?,
        Lookup::NoOrders
    ));
    assert_eq!(store.cursor(&destination)?, Some("replacement".into()));
    Ok(())
}

#[test]
fn adr0004_failed_page_never_advances_its_cursor() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let destination = Identifier::new("app")?;
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("first")?,
        complete: true,
        polled: 1,
        orders: vec![],
    };
    store.order_page(&page)?;
    store.db.execute_batch("CREATE TRIGGER reject_order BEFORE INSERT ON orders BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?;
    page.cursor = Identifier::new("must-not-commit")?;
    page.orders.push(CachedOrder {
        id: Identifier::new("order")?,
        sample: SampleId::new("0001")?,
        document: "synthetic".into(),
        tombstone: false,
        expiry: 100,
    });
    assert!(store.order_page(&page).is_err());
    assert_eq!(store.cursor(&destination)?, Some("first".into()));
    Ok(())
}

#[test]
fn mutation_adr0008_adr0004_feed_page_bounds_and_lookup_trust_are_independent() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("feed-bounds.db"))?;
    store.limits.page_rows = 1;
    let destination = Identifier::new("app")?;
    let sample = SampleId::new("000123")?;
    assert!(matches!(
        store.orders(&destination, &sample, 100, 30)?,
        Lookup::NotTrusted
    ));
    let order = CachedOrder {
        id: Identifier::new("order-a")?,
        sample: sample.clone(),
        document: "synthetic order".into(),
        tombstone: false,
        expiry: 200,
    };
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: false,
        polled: 100,
        orders: vec![serde_json::from_str(&key(&order)?)?],
    };
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 100, 30)?,
        Lookup::NotTrusted
    ));
    page.complete = true;
    store.order_page(&page)?;
    let Lookup::Orders(orders) = store.orders(&destination, &sample, 129, 30)? else {
        return Err("lost trusted order".into());
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(
        orders.first().ok_or("missing order")?.document,
        "synthetic order"
    );
    assert!(matches!(
        store.orders(&destination, &sample, 130, 30)?,
        Lookup::NotTrusted
    ));
    assert!(matches!(
        store.orders(&destination, &sample, 99, 30)?,
        Lookup::NotTrusted
    ));
    page.orders.push(CachedOrder {
        id: Identifier::new("order-b")?,
        ..order
    });
    assert!(matches!(store.order_page(&page), Err(Error::InvalidInput)));
    store.reset_orders(&destination)?;
    assert!(matches!(
        store.orders(&destination, &sample, 100, 30)?,
        Lookup::NotTrusted
    ));
    page.orders.clear();
    store.order_page(&page)?;
    assert!(matches!(
        store.orders(&destination, &sample, 100, 30)?,
        Lookup::NoOrders
    ));
    Ok(())
}
