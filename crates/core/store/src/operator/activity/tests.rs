//! Tests the Activity feed over a store seeded with every kind of line.
use super::*;
use crate::test_support::{TestResult, interpretation, open, production_result, receipt};
use crate::{Reading, document, key};
use analyte_model::{DeliveryId, SampleId};
use rusqlite::params;

/// A store with one line of each kind at fixed times, from `synthetic-machine`
/// unless said otherwise, and one parked item from before schema 16.
fn seeded(path: &std::path::Path) -> Result<Store, Box<dyn std::error::Error>> {
    let mut store = open(path)?;
    let mut metadata = receipt(b"raw")?;
    metadata.received_at = jiff::Timestamp::from_second(1000)?;
    store.receive(&metadata, b"raw", b"config")?;
    let mut evidence = interpretation()?;
    evidence.outcome = Reading::Results(vec![production_result(metadata.id)?]);
    store.db.execute(
        "INSERT INTO interpretations(receipt,version,evidence) VALUES (?1,1,?2)",
        params![key(&metadata.id)?, document(&evidence)?],
    )?;
    let delivery = DeliveryId::try_from(uuid::Uuid::now_v7())?;
    store.db.execute(
        "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt) VALUES (?1,'result',?2,1,'main-lab','k',x'00',x'00',1,'done',0)",
        params![key(&delivery)?, key(&metadata.id)?],
    )?;
    store.db.execute(
        "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,'{\"RetryAt\":1150}',1100),(?1,'\"Done\"',1200)",
        [key(&delivery)?],
    )?;
    let target = uuid::Uuid::from(metadata.id).to_string();
    let parked = |kind: &str| {
        format!(
            "{{\"kind\":\"{kind}\",\"reason\":\"synthetic\",\"targets\":[\"{target}\"],\"context\":null}}"
        )
    };
    let resolution = r#"{"actor":"admin-session-1","action":"parked-resolve","targets":["1"],"time":1400,"reason":"checked"}"#;
    store.db.execute(
        "INSERT INTO parked(evidence,resolution,parked_at) VALUES (?1,?2,1300)",
        params![parked("mapping_finding"), resolution],
    )?;
    store
        .db
        .execute("INSERT INTO audit(evidence) VALUES (?1)", [resolution])?;
    // An item parked before schema 16 has no time.
    store.db.execute_batch("DROP TRIGGER parked_time")?;
    store.db.execute(
        "INSERT INTO parked(evidence) VALUES (?1)",
        [parked("interpretation_failure")],
    )?;
    store.db.execute_batch(concat!(
        "INSERT INTO feed(destination,cursor,complete,trusted,polled) VALUES ('main-lab','c',1,1,0);",
        "INSERT INTO orders VALUES ('main-lab','order-1','000777','{}',0,0,0);",
        "INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES ('main-lab','order-1','synthetic-machine','1','Accepted',1500,1600,'x');",
    ))?;
    for (state, at) in [("Sent", 1500), ("Sent", 1550), ("Accepted", 1600)] {
        store.db.execute(
            "INSERT INTO machine_order_history(machine_order,evidence) VALUES (1,?1)",
            [format!(
                "{{\"id\":1,\"key\":{{\"destination\":\"main-lab\",\"order\":\"order-1\",\"machine\":\"synthetic-machine\"}},\"version\":1,\"state\":\"{state}\",\"created_at\":1500,\"updated_at\":{at},\"source_code\":\"x\",\"items\":[{{}},{{}}]}}"
            )],
        )?;
    }
    for (action, target, at) in [
        ("setup-save", "synthetic-machine", 1700),
        ("lab-app-save", "main-lab", 1800),
        ("admin-raw-view", "receipt:x", 1900),
    ] {
        store.db.execute(
            "INSERT INTO audit(evidence) VALUES (?1)",
            [format!(
                "{{\"actor\":\"admin-session-1\",\"action\":\"{action}\",\"targets\":[\"{target}\"],\"time\":{at},\"reason\":\"private words\"}}"
            )],
        )?;
    }
    let mut other = receipt(b"other")?;
    other.machine = analyte_model::Identifier::new("other-machine")?;
    other.received_at = jiff::Timestamp::from_second(2000)?;
    store.receive(&other, b"other", b"config")?;
    Ok(store)
}

fn times(lines: &[ActivityLine]) -> Vec<Option<i64>> {
    lines.iter().map(|line| line.at).collect()
}

// Owner decision 2026-11-20 (PR c): Activity is built from stored tables,
// newest first; a line without a stored time comes last and keeps none.
#[test]
fn activity_lists_stored_events_newest_first() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = seeded(&temp.path().join("activity.db"))?;
    let page = store.activity(None, None, ActivityCursor::NEWEST, 50)?;
    assert_eq!(
        times(&page),
        [
            Some(2000),
            Some(1800),
            Some(1700),
            Some(1600),
            Some(1500),
            Some(1400),
            Some(1300),
            Some(1200),
            Some(1100),
            Some(1000),
            None
        ],
        "the repeated Sent snapshot, the raw view and the resolution's own audit are not lines"
    );
    let what: Vec<&Activity> = page.iter().map(|line| &line.what).collect();
    assert_eq!(what.first(), Some(&&Activity::Stored(None)));
    assert_eq!(
        page.first().and_then(|line| line.machine.as_deref()),
        Some("other-machine")
    );
    assert!(matches!(
        what.get(1),
        Some(Activity::Changed { action, target, .. }) if action == "lab-app-save" && target.as_deref() == Some("main-lab")
    ));
    assert_eq!(
        what.get(3),
        Some(&&Activity::Order {
            order: "order-1".into(),
            sample: Some("000777".into()),
            tests: Some(2),
            state: OrderState::Accepted,
        })
    );
    assert_eq!(
        what.get(5),
        Some(&&Activity::Resolved {
            item: 1,
            action: "parked-resolve".into()
        })
    );
    assert!(matches!(
        what.get(6),
        Some(Activity::Parked { item: 1, details }) if details.kind.as_str() == "mapping_finding"
    ));
    let results = Some(RecentOutcome::Results {
        results: 1,
        samples: vec![SampleId::new("000123")?],
    });
    assert_eq!(
        what.get(7),
        Some(&&Activity::Attempt {
            lab_app: "main-lab".into(),
            outcome: AttemptOutcome::Delivered,
            carries: Carries::Results(results.clone()),
        })
    );
    assert!(matches!(
        what.get(8),
        Some(Activity::Attempt {
            outcome: AttemptOutcome::RetryAt(1150),
            ..
        })
    ));
    assert_eq!(what.get(9), Some(&&Activity::Stored(results)));
    assert_eq!(page.last().map(|line| line.cursor.at), Some(i64::MIN));
    // Every line but the lab app's change and the other machine's message
    // names the machine, the undated item through its receipt.
    assert!(
        page.iter()
            .skip(2)
            .all(|line| line.machine.as_deref() == Some("synthetic-machine"))
    );
    Ok(())
}

// Owner decision 2026-11-20 (PR c): the mockup's machine and type filters,
// bounded pages that continue after the last line shown, and today's counts.
#[test]
fn activity_filters_pages_and_counts_a_day() -> TestResult {
    let temp = tempfile::tempdir()?;
    let mut store = seeded(&temp.path().join("filtered.db"))?;
    let machine = store.activity(Some("synthetic-machine"), None, ActivityCursor::NEWEST, 50)?;
    assert_eq!(machine.len(), 9);
    let problems = store.activity(
        None,
        Some(ActivityGroup::Problems),
        ActivityCursor::NEWEST,
        50,
    )?;
    assert_eq!(times(&problems), [Some(1300), Some(1100), None]);
    let changes = store.activity(
        None,
        Some(ActivityGroup::Changes),
        ActivityCursor::NEWEST,
        50,
    )?;
    assert_eq!(times(&changes), [Some(1800), Some(1700), Some(1400)]);
    assert!(
        store
            .activity(Some("no-such-machine"), None, ActivityCursor::NEWEST, 50)?
            .is_empty()
    );
    let first = store.activity(None, None, ActivityCursor::NEWEST, 4)?;
    assert_eq!(
        times(&first),
        [Some(2000), Some(1800), Some(1700), Some(1600)]
    );
    let last = first.last().ok_or("a fourth line")?.cursor;
    let next = store.activity(None, None, last, 4)?;
    assert_eq!(
        times(&next),
        [Some(1500), Some(1400), Some(1300), Some(1200)]
    );
    let end = store.activity(
        None,
        None,
        ActivityCursor {
            at: i64::MIN,
            source: 1,
            key: 2,
        },
        4,
    )?;
    assert!(end.is_empty(), "nothing comes after the last line");
    assert_eq!(
        store.activity_counts(None, (1000, 1650))?,
        [
            (ActivityGroup::Results, 2),
            (ActivityGroup::Orders, 2),
            (ActivityGroup::Problems, 2),
            (ActivityGroup::Changes, 1),
        ]
    );
    assert_eq!(
        store
            .activity_counts(Some("other-machine"), (1000, 2001))?
            .first(),
        Some(&(ActivityGroup::Results, 1))
    );
    store.limits.page_rows = 2;
    assert_eq!(
        store
            .activity(None, None, ActivityCursor::NEWEST, 50)?
            .len(),
        2,
        "a page never holds more than the store's page bound"
    );
    Ok(())
}
