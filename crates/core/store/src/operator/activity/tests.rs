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

/// Every line as schema 16 read it: all sources combined, then sorted. Kept
/// here as the reference the indexed reads must match line for line.
macro_rules! combined {
    () => {
        concat!(
            "WITH parked_machine(id, machine) AS (SELECT p.id, coalesce(",
            "(SELECT coalesce(",
            "(SELECT json_extract(r.metadata,'$.machine') FROM receipts r WHERE r.id=json_quote(t.value)),",
            "(SELECT json_extract(r.metadata,'$.machine') FROM outbox o JOIN receipts r ON r.id=o.receipt WHERE o.id=json_quote(t.value)),",
            "(SELECT m.machine FROM machine_orders m WHERE m.id=CAST(t.value AS INTEGER) AND CAST(m.id AS TEXT)=t.value),",
            "(SELECT json_extract(f.evidence,'$.machine') FROM frame_provenance f WHERE f.transfer=t.value)",
            ") FROM json_each(p.evidence,'$.targets') t ORDER BY t.key LIMIT 1),",
            "(SELECT i.machine FROM poll_intents i WHERE i.parked=p.id)) FROM parked p),",
            "events(at, source, key, machine, grp) AS (",
            "SELECT unixepoch(json_extract(metadata,'$.received_at')),0,rowid,json_extract(metadata,'$.machine'),1 FROM receipts",
            " UNION ALL SELECT p.parked_at,1,p.id,m.machine,3 FROM parked p JOIN parked_machine m ON m.id=p.id",
            " UNION ALL SELECT json_extract(p.resolution,'$.time'),2,p.id,m.machine,4 FROM parked p JOIN parked_machine m ON m.id=p.id WHERE p.resolution IS NOT NULL",
            " UNION ALL SELECT a.time,3,a.sequence,coalesce(",
            "(SELECT json_extract(r.metadata,'$.machine') FROM receipts r WHERE r.id=o.receipt),",
            "(SELECT CASE WHEN count(DISTINCT m.machine)=1 THEN min(m.machine) END FROM machine_orders m WHERE m.order_id=o.order_id AND m.destination=o.destination)),",
            "CASE WHEN a.outcome='\"Done\"' THEN 1 ELSE 3 END FROM attempts a JOIN outbox o ON o.id=a.delivery",
            " UNION ALL SELECT json_extract(evidence,'$.updated_at'),4,sequence,json_extract(evidence,'$.key.machine'),2 FROM (",
            "SELECT sequence,evidence,json_extract(evidence,'$.state') AS state,",
            "lag(json_extract(evidence,'$.state')) OVER (PARTITION BY machine_order ORDER BY sequence) AS was ",
            "FROM machine_order_history) WHERE was IS NULL OR was!=state",
            " UNION ALL SELECT json_extract(a.evidence,'$.time'),5,a.sequence,",
            "CASE WHEN json_extract(a.evidence,'$.action') LIKE 'setup-%' THEN json_extract(a.evidence,'$.targets[0]') END,4 ",
            "FROM audit a WHERE json_extract(a.evidence,'$.action') NOT IN ('admin-raw-view','deliveries-list','orders-list') ",
            "AND NOT EXISTS(SELECT 1 FROM parked p WHERE p.resolution=a.evidence)",
            ") "
        )
    };
}

/// The number the combined read gave each group.
const fn group_number(group: ActivityGroup) -> i64 {
    match group {
        ActivityGroup::Results => 1,
        ActivityGroup::Orders => 2,
        ActivityGroup::Problems => 3,
        ActivityGroup::Changes => 4,
    }
}

/// A line's position, time and machine.
type Position = (ActivityCursor, Option<i64>, Option<String>);

fn positions(lines: &[ActivityLine]) -> Vec<Position> {
    lines
        .iter()
        .map(|line| (line.cursor, line.at, line.machine.clone()))
        .collect()
}

/// A page as the combined read gave it.
fn combined_page(
    store: &Store,
    machine: Option<&str>,
    group: Option<ActivityGroup>,
    before: ActivityCursor,
    limit: i64,
) -> Result<Vec<Position>, rusqlite::Error> {
    store
        .db
        .prepare(concat!(
            combined!(),
            "SELECT coalesce(at,-9223372036854775808) AS sort,source,key,at,machine FROM events ",
            "WHERE (?1 IS NULL OR machine=?1) AND (?2 IS NULL OR grp=?2) AND (coalesce(at,-9223372036854775808),source,key)<(?3,?4,?5) ORDER BY sort DESC,source DESC,key DESC LIMIT ?6"
        ))?
        .query_map(
            params![
                machine,
                group.map(group_number),
                before.at,
                before.source,
                before.key,
                limit
            ],
            |r| {
                Ok((
                    ActivityCursor {
                        at: r.get(0)?,
                        source: r.get(1)?,
                        key: r.get(2)?,
                    },
                    r.get(3)?,
                    r.get(4)?,
                ))
            },
        )?
        .collect()
}

/// A day's counts as the combined read gave them, in [`ActivityGroup::ALL`]'s order.
fn combined_counts(
    store: &Store,
    machine: Option<&str>,
    day: (i64, i64),
) -> Result<Vec<(ActivityGroup, u64)>, Box<dyn std::error::Error>> {
    let counted: Vec<(i64, i64)> = store
        .db
        .prepare(concat!(
            combined!(),
            "SELECT grp,count(*) FROM events WHERE (?1 IS NULL OR machine=?1) AND at>=?2 AND at<?3 GROUP BY grp"
        ))?
        .query_map(params![machine, day.0, day.1], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(ActivityGroup::ALL
        .into_iter()
        .map(|group| {
            let count = counted
                .iter()
                .find(|(number, _)| *number == group_number(group))
                .map_or(0, |&(_, count)| count);
            u64::try_from(count).map(|count| (group, count))
        })
        .collect::<Result<_, _>>()?)
}

// Owner decision 2026-11-20 (admin read speed, item 5): Activity reads only
// the newest rows each table needs, and shows exactly what the combined read
// of every kept row showed: the same lines, order, times, machines, page
// breaks and day counts, for every machine and filter, including lines in the
// same second, items with no stored time, a resolution whose audit names a
// poll, an order sent to two machines and a purged message.
#[test]
fn activity_pages_and_counts_match_the_combined_read() -> TestResult {
    let temp = tempfile::tempdir()?;
    let store = crate::test_support::mixed_store(&temp.path().join("mixed.db"))?;
    let everything = combined_page(&store, None, None, ActivityCursor::NEWEST, 100)?;
    assert!(everything.len() > 30, "the fixture shows many lines");
    assert!(
        everything.iter().filter(|line| line.1.is_none()).count() >= 3,
        "and several with no stored time"
    );
    let groups = std::iter::once(None).chain(ActivityGroup::ALL.into_iter().map(Some));
    for machine in [
        None,
        Some("m-a"),
        Some("m-b"),
        Some("m-quiet"),
        Some("no-such-machine"),
    ] {
        for group in groups.clone() {
            for limit in [1, 2, 3, 7, 100] {
                let mut before = ActivityCursor::NEWEST;
                let mut shown = 0;
                loop {
                    let page = store.activity(machine, group, before, limit)?;
                    assert_eq!(
                        positions(&page),
                        combined_page(&store, machine, group, before, i64::try_from(limit)?)?,
                        "{machine:?} {group:?} {limit} after {before:?}"
                    );
                    let Some(last) = page.last() else { break };
                    before = last.cursor;
                    shown += page.len();
                    assert!(shown <= everything.len(), "paging ends");
                }
            }
        }
        for day in [
            (9_000, 10_100),
            (10_000, 10_001),
            (10_050, 10_051),
            (10_130, 10_141),
            (10_500, 10_501),
            (i64::MIN, i64::MAX),
        ] {
            assert_eq!(
                store.activity_counts(machine, day)?,
                combined_counts(&store, machine, day)?,
                "{machine:?} {day:?}"
            );
        }
    }
    Ok(())
}

// Owner decision 2026-11-20 (PR c2): the live pages learn that something was
// stored from marks that move with every stored row they show.
#[test]
fn activity_marks_move_with_every_stored_kind() -> TestResult {
    let temp = tempfile::tempdir()?;
    let empty = open(&temp.path().join("empty.db"))?;
    assert_eq!(empty.activity_marks()?, ActivityMarks::default());
    let store = seeded(&temp.path().join("marks.db"))?;
    let marks = store.activity_marks()?;
    assert_eq!(
        marks,
        ActivityMarks {
            receipts: 2,
            readings: 1,
            parked: 2,
            open: 1,
            deliveries: 1,
            attempts: 2,
            orders: 3,
            audit: 4,
        }
    );
    store
        .db
        .execute("UPDATE parked SET resolution='{}' WHERE id=2", [])?;
    assert_eq!(
        store.activity_marks()?.open,
        0,
        "a resolved item moves the open count"
    );
    Ok(())
}
