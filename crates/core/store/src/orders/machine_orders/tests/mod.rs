//! Machine order regression tests kept beside the storage behavior they guard.
use crate::test_support::{TestResult, open};
use crate::*;
use analyte_model::{OrderState, SourceText};
use std::num::NonZeroU64;

fn identity() -> Result<MachineOrderKey, analyte_model::TextError> {
    Ok(MachineOrderKey {
        destination: Identifier::new("app")?,
        order: Identifier::new("order")?,
        machine: Identifier::new("machine")?,
    })
}

fn page(
    version: u64,
    state: &str,
    tombstone: bool,
    expiry: i64,
) -> Result<OrderPage, Box<dyn std::error::Error>> {
    Ok(OrderPage {
        destination: Identifier::new("app")?,
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 100,
        orders: vec![CachedOrder {
            id: Identifier::new("order")?,
            sample: SampleId::new("001a")?,
            document: serde_json::json!({"version":version,"state":state,"tombstone":tombstone,"tests":[{"namespace":"local","value":"test"}]})
                .to_string(),
            tombstone,
            expiry,
        }],
    })
}

fn event(order: &MachineOrder) -> Result<OrderEventDelivery, Error> {
    Ok(OrderEventDelivery {
        id: DeliveryId::try_from(uuid::Uuid::now_v7()).map_err(|_| Error::InvalidInput)?,
        destination: order.key.destination.clone(),
        order_id: order.key.order.clone(),
        payload: serde_json::to_vec(order)?,
        contract_version: 1,
    })
}

fn verify_adr0004_restart_and_reconciliation(path: &Path, reserved_id: i64) -> TestResult {
    let mut store = open(path)?;
    store.order_page(&page(2, "requested", false, 200)?)?;
    let version = NonZeroU64::new(2).ok_or("version")?;
    assert!(matches!(
        store.reserve_send(
            &OrderReservation {
                identity: &identity()?,
                version,
                now: 103,
                freshness: 30
            },
            ReserveOptions::default()
        )?,
        Err(SendRefusal::AlreadyReserved)
    ));
    let audit = Audit {
        actor: Identifier::new("operator")?,
        action: Identifier::new("orders-reconcile")?,
        targets: vec![Identifier::new(&reserved_id.to_string())?],
        time: 104,
        reason: analyte_model::Reason::new("checked synthetic machine work list")?,
    };
    store.reconcile_order(reserved_id, OrderState::Accepted, &audit, event)?;
    assert_eq!(
        store
            .machine_orders(&MachineOrderFilter {
                after: 0,
                machine: None,
                state: Some(OrderState::Accepted)
            })?
            .len(),
        1
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM machine_order_history", [], |r| r
                .get::<_, i64>(0))?,
        3
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        2
    );
    assert_eq!(store.audits(0)?.len(), 1);
    assert!(
        store
            .db
            .execute("DELETE FROM machine_order_history", [])
            .is_err()
    );
    assert!(
        store
            .db
            .execute("UPDATE machine_order_history SET evidence='{}'", [])
            .is_err()
    );
    Ok(())
}

fn test_groups(
    names: &[&str],
) -> Result<Vec<Vec<analyte_model::NamespacedId>>, analyte_model::TextError> {
    names
        .iter()
        .map(|name| {
            Ok(vec![analyte_model::NamespacedId {
                namespace: Identifier::new("local")?,
                value: Identifier::new(name)?,
            }])
        })
        .collect()
}

fn reserve_two(store: &mut Store) -> Result<MachineOrder, Box<dyn std::error::Error>> {
    let mut page = page(1, "requested", false, 200)?;
    page.orders.first_mut().ok_or("missing fixture")?.document = serde_json::json!({"version":1,"state":"requested","tombstone":false,"tests":[{"namespace":"local","value":"a"},{"namespace":"local","value":"b"}]}).to_string();
    store.order_page(&page)?;
    Ok(store
        .reserve_send(
            &OrderReservation {
                identity: &identity()?,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions {
                items: OrderItems::Groups {
                    groups: &test_groups(&["a", "b"])?,
                    namespace: Some(&Identifier::new("lab")?),
                    max_identity_bytes: 18,
                },
                ..Default::default()
            },
        )?
        .map_err(std::io::Error::other)?)
}

fn verify_plan11_rule8_audited_resume(store: &mut Store, mut audit: Audit) -> TestResult {
    audit.action = Identifier::new("orders-resume-after-restore")?;
    store.resume_after_restore(&audit)?;
    assert!(!store.restore_unreconciled()?);
    assert_eq!(
        store
            .audits(0)?
            .last()
            .ok_or("missing audit")?
            .1
            .action
            .as_str(),
        "orders-resume-after-restore"
    );
    assert!(
        store
            .reserve_send(
                &OrderReservation {
                    identity: &identity()?,
                    version: NonZeroU64::MIN,
                    now: 103,
                    freshness: 30
                },
                ReserveOptions::default()
            )?
            .is_ok()
    );
    Ok(())
}

fn verify_mutation_rule8_larger_cancel_page(
    store: &mut Store,
    base: &MachineOrderKey,
) -> TestResult {
    let key = MachineOrderKey {
        order: Identifier::new("order-c")?,
        ..identity()?
    };
    let mut requested = page(1, "requested", false, 200)?;
    requested
        .orders
        .first_mut()
        .ok_or("missing cache order")?
        .id = key.order.clone();
    store.order_page(&requested)?;
    let record = store
        .reserve_send(
            &OrderReservation {
                identity: &key,
                version: NonZeroU64::MIN,
                now: 101,
                freshness: 30,
            },
            ReserveOptions::default(),
        )?
        .map_err(std::io::Error::other)?;
    store.transition_order(
        record.id,
        OrderState::Accepted,
        SourceText::new("accepted")?,
        102,
        event,
    )?;
    let mut cancelled = page(2, "cancel_requested", false, 200)?;
    cancelled
        .orders
        .first_mut()
        .ok_or("missing cache order")?
        .id = key.order;
    store.order_page(&cancelled)?;
    store.limits.page_rows = 2;
    assert_eq!(
        store
            .cancel_candidates(&base.destination, &base.machine, 103, 30)?
            .len(),
        2
    );
    Ok(())
}

fn verify_adr0003_invalid_groups_and_identity_budget(
    store: &mut Store,
    candidate: &OrderReservation<'_>,
    namespace: Option<&Identifier>,
) -> Result<Vec<Vec<analyte_model::NamespacedId>>, Box<dyn std::error::Error>> {
    for groups in [
        vec![],
        vec![vec![]],
        test_groups(&["a", "b"])?,
        test_groups(&["a", "a", "c"])?,
        test_groups(&["a", "b", "unknown"])?,
    ] {
        assert!(matches!(
            store.reserve_send(
                candidate,
                ReserveOptions {
                    items: OrderItems::Groups {
                        groups: &groups,
                        namespace,
                        max_identity_bytes: 3
                    },
                    ..Default::default()
                }
            ),
            Err(Error::InvalidInput)
        ));
    }
    let one_per_test = test_groups(&["c", "a", "b"])?;
    let first = one_per_test
        .first()
        .ok_or("missing c")?
        .iter()
        .chain(one_per_test.get(1).ok_or("missing a")?)
        .cloned()
        .collect::<Vec<_>>();
    let groups = vec![first, one_per_test.get(2).ok_or("missing b")?.clone()];
    assert!(matches!(
        store.reserve_send(
            candidate,
            ReserveOptions {
                items: OrderItems::Groups {
                    groups: &groups,
                    namespace,
                    max_identity_bytes: 2
                },
                ..Default::default()
            }
        ),
        Err(Error::ItemIdentityTooLong)
    ));
    for table in [
        "machine_orders",
        "machine_order_items",
        "machine_order_history",
        "machine_order_item_history",
        "outbox",
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))?,
            0
        );
    }
    Ok(groups)
}

type TestGroups = Vec<Vec<analyte_model::NamespacedId>>;

fn seed_mutation_rule8_item_groups(
    store: &mut Store,
    count: usize,
    shape: &str,
) -> Result<(Vec<analyte_model::NamespacedId>, TestGroups), Box<dyn std::error::Error>> {
    let tests = (0..count)
        .map(|index| {
            Ok(analyte_model::NamespacedId {
                namespace: Identifier::new("synthetic")?,
                value: Identifier::new(&format!("test-{index}"))?,
            })
        })
        .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
    let mut cached = page(1, "requested", false, 200)?;
    let order = cached.orders.first_mut().ok_or("missing cache order")?;
    let mut document: serde_json::Value = serde_json::from_str(&order.document)?;
    document["tests"] = serde_json::to_value(&tests)?;
    order.document = document.to_string();
    store.order_page(&cached)?;
    let groups = match shape {
        "separate" => tests
            .iter()
            .cloned()
            .map(|test| vec![test])
            .collect::<Vec<_>>(),
        "single-group" => vec![tests.clone()],
        "empty-member" => vec![vec![], tests.clone()],
        "over-groups" => {
            let mut groups = vec![vec![]; 100];
            groups.push(tests.clone());
            groups
        }
        "together" | "empty-groups" => vec![],
        _ => unreachable!("fixed test cases"),
    };
    Ok((tests, groups))
}

mod mutation;

mod cancellation;

mod items;

mod lifecycle;

mod trust;
