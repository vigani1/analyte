//! Tests the restyled More pages (admin restyle b5) through the real handlers: Machine orders and an order item, Cancelled by machine, Deliveries with Send again, Data keeping with its holds, and Settings.
use super::machines::{AT, CODE, VALUE, evidence, id, key, receive, result};
use super::overview::{empty_store, stop};
use super::pages::{csrf, encode, login};
use super::*;
use crate::machine::link::status::{Link, Reporter};
use analyte_model::{Digest, ReceiptId, SourceText};
use analyte_store::{Audit, CachedOrder, HoldTarget, MachineOrderKey, OrderPage, Reading};

/// The lab app the scene's machines send to.
const LAB: &str = "main-lab";

/// A key value no page may ever show.
const SECRET: &str = "synthetic-secret-value-0451";

/// The scene's machines and their models, as the More mockups name them.
pub(super) const MACHINES: [(&str, &str); 4] = [
    ("chemistry-analyzer", "Chemistry analyzer XC-200"),
    ("hematology-analyzer", "Hematology analyzer HM-5"),
    ("immunoassay-analyzer", "Immunoassay analyzer IA-900"),
    ("coagulation-analyzer", "Coagulation analyzer CG-7"),
];

/// What [`seed`] stored that the tests look for.
#[derive(Default)]
pub(super) struct Seeded {
    pub(super) delivered: Option<uuid::Uuid>,
    /// A delivered order message, which Send again may queue again.
    pub(super) event: Option<uuid::Uuid>,
    pub(super) rejected: Option<uuid::Uuid>,
    pub(super) waiting: Option<uuid::Uuid>,
    /// The order whose outcome is unknown.
    pub(super) unknown: i64,
    /// The order a machine cancelled, item 1.
    pub(super) cancelled: i64,
    /// The order that was done, with no message about it.
    pub(super) done: i64,
    /// The message held from clean-up.
    pub(super) held: Option<ReceiptId>,
}

/// A running admin server over a store seeded like the More mockups, signed in.
pub(super) struct Scene {
    pub(super) directory: tempfile::TempDir,
    pub(super) store: Arc<analyte_store::Handle>,
    pub(super) started: Started,
    pub(super) bearer: String,
    pub(super) auth: String,
    pub(super) post: String,
    pub(super) seeded: Seeded,
}

impl Scene {
    /// A scene with every state the mockups show when `full`, or an empty store.
    pub(super) async fn start(full: bool) -> Result<Self, Error> {
        let directory = crate::admin::private_tempdir()?;
        let root = directory.path();
        let bearer = crate::admin::rotate_token(root)?;
        std::fs::write(root.join("program.toml"), "")?;
        std::fs::write(
            root.join("lab-apps.toml"),
            format!(
                "schema_version = 1\n{}{}",
                lab_app(LAB, "https://lab-main.hospital.internal/api"),
                lab_app("outpatient-lab", "https://outpatient.hospital.internal/api")
            ),
        )?;
        let mut updates = Vec::new();
        for name in ["main-lab-api", "main-lab-signing", "alerts-signing"] {
            let name = Identifier::new(name)?;
            let secret = crate::installation::credentials::Secret::new(&name, SECRET.into())?;
            updates.push((name, secret));
        }
        crate::installation::credentials::Locked::new(root).replace(updates, AT)?;
        let store = empty_store(root).await?;
        let seeded = if full {
            seed(&store, &root.join("analyte.db")).await?
        } else {
            Seeded::default()
        };
        let machines = MACHINES
            .iter()
            .map(|&(name, model)| {
                let mut machine =
                    machine(name, true, Reporter::new(Link::Serial).subscribe(), LAB)?;
                machine.model = Some(model.into());
                Ok(machine)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let retention = vec![
            (Identifier::new("chemistry-analyzer")?, Some(365)),
            (Identifier::new("hematology-analyzer")?, None),
        ];
        let installed = crate::admin::more::Installed::for_test(vec![(
            Identifier::new("lab-alerts")?,
            Identifier::new("alerts-signing")?,
        )]);
        let started = start_full(
            root,
            &store,
            (machines, Vec::new()),
            (crate::admin::Draining::new(), root),
            (retention, installed),
        )
        .await?;
        let cookie = login(&started.3, &bearer).await?;
        let auth = format!("Cookie: {cookie}\r\n");
        let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
        Ok(Self {
            directory,
            store,
            started,
            bearer,
            auth,
            post,
            seeded,
        })
    }

    /// GET `path` signed in.
    pub(super) async fn get(&self, path: &str) -> Result<String, Error> {
        request(&self.started.3, path, &self.auth, None).await
    }

    /// GET `path` with the admin key alone.
    pub(super) async fn get_with_key(&self, path: &str) -> Result<String, Error> {
        let auth = format!("Authorization: Bearer {}\r\n", self.bearer);
        request(&self.started.3, path, &auth, None).await
    }

    /// POST `form` to `path` signed in.
    pub(super) async fn post(&self, path: &str, form: &str) -> Result<String, Error> {
        request(&self.started.3, path, &self.post, Some(form)).await
    }

    /// The session's form token, read from a page.
    pub(super) async fn token(&self) -> Result<String, Error> {
        Ok(encode(&csrf(&self.get("/deliveries").await?)?))
    }

    /// The store file, for direct reads and fault injection.
    pub(super) fn database(&self) -> std::path::PathBuf {
        self.directory.path().join("analyte.db")
    }

    pub(super) async fn stop(self) -> Result<(), Error> {
        let (state, stopper, owner, _) = self.started;
        stop(state, stopper, owner, self.store).await
    }
}

/// One `[[lab_apps]]` table.
fn lab_app(name: &str, url: &str) -> String {
    format!(
        "[[lab_apps]]\nname = '{name}'\nbase_url = '{url}'\napi_key_name = '{name}-api'\nsigning_key_name = '{name}-signing'\n"
    )
}

/// The store's key for a delivery: its UUID as a JSON string.
fn delivery_key(id: uuid::Uuid) -> Result<String, Error> {
    Ok(serde_json::to_string(&id.to_string())?)
}

/// Stores the scene: four messages with results, their deliveries in each
/// state and an order message; six orders sent to machines, one with an
/// unknown outcome and one a machine cancelled with its message; two holds
/// with who placed them; and one clean-up.
async fn seed(store: &analyte_store::Handle, database: &std::path::Path) -> Result<Seeded, Error> {
    let messages = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let mut stored = Vec::new();
            for (machine, ago, sample, results) in [
                ("hematology-analyzer", 3_600, "261119-044", 6),
                ("immunoassay-analyzer", 3_000, "261119-031", 8),
                ("coagulation-analyzer", 2_400, "261119-029", 3),
                ("chemistry-analyzer", 1_800, "261119-038", 12),
            ] {
                let at = AT.saturating_sub(ago);
                stored.push((receive(s, machine, at)?, sample, results, at));
            }
            Ok(stored)
        })
        .await?;
    let db = rusqlite::Connection::open(database)?;
    let mut seeded = Seeded::default();
    for ((receipt, sample, results, at), state) in
        messages.iter().zip(["paused", "done", "done", "parked"])
    {
        let readings = (0..*results)
            .map(|_| result(*receipt, Some(sample)))
            .collect::<Result<Vec<_>, _>>()?;
        db.execute(
            "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
            rusqlite::params![key(*receipt)?, evidence(Reading::Results(readings), None)?],
        )?;
        let delivery = uuid::Uuid::now_v7();
        db.execute(
            "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,'result',?2,1,?3,'k',x'00',x'00',1,?4,0,?5)",
            rusqlite::params![delivery_key(delivery)?, key(*receipt)?, LAB, state, at.saturating_add(5)],
        )?;
        match state {
            "done" => seeded.delivered = Some(delivery),
            "parked" => seeded.rejected = Some(delivery),
            _ => {}
        }
    }
    let [(hematology, ..), _, (coagulation, ..), _] = messages.as_slice() else {
        return Err("four messages".into());
    };
    // A delivery queued by an older version, whose queue time was filled in later.
    let waiting = uuid::Uuid::now_v7();
    db.execute(
        "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at,enqueue_time_backfilled) VALUES (?1,'result',?2,1,?3,'k2',x'00',x'00',1,'pending',0,1,1)",
        rusqlite::params![delivery_key(waiting)?, key(*hematology)?, LAB],
    )?;
    seeded.waiting = Some(waiting);
    seeded.held = Some(*hematology);
    drop(db);
    let orders = seed_orders(store).await?;
    // The lab app accepted every order message but the newest.
    let db = rusqlite::Connection::open(database)?;
    db.execute(
        "UPDATE outbox SET state='done' WHERE kind='order_event' AND sequence<(SELECT max(sequence) FROM outbox WHERE kind='order_event')",
        [],
    )?;
    let event: String = db.query_row(
        "SELECT id FROM outbox WHERE kind='order_event' AND state='done' ORDER BY sequence LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    seeded.event = Some(serde_json::from_str::<String>(&event)?.parse()?);
    drop(db);
    let [_, _, done, unknown, cancelled, _] = orders.as_slice() else {
        return Err("six orders".into());
    };
    seeded.unknown = *unknown;
    seeded.cancelled = *cancelled;
    seeded.done = *done;
    seed_cancel(database, *cancelled, *coagulation, store).await?;
    seed_keeping(store, database, *hematology).await?;
    Ok(seeded)
}

/// The scene's orders: number, sample, machine and the states it moves through.
const PLAN: [(&str, &str, &str, &[OrderState]); 6] = [
    ("ORD-1047", "261119-047", "immunoassay-analyzer", &[]),
    (
        "ORD-1033",
        "261119-033",
        "immunoassay-analyzer",
        &[OrderState::Sent],
    ),
    (
        "ORD-1040",
        "261119-040",
        "hematology-analyzer",
        &[OrderState::Sent, OrderState::Accepted, OrderState::Done],
    ),
    (
        "ORD-1021",
        "261119-021",
        "chemistry-analyzer",
        &[OrderState::Uncertain],
    ),
    (
        "ORD-1018",
        "261119-018",
        "coagulation-analyzer",
        &[OrderState::Sent, OrderState::Accepted],
    ),
    (
        "ORD-1012",
        "261119-012",
        "chemistry-analyzer",
        &[OrderState::Sent, OrderState::Accepted, OrderState::Done],
    ),
];

/// Six orders from the lab app, each sent to a machine and moved to its state;
/// returns their machine order numbers.
async fn seed_orders(store: &analyte_store::Handle) -> Result<Vec<i64>, Error> {
    Ok(store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            s.order_page(&OrderPage {
                destination: id(LAB)?,
                cursor: id("scene")?,
                complete: true,
                polled: AT - 4_000,
                orders: PLAN
                    .iter()
                    .map(|&(order, sample, _, _)| {
                        Ok(CachedOrder {
                            id: id(order)?,
                            sample: analyte_model::SampleId::new(sample)
                                .map_err(FixtureError::Text)?,
                            document: serde_json::json!({
                                "id": order, "version": 1, "state": "requested",
                                "sample_id": sample,
                                "tests": [{"namespace": "local", "value": CODE}],
                                "priority": "routine", "target_analyzers": null,
                                "expires_at": null, "tombstone": false,
                            })
                            .to_string(),
                            tombstone: false,
                            expiry: AT + 86_400,
                        })
                    })
                    .collect::<Result<_, FixtureError>>()?,
            })?;
            let mut ids = Vec::new();
            for (step, &(order, _, machine, states)) in PLAN.iter().enumerate() {
                let step = i64::try_from(step).map_err(|_| FixtureError::Conflict)?;
                let now = AT
                    .saturating_sub(3_900)
                    .saturating_add(step.saturating_mul(60));
                let key = MachineOrderKey {
                    destination: id(LAB)?,
                    order: id(order)?,
                    machine: id(machine)?,
                };
                let record = s
                    .reserve_send(
                        &analyte_store::OrderReservation {
                            model: &id("example/synthetic")?,
                            identity: &key,
                            version: std::num::NonZeroU64::MIN,
                            now,
                            freshness: 86_400,
                        },
                        analyte_store::ReserveOptions::default(),
                    )?
                    .map_err(|_| FixtureError::Conflict)?;
                for (offset, &state) in (1_i64..).zip(states) {
                    let code = if state == OrderState::Uncertain {
                        "disconnected before confirming"
                    } else {
                        "acknowledged"
                    };
                    s.transition_order(
                        record.id,
                        state,
                        SourceText::new(code).map_err(FixtureError::Text)?,
                        now.saturating_add(offset),
                        |order| {
                            Ok(analyte_store::OrderEventDelivery {
                                id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7())
                                    .map_err(analyte_store::OrderError::Identity)?,
                                destination: order.key.destination.clone(),
                                order_id: order.key.order.clone(),
                                payload: b"order event".to_vec(),
                                contract_version: 1,
                            })
                        },
                    )?;
                }
                ids.push(record.id);
            }
            Ok(ids)
        })
        .await?)
}

/// The machine's own cancel of `order`'s item 1, in message `receipt`, with
/// the times it sent and a note, as a LAW status message records them.
async fn seed_cancel(
    database: &std::path::Path,
    order: i64,
    receipt: ReceiptId,
    store: &analyte_store::Handle,
) -> Result<(), Error> {
    let placer = store
        .request::<_, analyte_store::ReconciliationError>(fixture_deadline()?, move |s| {
            Ok(s.machine_order_by_id(order)?
                .items
                .first()
                .map(|item| item.placer.clone()))
        })
        .await?
        .ok_or("order item")?;
    // ORC-9, the event time, and ORC-27, the expected ready time, as sent.
    let control: Vec<&str> = (0..28)
        .map(|field| match field {
            0 => "ORC",
            9 => "20261119095600",
            27 => "20261119103000",
            _ => "",
        })
        .collect();
    let custody = analyte_store::law::Custody {
        scope: SourceText::new("coagulation-analyzer/sender")?,
        message: Identifier::new("MSG-1018")?,
        work: vec![analyte_store::law::Work {
            order,
            item: 1,
            placer: Identifier::new(&placer)?,
            command: Digest([0; 32]),
            occurrence: 1,
            execution: Some(analyte_model::AwosExecution::AnalyzerCancelled),
            complete: false,
            source: SourceText::new(
                &serde_json::json!({
                    "control": control.join("|"),
                    "header_time": "20261119095600+0100",
                    "notes": ["NTE|||Sample <not> found on the rack"],
                })
                .to_string(),
            )?,
        }],
        rejection: None,
        recorded_at: AT - 2_300,
    };
    let db = rusqlite::Connection::open(database)?;
    db.execute(
        "INSERT INTO awos_execution_history(machine_order,item_index,receipt,interpretation,state,recorded_at,cancel_outstanding) VALUES (?1,1,?2,1,'AnalyzerCancelled',?3,0)",
        rusqlite::params![order, key(receipt)?, AT - 2_300],
    )?;
    db.execute(
        "INSERT INTO law_messages(receipt,version,scope,message,digest,disposition,evidence) VALUES (?1,1,'scene','MSG-1018',x'00',?2,?3)",
        rusqlite::params![
            key(receipt)?,
            serde_json::to_string(&analyte_store::law::Disposition::Accepted)?,
            serde_json::to_string(&custody)?
        ],
    )?;
    Ok(())
}

/// Two holds placed by people with their reasons, and one clean-up.
async fn seed_keeping(
    store: &analyte_store::Handle,
    database: &std::path::Path,
    held: ReceiptId,
) -> Result<(), Error> {
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            let place = |actor: &str, time, reason: &str| -> Result<Audit, FixtureError> {
                Ok(Audit {
                    actor: id(actor)?,
                    action: id("retention-hold-place")?,
                    targets: Vec::new(),
                    time,
                    reason: Reason::new(reason).map_err(FixtureError::Text)?,
                })
            };
            s.set_retention_hold(
                &HoldTarget::Receipt(held),
                true,
                &place("admin-session-1", AT - 1_000, "Audit of hematology results")?,
            )?;
            s.set_retention_hold(
                &HoldTarget::Order {
                    destination: id(LAB)?,
                    order: id("ORD-1018")?,
                },
                true,
                &place("operator", AT - 900, "Complaint follow-up")?,
            )?;
            Ok(())
        })
        .await?;
    let run = analyte_store::PurgeRun {
        orders: 2,
        order_after: None,
        order_range: None,
        deliveries: 9,
        results: 14,
        failed: false,
        days: Some(365),
        machine: Identifier::new("chemistry-analyzer")?,
        time: AT - 7_200,
        examined: 40,
        purged: 9,
        first: None,
        last: None,
        after: None,
        paused: false,
    };
    rusqlite::Connection::open(database)?.execute(
        "INSERT INTO audit(evidence) VALUES (?1)",
        [serde_json::json!({
            "actor": "analyte", "action": "retention-purge", "targets": [],
            "time": AT - 7_200, "reason": "scheduled clean-up", "retention": run,
        })
        .to_string()],
    )?;
    Ok(())
}

/// No page shows a test code, a result value or a key's value.
fn assert_private(path: &str, page: &str) {
    for hidden in [CODE, VALUE, SECRET] {
        assert!(!page.contains(hidden), "{path} shows {hidden}");
    }
}

/// The text of a page people read: its markup without attribute values, so
/// IDs kept on hover are not counted as shown.
fn shown(page: &str) -> String {
    let mut text = String::new();
    let mut inside = false;
    for character in page.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            other if !inside => text.push(other),
            _ => {}
        }
    }
    text
}

// Owner request 2026-11-20 (admin restyle b5, the More mockup): the filters
// count orders per state, each row shows its sample, machine and model, a test
// count and one of the separate states (safety rule 8), with what was recorded
// under an unknown outcome; test codes never appear.
#[tokio::test(flavor = "multi_thread")]
async fn machine_orders_are_filtered_and_never_show_test_codes() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let all = scene.get("/orders").await?;
    assert!(all.starts_with("HTTP/1.1 200"), "{all}");
    assert_private("/orders", &all);
    for part in [
        "<h1>Machine orders</h1>",
        "<a class=\"chip\" href=\"/orders\" aria-current=\"page\">All <span class=\"chip__n\">6</span></a>",
        "<a class=\"chip\" href=\"/orders?show=unknown\">Unknown <span class=\"chip__n\">1</span></a>",
        "<a class=\"chip\" href=\"/orders?show=cancelled\">Cancelled <span class=\"chip__n\">0</span></a>",
        "<td class=\"num\">261119-021</td>",
        "<td class=\"num\"><span title=\"Lab app main-lab · order version 1 sent · machine order ",
        "chemistry-analyzer<div class=\"cell-sub\">Chemistry analyzer XC-200</div>",
        "<td class=\"num right\">1</td>",
        "Outcome unknown: check the machine</span><div class=\"cell-sub cell-note\">Recorded: disconnected before confirming</div>",
        "Accepted by the machine</span>",
        "Done</span>",
        "Sending</span>",
        "Sent, not yet accepted</span>",
        "<a class=\"link\" href=\"/analyzer-cancelled\">Review</a>",
        "<div class=\"cell-sub cell-note\">The machine cancelled part of this order</div>",
    ] {
        assert!(all.contains(part), "missing {part}");
    }
    assert!(all.contains(&format!(
        "<a class=\"link\" href=\"/awos/{}/1\">Details</a>",
        scene.seeded.unknown
    )));
    let unknown = scene.get("/orders?show=unknown").await?;
    assert!(unknown.contains(">ORD-1021</span>"));
    assert!(!unknown.contains(">ORD-1047</span>"));
    assert!(unknown.contains("href=\"/orders?show=unknown\" aria-current=\"page\""));
    let cancelled = scene.get("/orders?show=cancelled").await?;
    assert!(cancelled.contains("<p class=\"empty__title\">No cancelled order</p>"));
    assert!(cancelled.contains("<a class=\"link\" href=\"/orders\">Show all orders</a>"));
    let past = scene.get("/orders?after=100000").await?;
    assert!(past.contains("<p class=\"empty__title\">No more orders</p>"));
    scene.stop().await?;
    let empty = Scene::start(false).await?;
    let none = empty.get("/orders").await?;
    assert!(none.contains("<p class=\"empty__title\">No orders yet</p>"));
    assert!(!none.contains("<table"));
    empty.stop().await
}

// ADR 0016; owner request 2026-11-20 (admin restyle b5): an order item's page
// keeps what the machine reported apart from the item's own state, shows the
// lab app's copy without its tests, and the machine's messages with the times
// as sent; IDs stay under "Details for support". Owner decision 2026-11-20
// (review B6): the machine's notes are free text, so they stay hidden here
// until the audited "Show the original message".
#[tokio::test(flavor = "multi_thread")]
async fn order_item_page_keeps_what_the_machine_did_apart() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let path = format!("/awos/{}/1", scene.seeded.cancelled);
    let item = scene.get(&path).await?;
    assert!(item.starts_with("HTTP/1.1 200"), "{item}");
    assert_private(&path, &item);
    for part in [
        "<ol class=\"crumbs\"><li><a href=\"/orders\">Machine orders</a></li><li>Order ORD-1018</li></ol>",
        "<h1>Order ORD-1018</h1>",
        "<dt>Machine</dt><dd><a class=\"link\" href=\"/machines/coagulation-analyzer\">coagulation-analyzer (Coagulation analyzer CG-7)</a></dd>",
        "<dt>Sample</dt><dd><span class=\"mono\" title=\"261119-018\">261119-018</span></dd>",
        "Cancelled by the machine</span><span class=\"kv__note\">Recorded ",
        "<dt>Its version</dt><dd>1: the same version that was sent</dd>",
        "<h3 class=\"card__title\">Message received ",
        "<dt>Recorded by Analyte</dt><dd><span class=\"mono\"><time",
        "<dt>Expected ready time, as sent</dt><dd>20261119103000</dd>",
        "<dt>Message time and zone</dt><dd>20261119095600+0100</dd>",
        "<dt>Event time, as sent</dt><dd>20261119095600</dd>",
        "<p class=\"muted text--sm\">1 note from the machine, hidden</p>",
        "Message MSG-1018 · receipt ",
    ] {
        assert!(item.contains(part), "missing {part}");
    }
    assert!(
        !shown(&item).contains(&uuid::Uuid::from(scene.seeded.held.ok_or("held")?).to_string())
    );
    assert!(!item.contains("Sample &#60;not&#62; found"), "{item}");
    // One message, so no next page.
    assert!(!item.contains("Next page"));
    let done = scene.get(&format!("/awos/{}/1", scene.seeded.done)).await?;
    assert!(done.contains("<p class=\"empty__title\">No message about this item</p>"));
    assert!(done.contains("<dt>What the machine reports</dt><dd>Nothing yet</dd>"));
    let missing = scene.get(&format!("/awos/{}/9", scene.seeded.done)).await?;
    assert!(missing.starts_with("HTTP/1.1 503"), "{missing}");
    assert!(missing.contains("AWOS item not retained"));
    scene.stop().await
}

// Owner request 2026-11-20 (admin restyle b5, MoreCancelled): each order item
// a machine cancelled is a card with its sample, machine and test count, what
// was recorded and the lab app's own status, under what to check; there is
// no Mark resolved, because Analyte has none for these.
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_by_machine_lists_each_item_with_what_to_check() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let page = scene.get("/analyzer-cancelled").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert_private("/analyzer-cancelled", &page);
    for part in [
        "<p class=\"alert__title\">Check each order below with the lab and the lab app</p>",
        "Sample <span class=\"mono\">261119-018</span> · coagulation-analyzer (Coagulation analyzer CG-7) · 1 test</p>",
        "Cancelled by the machine · recorded <time",
        "The lab app&#39;s copy says: requested",
    ] {
        assert!(page.contains(part), "missing {part}");
    }
    assert!(page.contains(&format!(
        "<a class=\"btn btn--outline btn--sm\" href=\"/awos/{}/1\">Details</a>",
        scene.seeded.cancelled
    )));
    assert!(!page.contains("Mark resolved"));
    assert_eq!(page.matches("<li class=\"card\">").count(), 1);
    scene.stop().await?;
    let empty = Scene::start(false).await?;
    let none = empty.get("/analyzer-cancelled").await?;
    assert!(none.contains("<p class=\"empty__title\">No order was cancelled by a machine</p>"));
    assert!(!none.contains("alert__title"));
    empty.stop().await
}

// Owner request 2026-11-20 (admin restyle b5, MoreDeliveries): each delivery
// shows when it was queued, its sample, from which machine to which lab app,
// its result count and its outcome in the existing delivery words; the Not
// delivered filter keeps every state but delivered; Send again shows only where
// Analyte allows it, and the delivery ID stays on hover.
#[tokio::test(flavor = "multi_thread")]
async fn deliveries_are_filtered_and_say_only_what_was_observed() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let all = scene.get("/deliveries").await?;
    assert!(all.starts_with("HTTP/1.1 200"), "{all}");
    assert_private("/deliveries", &all);
    for part in [
        "<a class=\"chip\" href=\"/deliveries\" aria-current=\"page\">All <span class=\"chip__n\">",
        "<a class=\"chip\" href=\"/deliveries?show=undelivered\">Not delivered <span class=\"chip__n\">",
        "<td class=\"num\"><span title=\"261119-044\">261119-044</span></td>",
        "<td class=\"more-nowrap\">hematology-analyzer</td><td class=\"more-nowrap\">main-lab</td><td class=\"num right\">6</td>",
        "<td class=\"subtle more-nowrap\">Order message</td>",
        "<td class=\"subtle\">Not recorded</td>",
        "Paused: lab app settings need repair</span>",
        "Rejected by the lab app</span>",
        "Delivered</span>",
        "Waiting to be sent</span>",
    ] {
        assert!(all.contains(part), "missing {part}");
    }
    let delivered = scene.seeded.delivered.ok_or("delivered")?;
    assert!(all.contains(&format!("<td class=\"right more-nowrap\"><a class=\"link\" href=\"/deliveries/{delivered}/resend\">Send again</a>")));
    assert!(all.contains(&format!("title=\"Delivery {delivered} · Done\"")));
    assert!(!shown(&all).contains(&delivered.to_string()));
    let waiting = scene.seeded.waiting.ok_or("waiting")?;
    assert!(!all.contains(&format!("/deliveries/{waiting}/resend")));
    let open = scene.get("/deliveries?show=undelivered").await?;
    assert!(!open.contains(">Delivered</span>"));
    assert!(open.contains("Rejected by the lab app</span>"));
    assert!(open.contains("href=\"/deliveries?show=undelivered\" aria-current=\"page\""));
    scene.stop().await?;
    let empty = Scene::start(false).await?;
    let none = empty.get("/deliveries").await?;
    assert!(none.contains("<p class=\"empty__title\">No deliveries yet</p>"));
    let calm = empty.get("/deliveries?show=undelivered").await?;
    assert!(calm.contains("<p class=\"empty__title\">Nothing waits for a lab app</p>"));
    empty.stop().await
}

// Safety rules 4 and 8; owner request 2026-11-20 (admin restyle b5): Send
// again is a dialog over Deliveries with a required reason, the superseded
// box and Cancel back to the list; it queues the same delivery ID again,
// never sends an order to a machine, and refuses what it may not resend.
#[tokio::test(flavor = "multi_thread")]
async fn send_again_keeps_the_delivery_id_and_never_resends_an_order() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let delivered = scene.seeded.event.ok_or("delivered order message")?;
    let path = format!("/deliveries/{delivered}/resend");
    let dialog = scene.get(&path).await?;
    assert!(dialog.starts_with("HTTP/1.1 200"), "{dialog}");
    for part in [
        "<div class=\"scrim\">",
        &format!(
            "<form class=\"dialog\" role=\"dialog\" aria-modal=\"true\" aria-labelledby=\"dlg-title\" method=\"post\" action=\"{path}\">"
        ),
        "<h2 id=\"dlg-title\" class=\"dialog__title\">Send this delivery again?</h2>",
        "Analyte sends it to lab app main-lab again, unchanged and with the same delivery number. Its status now: Delivered.",
        "<input type=\"checkbox\" name=\"acknowledge_superseded\" value=\"true\">",
        "<a class=\"btn btn--outline\" href=\"/deliveries\">Cancel</a><button type=\"submit\" class=\"btn btn--primary\">Send again</button>",
        &format!("Delivery {delivered}\n"),
    ] {
        assert!(dialog.contains(part), "missing {part}");
    }
    let keyed = scene.get_with_key(&path).await?;
    assert!(keyed.starts_with("HTTP/1.1 403"), "{keyed}");
    assert!(keyed.contains("<p class=\"alert__title\">Analyte records who sends again or changes a hold, so this needs a signed-in browser session</p>"));
    let unknown = scene
        .get(&format!("/deliveries/{}/resend", uuid::Uuid::now_v7()))
        .await?;
    assert!(unknown.starts_with("HTTP/1.1 409"), "{unknown}");
    assert!(unknown.contains("<p class=\"alert__title\">That could not be done</p>"));
    let token = scene.token().await?;
    let blank = scene
        .post(&path, &format!("csrf={token}&confirm=yes&reason=%20"))
        .await?;
    assert!(blank.starts_with("HTTP/1.1 400"), "{blank}");
    assert!(blank.contains("Write a reason. It is recorded in the history."));
    let orders_before = order_history(&scene.database())?;
    let sent = scene
        .post(
            &path,
            &format!("csrf={token}&confirm=yes&reason=lab%20asked&acknowledge_superseded=true"),
        )
        .await?;
    assert!(sent.starts_with("HTTP/1.1 200"), "{sent}");
    assert!(sent.contains("<p class=\"alert__title\">The delivery is queued to send again</p>"));
    assert!(
        sent.contains("<a class=\"btn btn--primary\" href=\"/deliveries\">Back to Deliveries</a>")
    );
    let id = analyte_model::DeliveryId::try_from(delivered)?;
    scene
        .store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            let row = s.delivery(id)?.ok_or(FixtureError::UnknownDelivery)?;
            assert_eq!(row.state, analyte_store::DeliveryState::Pending);
            let audits = s.audits(0)?;
            let last = &audits.last().ok_or(FixtureError::UnknownAudit)?.1;
            assert_eq!(last.action.as_str(), "delivery-resend");
            assert_eq!(last.reason.as_str(), "lab asked");
            Ok(())
        })
        .await?;
    assert_eq!(order_history(&scene.database())?, orders_before);
    let again = scene
        .post(&path, &format!("csrf={token}&confirm=yes&reason=twice"))
        .await?;
    assert!(again.starts_with("HTTP/1.1 409"), "{again}");
    assert!(again.contains("<p class=\"alert__title\">That could not be done</p>"));
    scene.stop().await
}

/// How many order changes the store holds, so a test sees that none was added.
fn order_history(database: &std::path::Path) -> Result<i64, Error> {
    Ok(rusqlite::Connection::open(database)?.query_row(
        "SELECT (SELECT count(*) FROM machine_order_history)+(SELECT count(*) FROM outbox WHERE kind='order_event')",
        [],
        |row| row.get(0),
    )?)
}

// ADR 0013 items 4 and 7; owner request 2026-11-20 (admin restyle b5,
// MoreDataKeeping): each machine's keeping time, the last clean-up and the
// oldest message kept, then each hold with who placed it, when and why, and
// Release as a dialog; signing in is needed to add or release.
#[tokio::test(flavor = "multi_thread")]
async fn data_keeping_shows_holds_with_who_placed_them() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let page = scene.get("/retention").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    for part in [
        "<h2 id=\"dk-times\" class=\"label\">Keeping times</h2>",
        "<dt>chemistry-analyzer</dt><dd>Deleted 365 days after the lab app last confirmed it</dd>",
        "<dt>hematology-analyzer</dt><dd>Kept, never deleted</dd>",
        "<dt>Messages checked</dt><dd>40</dd>",
        "<dt>Messages deleted</dt><dd>9</dd>",
        "<dt>Oldest message kept</dt><dd><span class=\"mono\"><time",
        "Message from hematology-analyzer · received <time",
        "Set by admin session · <time",
        "Reason: Audit of hematology results · until released",
        "Order ORD-1018 from lab app main-lab",
        "Set by operator · <time",
        "<a class=\"btn btn--outline btn--sm\" href=\"#dk-add\">Add a hold</a>",
        "<input type=\"hidden\" name=\"operation\" value=\"place\">",
    ] {
        assert!(page.contains(part), "missing {part}");
    }
    let release = page
        .split("href=\"/retention?release=")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or("Release link")?
        .to_owned();
    let dialog = scene.get(&format!("/retention?release={release}")).await?;
    let held = scene.seeded.held.ok_or("held")?;
    for part in [
        "<h2 id=\"dlg-title\" class=\"dialog__title\">Release this hold?</h2>",
        "<p class=\"dialog__text\">Message from hematology-analyzer · received ",
        "<input type=\"hidden\" name=\"scope\" value=\"receipt\">",
        &format!(
            "<input type=\"hidden\" name=\"id\" value=\"{held}\">",
            held = uuid::Uuid::from(held)
        ),
        "<input type=\"hidden\" name=\"operation\" value=\"release\">",
        "<a class=\"btn btn--outline\" href=\"/retention\">Cancel</a><button type=\"submit\" class=\"btn btn--primary\">Release</button>",
    ] {
        assert!(dialog.contains(part), "missing {part}");
    }
    // The full message ID stays in the form, never in the words.
    assert!(!shown(&dialog).contains(&uuid::Uuid::from(held).to_string()));
    let token = scene.token().await?;
    let released = scene
        .post(
            "/retention",
            &format!(
                "csrf={token}&confirm=yes&scope=receipt&id={}&destination=&operation=release&reason=audit%20done",
                uuid::Uuid::from(held)
            ),
        )
        .await?;
    assert!(released.starts_with("HTTP/1.1 200"), "{released}");
    assert!(released.contains("<p class=\"alert__title\">The hold is released</p>"));
    let after = scene.get("/retention").await?;
    assert!(!after.contains("Message from hematology-analyzer"));
    let keyed = scene.get_with_key("/retention").await?;
    assert!(keyed.contains("<p class=\"alert__title\">Sign in to add or release a hold</p>"));
    assert!(!keyed.contains("?release=") && !keyed.contains("id=\"dk-add\""));
    let ignored = scene
        .get_with_key(&format!("/retention?release={release}"))
        .await?;
    assert!(!ignored.contains("class=\"scrim\""));
    scene.stop().await?;
    let empty = Scene::start(false).await?;
    let none = empty.get("/retention").await?;
    for part in [
        "No clean-up has run yet.",
        "<dt>Oldest message kept</dt><dd><span class=\"subtle\">Nothing yet</span></dd>",
        "<p class=\"empty__title\">No holds</p>",
    ] {
        assert!(none.contains(part), "missing {part}");
    }
    empty.stop().await
}

// Owner decision 6 (2026-11-20): Settings shows real values only, from the
// loaded program and this installation, read-only; for each key it says where
// the key is found, never its value. Known keys are stored here, and neither
// they, the admin key nor an alert target's address may appear.
#[tokio::test(flavor = "multi_thread")]
async fn settings_show_real_values_and_never_a_key() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    let page = scene.get("/settings").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(!page.contains(SECRET));
    assert!(!page.contains(&scene.bearer));
    assert!(!page.contains("hospital.internal"));
    let root = scene.directory.path().display().to_string();
    for part in [
        "<h1>Settings</h1>",
        "<h2 id=\"st-engine\" class=\"card__title\">Engine</h2><span class=\"status status--good\">",
        &format!(
            "<dt>Analyte version</dt><dd><span class=\"mono\" title=\"{v}\">{v}</span></dd>",
            v = env!("CARGO_PKG_VERSION")
        ),
        "<dt>Machine links open</dt><dd>0 of 4</dd>",
        &format!(
            "<dt>Data folder</dt><dd><span class=\"mono\" title=\"{root}\">{root}</span></dd>"
        ),
        "<dt>Disk space kept free</dt><dd><span class=\"mono\" title=\"1 byte\">1 byte</span></dd>",
        "<dt>Page reads</dt><dd><span class=\"mono\" title=\"1500 ms\">1.5 s</span></dd>",
        "<dt>Page requests</dt><dd><span class=\"mono\" title=\"1750 ms\">1.75 s</span></dd>",
        "<dt>Setup changes</dt><dd><span class=\"mono\" title=\"10000 ms\">10 s</span></dd>",
        "<dt>Alert targets</dt><dd>lab-alerts</dd>",
        "<dt>Admin pages · Admin key</dt><dd><span class=\"status status--good\" title=\"",
        "In the file admin-token in the data folder</span>",
        "<dt>Lab app main-lab · API key</dt><dd><span class=\"status status--good\" title=\"main-lab-api, stored ",
        "<dt>Lab app outpatient-lab · Signing key</dt><dd><span class=\"status status--bad\" title=\"outpatient-lab-signing\">",
        "Missing</span>",
        "<dt>Alert target lab-alerts · Signing key</dt><dd><span class=\"status status--good\" title=\"alerts-signing, stored ",
    ] {
        assert!(page.contains(part), "missing {part}");
    }
    assert!(page.contains(
        "<a href=\"/settings\" aria-current=\"page\"><span class=\"nav__label\">Settings</span></a>"
    ));
    assert!(page.contains("<details class=\"nav-group nav-more\" open>"));
    std::fs::write(scene.directory.path().join("lab-apps.toml"), "not toml [")?;
    let unreadable = scene.get("/settings").await?;
    assert!(unreadable.starts_with("HTTP/1.1 200"), "{unreadable}");
    assert!(unreadable.contains("<p class=\"alert__title\">The lab apps’ settings could not be read, so their keys are not listed</p>"));
    assert!(unreadable.contains("Alert target lab-alerts · Signing key"));
    assert!(!unreadable.contains(SECRET));
    scene.stop().await
}

// ADR 0039: use the browser's preferred supported language until the user
// saves another choice. The picker shows the language in its own wording.
#[tokio::test(flavor = "multi_thread")]
async fn settings_render_in_albanian_from_browser_or_saved_choice() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    let host = &scene.started.3;
    let preferred = format!("{}Accept-Language: en;q=0.4, sq-XK;q=0.9\r\n", scene.auth);
    let page = request(host, "/settings", &preferred, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("<html lang=\"sq\""));
    assert!(page.contains("<h1>Cilësimet</h1>"));
    assert!(page.contains("<option value=\"sq\" lang=\"sq\" selected>Shqip</option>"));

    let form = format!("csrf={}&language=sq", encode(&csrf(&page)?));
    let saved = scene.post("/settings/language", &form).await?;
    assert!(saved.starts_with("HTTP/1.1 303"), "{saved}");
    let language_cookie = pages::cookie(&saved)?;
    assert_eq!(language_cookie, "analyte_language=sq");
    let cookie = scene.auth.trim_end();
    let headers = format!("{cookie}; {language_cookie}\r\nAccept-Language: en\r\n");
    let page = request(host, "/settings", &headers, None).await?;
    assert!(page.contains("<h1>Cilësimet</h1>"));
    scene.stop().await
}

// ADR 0039: a French browser preference renders the page in French, and the
// picker names French in its own wording.
#[tokio::test(flavor = "multi_thread")]
async fn settings_render_in_french_from_browser_choice() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    let host = &scene.started.3;
    let preferred = format!("{}Accept-Language: fr-CH, en;q=0.5\r\n", scene.auth);
    let page = request(host, "/settings", &preferred, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("<html lang=\"fr\""));
    assert!(page.contains("<h1>Paramètres</h1>"));
    assert!(page.contains("<option value=\"fr\" lang=\"fr\" selected>Français</option>"));
    scene.stop().await
}

// Owner request 2026-11-20 (admin restyle b5, Sidebar mockup): every page's
// More group lists Machine orders, Cancelled by machine, Deliveries, Data
// keeping and Settings, opened and marked on the page that is one of them.
#[tokio::test(flavor = "multi_thread")]
async fn more_group_lists_five_pages_and_marks_the_current_one() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    let more = [
        ("/orders", "Machine orders"),
        ("/analyzer-cancelled", "Cancelled by machine"),
        ("/deliveries", "Deliveries"),
        ("/retention", "Data keeping"),
        ("/settings", "Settings"),
    ];
    for (path, _) in more {
        let page = scene.get(path).await?;
        assert!(page.starts_with("HTTP/1.1 200"), "{path}: {page}");
        let group = page
            .split("<ul class=\"nav nav--sub\">")
            .nth(1)
            .and_then(|rest| rest.split("</ul>").next())
            .ok_or("More group")?;
        for (href, label) in more {
            let current = if href == path {
                " aria-current=\"page\""
            } else {
                ""
            };
            assert!(
                group.contains(&format!(
                    "<a href=\"{href}\"{current}><span class=\"nav__label\">{label}</span></a>"
                )),
                "{path}: {label}"
            );
        }
        assert!(
            page.contains("<details class=\"nav-group nav-more\" open>"),
            "{path}"
        );
    }
    scene.stop().await
}

// Owner request 2026-11-20 (admin restyle b5): when the store cannot be read,
// each More page says so in the new look with the reason kept for support.
#[tokio::test(flavor = "multi_thread")]
async fn more_pages_say_when_the_store_cannot_be_read() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    rusqlite::Connection::open(scene.database())?.execute_batch(
        "ALTER TABLE outbox RENAME TO outbox_away; ALTER TABLE machine_orders RENAME TO orders_away; ALTER TABLE retention_holds RENAME TO holds_away;",
    )?;
    for path in ["/orders", "/deliveries", "/retention"] {
        let page = scene.get(path).await?;
        assert!(page.starts_with("HTTP/1.1 503"), "{path}: {page}");
        assert!(
            page.contains(
                "<p class=\"alert__title\">Analyte could not read its stored data just now</p>"
            ),
            "{path}"
        );
        assert!(page.contains("Details for support"), "{path}");
    }
    rusqlite::Connection::open(scene.database())?.execute_batch(
        "ALTER TABLE outbox_away RENAME TO outbox; ALTER TABLE orders_away RENAME TO machine_orders; ALTER TABLE holds_away RENAME TO retention_holds;",
    )?;
    scene.stop().await
}
