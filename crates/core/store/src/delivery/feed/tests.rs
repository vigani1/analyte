//! Tests order-change feed storage and pagination.
use crate::test_support::*;
use crate::*;

// ADR 0004
#[test]
fn order_cache_tells_empty_stale_reset_and_tombstones() -> TestResult {
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
            document: r#"{"version":1}"#.into(),
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

// ADR 0004
#[test]
fn reset_snapshot_drops_orders_absent_from_replacement() -> TestResult {
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

// ADR 0004
#[test]
fn failed_page_never_advances_its_cursor() -> TestResult {
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
    {
        let refusal = store
            .order_page(&page)
            .map(|_| ())
            .expect_err("input must be refused");
        assert!(
            matches!(
                &refusal,
                OrderError::Storage(StorageError::ConstraintViolation(_))
            ),
            "unexpected refusal: {refusal:?}"
        );
    }
    assert_eq!(store.cursor(&destination)?, Some("first".into()));
    Ok(())
}

// ADR 0008, ADR 0004; mutation-testing regression
#[test]
fn feed_page_bounds_and_lookup_trust_are_independent() -> TestResult {
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
        document: r#"{"version":1}"#.into(),
        tombstone: false,
        expiry: 200,
    };
    let mut page = OrderPage {
        destination: destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: false,
        polled: 100,
        orders: vec![serde_json::from_str(&document(&order)?)?],
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
        r#"{"version":1}"#
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
    assert!(matches!(
        store.order_page(&page),
        Err(OrderError::InvalidOrderInput)
    ));
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

fn versioned(version: u64, state: &str) -> Result<OrderPage, Box<dyn std::error::Error>> {
    Ok(OrderPage {
        destination: Identifier::new("app")?,
        cursor: Identifier::new(&format!("cursor-{version}-{state}"))?,
        complete: true,
        polled: 100,
        orders: vec![CachedOrder {
            id: Identifier::new("order")?,
            sample: SampleId::new("001a")?,
            document: serde_json::json!({
                "version": version,
                "state": state,
                "tombstone": false,
                "tests": [{"namespace": "local", "value": "test"}],
            })
            .to_string(),
            tombstone: false,
            expiry: 200,
        }],
    })
}

fn stored_document(store: &Store) -> Result<String, Box<dyn std::error::Error>> {
    let app = Identifier::new("app")?;
    let order = Identifier::new("order")?;
    Ok(store
        .cached_order(&app, &order)?
        .ok_or("cache absent")?
        .document)
}

// Contract v1 "GET /orders": each order carries a positive version; ADR 0004 rechecks
// version and cancellation before sending, so a replayed older version never wins.
#[test]
fn older_version_refused() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let cancelled = versioned(2, "cancel_confirmed")?;
    store.order_page(&cancelled)?;
    store.order_page(&versioned(1, "requested")?)?;
    assert_eq!(
        stored_document(&store)?,
        cancelled.orders.first().ok_or("order")?.document
    );
    let (app, machine) = (Identifier::new("app")?, Identifier::new("machine")?);
    assert!(matches!(
        store.pending_orders(&app, &machine, "", 101, 30)?,
        PendingOrders::Page { orders, .. } if orders.is_empty()
    ));
    let identity = MachineOrderKey {
        destination: app,
        order: Identifier::new("order")?,
        machine,
    };
    let reservation = OrderReservation {
        model: &order_model(),
        identity: &identity,
        version: std::num::NonZeroU64::MIN,
        now: 101,
        freshness: 30,
    };
    assert!(matches!(
        store.reserve_send(&reservation, ReserveOptions::default())?,
        Err(SendRefusal::VersionChanged)
    ));
    assert_eq!(store.parked(0)?.len(), 1);
    Ok(())
}

// Contract v1 "GET /orders" and 410 reload: a snapshot replaying an older version keeps
// the newer cached order in the reloaded cache, so it is never reported absent.
#[test]
fn older_version_in_reload_keeps_newer() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let app = Identifier::new("app")?;
    store.order_page(&versioned(2, "cancel_confirmed")?)?;
    store.reset_orders(&app)?;
    store.order_page(&versioned(1, "requested")?)?;
    let identity = MachineOrderKey {
        destination: app,
        order: Identifier::new("order")?,
        machine: Identifier::new("machine")?,
    };
    let reservation = OrderReservation {
        model: &order_model(),
        identity: &identity,
        version: std::num::NonZeroU64::MIN.saturating_add(1),
        now: 101,
        freshness: 30,
    };
    assert!(matches!(
        store.reserve_send(&reservation, ReserveOptions::default())?,
        Err(SendRefusal::NotRequested)
    ));
    Ok(())
}

// Contract v1 "GET /orders": one order version has one content; a different document
// under the same version is a conflict, kept for the operator, never applied.
#[test]
fn equal_version_conflict() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let first = versioned(2, "requested")?;
    store.order_page(&first)?;
    store.order_page(&first)?;
    assert!(store.parked(0)?.is_empty());
    store.order_page(&versioned(2, "cancel_confirmed")?)?;
    assert_eq!(
        stored_document(&store)?,
        first.orders.first().ok_or("order")?.document
    );
    assert_eq!(store.parked(0)?.len(), 1);
    Ok(())
}

fn vector(bytes: &[u8]) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(bytes)
}

// Contract v1 "GET /orders", published vector order-versions.json: an order's version only
// increases and one version has one content; a refused offer keeps the cache and is parked.
#[test]
fn order_version_vector_keeps_newer_and_parks() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let steps = vector(include_bytes!(
        "../../../../../../docs/contract/vectors/order-versions.json"
    ))?;
    let mut parked = 0_usize;
    for (index, step) in steps.as_array().ok_or("steps")?.iter().enumerate() {
        let offer = step.get("offer").ok_or("offer")?;
        store.order_page(&OrderPage {
            destination: Identifier::new("app")?,
            cursor: Identifier::new(&format!("cursor-{index}"))?,
            complete: true,
            polled: 100,
            orders: vec![CachedOrder {
                id: Identifier::new(offer["id"].as_str().ok_or("id")?)?,
                sample: SampleId::new(offer["sample_id"].as_str().ok_or("sample")?)?,
                document: offer.to_string(),
                tombstone: false,
                expiry: i64::MAX,
            }],
        })?;
        let kept: serde_json::Value = serde_json::from_str(
            &store
                .cached_order(&Identifier::new("app")?, &Identifier::new("order-1")?)?
                .ok_or("cache absent")?
                .document,
        )?;
        let name = step["step"].as_str().ok_or("step")?;
        assert_eq!(kept["version"], step["kept_version"], "{name}");
        assert_eq!(kept["state"], step["kept_state"], "{name}");
        if step["parked"].as_bool().ok_or("parked")? {
            parked = parked.saturating_add(1);
        }
        assert_eq!(store.parked(0)?.len(), parked, "{name}");
    }
    Ok(())
}

// Contract v1 "GET /orders" (ADR 0004), published vector freshness.json: only a chain that
// reaches complete refreshes freshness, counted from when that chain began.
#[test]
fn freshness_vector_counts_complete_chains_only() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("store.db"))?;
    let app = Identifier::new("app")?;
    let vector = vector(include_bytes!(
        "../../../../../../docs/contract/vectors/freshness.json"
    ))?;
    let freshness = vector["freshness_seconds"].as_u64().ok_or("freshness")?;
    for chain in vector["chains"].as_array().ok_or("chains")? {
        let name = chain["chain"].as_str().ok_or("chain")?;
        let started = chain["started"].as_i64().ok_or("started")?;
        for (page, complete) in chain["pages"].as_array().ok_or("pages")?.iter().enumerate() {
            store.order_page(&OrderPage {
                destination: app.clone(),
                cursor: Identifier::new(&format!("{name}-{page}"))?,
                complete: complete.as_bool().ok_or("complete")?,
                polled: started,
                orders: Vec::new(),
            })?;
        }
        let finished = chain["finished"].as_i64().ok_or("finished")?;
        let health = store.health(&app, finished, freshness)?;
        assert_eq!(
            health.last_successful_poll,
            chain["fresh_from"].as_i64(),
            "{name}"
        );
        assert_eq!(
            Some(health.cache_trusted),
            chain["trusted_at_finish"].as_bool(),
            "{name}"
        );
    }
    Ok(())
}

// Contract v1 `GET /orders`: the reason Analyte parks when it keeps its cached
// order reads back exactly, and any other spelling does not.
#[test]
fn order_refusals_read_back_only_their_exact_spelling() -> TestResult {
    let version = |n| std::num::NonZeroU64::new(n).ok_or("zero");
    for (refusal, stored) in [
        (
            OrderRefusal::Older {
                offered: version(2)?,
                kept: version(5)?,
            },
            "app sent order version 2 after cached version 5; kept 5",
        ),
        (
            OrderRefusal::ChangedContent { kept: version(3)? },
            "app sent different content for order version 3; kept the cached one",
        ),
    ] {
        assert_eq!(refusal.to_string(), stored);
        assert_eq!(OrderRefusal::from_stored(stored), Some(refusal));
    }
    for text in [
        "",
        "app sent order version 2 after cached version 5; kept 6",
        "app sent order version 02 after cached version 5; kept 5",
        "app sent order version 0 after cached version 5; kept 5",
        "app sent different content for order version 3; kept the cached one.",
        "app sent different content for order version ; kept the cached one",
    ] {
        assert_eq!(OrderRefusal::from_stored(text), None, "{text}");
    }
    Ok(())
}
