//! Tests Activity (PR c) through the real handlers over the More pages' scene: the lines built from stored tables, the filters, paging, every empty state and the store's failures.
use super::machines::{AT, CODE, VALUE};
use super::more::Scene;
use super::*;

/// Adds to the scene what only Activity shows: delivery tries in each
/// outcome, a parked item with its time, one parked before schema 16, and
/// one resolved with its audit.
fn seed_activity(scene: &Scene) -> Result<(), Error> {
    let db = rusqlite::Connection::open(scene.database())?;
    let delivery = |state: &str| -> Result<String, Error> {
        Ok(db.query_row(
            "SELECT id FROM outbox WHERE kind='result' AND state=?1 ORDER BY sequence LIMIT 1",
            [state],
            |row| row.get(0),
        )?)
    };
    let (done, parked, paused) = (delivery("done")?, delivery("parked")?, delivery("paused")?);
    for (id, outcome, at) in [
        (&done, "{\"RetryAt\":1}", AT - 2_990),
        (&done, "\"Done\"", AT - 2_980),
        (&parked, "{\"Parked\":\"HTTP 422\"}", AT - 1_790),
        (&paused, "{\"Paused\":\"keys missing\"}", AT - 3_590),
    ] {
        db.execute(
            "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
            rusqlite::params![id, outcome, at],
        )?;
    }
    let receipt: String = db.query_row(
        "SELECT json_extract(metadata,'$.id') FROM receipts WHERE json_extract(metadata,'$.machine')='chemistry-analyzer'",
        [],
        |row| row.get(0),
    )?;
    let finding = format!(
        "{{\"kind\":\"mapping_finding\",\"reason\":\"{CODE}\",\"targets\":[\"{receipt}\"],\"context\":null}}"
    );
    let resolution = format!(
        "{{\"actor\":\"admin-session-1\",\"action\":\"parked-resolve\",\"targets\":[\"1\"],\"time\":{},\"reason\":\"checked {VALUE}\"}}",
        AT - 600
    );
    db.execute(
        "INSERT INTO parked(evidence,parked_at,resolution) VALUES (?1,?2,?3)",
        rusqlite::params![finding, AT - 1_700, resolution],
    )?;
    db.execute("INSERT INTO audit(evidence) VALUES (?1)", [&resolution])?;
    // An item kept from before schema 16 has no parking time.
    db.execute_batch("DROP TRIGGER parked_time")?;
    db.execute(
        "INSERT INTO parked(evidence) VALUES (?1)",
        [finding.replace("mapping_finding", "interpretation_failure")],
    )?;
    Ok(())
}

/// The visible words of the feed's lines, one per line.
fn lines(page: &str) -> Vec<&str> {
    page.split("<li class=\"feed__line\" data-key=\"")
        .skip(1)
        .map(|line| line.split("</li>").next().unwrap_or_default())
        .map(|line| line.split_once("\">").map_or(line, |(_, rest)| rest))
        .collect()
}

// Owner decisions 2026-11-20 (PR c): Activity is built from stored tables,
// newest first, says only what Analyte observed, names machines by their ID
// with the model beside it, and never shows test codes, values or typed reasons.
#[tokio::test(flavor = "multi_thread")]
async fn activity_lists_what_was_stored_and_never_test_codes_or_values() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    seed_activity(&scene)?;
    let page = scene.get("/activity").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("<title>Activity · Analyte</title>"));
    assert!(page.contains("<a href=\"/activity\" aria-current=\"page\">"));
    assert!(page.contains("<p class=\"eyebrow\">Newest at the top</p>"));
    assert!(page.contains("Patient names and result values never appear here."));
    for line in [
        "<b>hematology-analyzer</b> <span class=\"muted\">(Hematology analyzer HM-5) – sample 261119-044 – 6 results – message stored</span>",
        "<b>immunoassay-analyzer</b> <span class=\"muted\">(Immunoassay analyzer IA-900) – sample 261119-031 – 8 results – delivered to main-lab</span>",
        "– sample 261119-031 – 8 results – sending to main-lab failed, will retry",
        "– sample 261119-038 – 12 results – rejected by main-lab</span></span><a class=\"link\" href=\"/deliveries?show=undelivered\">Deliveries</a>",
        "– sample 261119-044 – 6 results – sending to main-lab paused: lab app settings need repair</span></span><a class=\"link\" href=\"/setup/lab-apps/main-lab\">Lab app</a>",
        "<b>hematology-analyzer</b> <span class=\"muted\">(Hematology analyzer HM-5) – order ORD-1040 – sample 261119-040 – 1 test – done</span>",
        "– order ORD-1018 – sample 261119-018 – 1 test – accepted by the machine",
        "– order ORD-1033 – sample 261119-033 – 1 test – sent, not yet accepted",
        "– order ORD-1021 – sample 261119-021 – 1 test – outcome unknown: check the machine</span></span><a class=\"link\" href=\"/orders\">Orders</a>",
        "<b>Needs review</b> <span class=\"muted\">– item 1 added: test or unit not mapped – from chemistry-analyzer (Chemistry analyzer XC-200)</span></span><a class=\"link\" href=\"/parked/1\">Open</a>",
        "<b>Needs review</b> <span class=\"muted\">– item 1 marked resolved – from chemistry-analyzer (Chemistry analyzer XC-200)</span>",
        "<b>admin session</b> <span class=\"muted\">– placed a hold under Data keeping</span></span><a class=\"link\" href=\"/retention\">Data keeping</a>",
        "<b>operator</b> <span class=\"muted\">– placed a hold under Data keeping</span>",
        "<b>Analyte</b> <span class=\"muted\">– removed data past its keeping time</span>",
        "– order ORD-1047 – sample 261119-047 – 1 test – sending",
    ] {
        assert!(page.contains(line), "missing {line}");
    }
    // Each line carries its tone's icon with a word for screen readers.
    assert!(page.contains(
        "class=\"icon tone-good\" viewBox=\"0 0 24 24\" role=\"img\" aria-label=\"Delivered\""
    ));
    assert!(page.contains(
        "class=\"icon tone-warn\" viewBox=\"0 0 24 24\" role=\"img\" aria-label=\"Will retry\""
    ));
    // Newest first: the resolution (10 minutes before the scene's time) comes
    // before the oldest message, and the undated item comes last, under its
    // own divider.
    let shown = lines(&page);
    let position = |words: &str| shown.iter().position(|line| line.contains(words));
    assert!(position("marked resolved") < position("261119-044 – 6 results – message stored"));
    let undated = shown.last().ok_or("an undated line")?;
    assert!(
        undated.contains("item 2 added: could not read message"),
        "{undated}"
    );
    assert!(undated.contains("<span class=\"feed__time\" aria-hidden=\"true\">—</span>"));
    assert!(page.contains("<p class=\"feed__divider activity__divider\">Time not recorded</p>"));
    assert!(page.contains("<time class=\"feed__time\" datetime=\""));
    // Today's counts and the type filters, All pressed.
    assert!(page.contains("<span class=\"subtle activity__today\">Today · <span class=\"mono\">"));
    assert!(page.contains(
        "<button type=\"submit\" class=\"chip\" name=\"show\" value=\"all\" aria-pressed=\"true\">All</button>"
    ));
    assert!(page.contains(
        "name=\"show\" value=\"problems\" aria-pressed=\"false\">Problems <span class=\"chip__n\">"
    ));
    assert!(page.contains("<option value=\"chemistry-analyzer\">chemistry-analyzer (Chemistry analyzer XC-200)</option>"));
    for private in [
        CODE,
        VALUE,
        "Audit of hematology results",
        "Complaint follow-up",
        "HTTP 422",
        "keys missing",
    ] {
        assert!(!page.contains(private), "the page shows {private}");
    }
    assert!(!other_script(&page));
    assert!(
        !page.contains("Show earlier activity"),
        "one page holds the scene"
    );
    scene.stop().await
}

// Owner decisions 2026-11-20 (PR c): the mockup's machine and type filters
// work without a script, pages are bounded and continue after the last line,
// and every empty page says why.
#[tokio::test(flavor = "multi_thread")]
async fn activity_filters_pages_and_says_when_empty() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    seed_activity(&scene)?;
    let problems = scene.get("/activity?machine=&show=problems").await?;
    assert!(problems.contains("value=\"problems\" aria-pressed=\"true\""));
    assert!(problems.contains("name=\"show\" value=\"problems\">Show</button>"));
    assert!(problems.contains("rejected by main-lab"));
    assert!(problems.contains("item 1 added"));
    assert!(!problems.contains("message stored"));
    assert!(!problems.contains("delivered to main-lab"));
    let chemistry = scene.get("/activity?machine=chemistry-analyzer").await?;
    assert!(chemistry.contains("<option value=\"chemistry-analyzer\" selected>"));
    assert!(chemistry.contains("261119-038 – 12 results – message stored"));
    assert!(chemistry.contains("item 1 added"));
    assert!(!chemistry.contains("<b>hematology-analyzer</b>"));
    assert!(
        !chemistry.contains("placed a hold"),
        "a hold names no machine"
    );
    let gone = scene.get("/activity?machine=removed-analyzer").await?;
    assert!(gone.contains("<option value=\"removed-analyzer\" selected>removed-analyzer</option>"));
    assert!(gone.contains("<p class=\"empty__title\">Nothing for this filter</p>"));
    assert!(gone.contains("<a class=\"link\" href=\"/activity\">Show all activity</a>"));
    // More lines than one page: the first page links to the rest, which ends.
    let db = rusqlite::Connection::open(scene.database())?;
    for step in 0..60 {
        db.execute(
            "INSERT INTO audit(evidence) VALUES (?1)",
            [format!(
                "{{\"actor\":\"admin-session-1\",\"action\":\"setup-save\",\"targets\":[\"chemistry-analyzer\"],\"time\":{},\"reason\":\"saved\"}}",
                AT + step
            )],
        )?;
    }
    drop(db);
    let first = scene.get("/activity?show=changes").await?;
    assert_eq!(lines(&first).len(), 50);
    let href = first
        .split("<a class=\"btn btn--quiet btn--sm\" rel=\"next\" href=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or("Show earlier activity")?
        .replace("&#38;", "&");
    assert!(href.starts_with("/activity?show=changes&before="), "{href}");
    let rest = scene.get(&href).await?;
    assert!(rest.starts_with("HTTP/1.1 200"), "{rest}");
    assert!(!rest.contains("Show earlier activity"));
    assert!(
        lines(&rest)
            .iter()
            .any(|line| line.contains("marked resolved"))
    );
    assert!(first.contains("– saved the settings of chemistry-analyzer</span></span><a class=\"link\" href=\"/machines/chemistry-analyzer\">Machine</a>"));
    let past = scene
        .get("/activity?before=-9223372036854775808.0.0")
        .await?;
    assert!(past.contains("<p class=\"empty__title\">No earlier activity</p>"));
    assert!(past.contains("Back to the newest"));
    let unreadable = scene.get("/activity?before=yesterday").await?;
    assert!(unreadable.starts_with("HTTP/1.1 400"), "{unreadable}");
    assert!(unreadable.contains("This could not be read"));
    scene.stop().await?;
    let empty = Scene::start(false).await?;
    let page = empty.get("/activity").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("<p class=\"empty__title\">No activity yet</p>"));
    assert!(page.contains("<span class=\"mono\">0</span> events"));
    empty.stop().await
}

// Owner decisions 2026-11-20 (PR c): a store that cannot be read gets 503 and
// one that is too slow 504, both in the new look with Activity marked.
#[tokio::test(flavor = "multi_thread")]
async fn activity_says_when_the_store_cannot_be_read_or_is_slow() -> Result<(), Error> {
    let scene = Scene::start(false).await?;
    rusqlite::Connection::open(scene.database())?
        .execute_batch("ALTER TABLE audit RENAME TO audit_away")?;
    let broken = scene.get("/activity").await?;
    assert!(broken.starts_with("HTTP/1.1 503"), "{broken}");
    assert!(broken.contains("Analyte could not read its stored data just now"));
    assert!(broken.contains("<a href=\"/activity\" aria-current=\"page\">"));
    assert!(broken.contains("Details for support"));
    rusqlite::Connection::open(scene.database())?
        .execute_batch("ALTER TABLE audit_away RENAME TO audit")?;
    let busy = scene.store.clone();
    let blocker = tokio::spawn(async move {
        busy.request::<_, FixtureError>(fixture_deadline()?, |_| {
            std::thread::sleep(Duration::from_millis(1800));
            Ok(())
        })
        .await
        .map_err(Error::from)
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let slow = scene.get("/activity").await?;
    assert!(slow.starts_with("HTTP/1.1 504"), "{slow}");
    assert!(slow.contains("Analyte did not finish reading within 1.5 seconds."));
    assert!(slow.contains("admin Activity deadline of 1500 ms exceeded"));
    assert!(slow.contains("<a href=\"/activity\" aria-current=\"page\">"));
    blocker.await??;
    scene.stop().await
}
