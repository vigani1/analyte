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
    let blocker = hold_store(&scene.store, PAST_READ_BUDGET).await?;
    let slow = scene.get("/activity").await?;
    assert!(slow.starts_with("HTTP/1.1 504"), "{slow}");
    assert!(slow.contains("Analyte did not finish reading within 1.5 seconds."));
    assert!(slow.contains("admin Activity deadline of 1500 ms exceeded"));
    assert!(slow.contains("<a href=\"/activity\" aria-current=\"page\">"));
    blocker.await??;
    scene.stop().await
}

/// A failed try as stored, to be tried again `after` (Unix seconds).
fn retry(after: i64) -> String {
    format!("{{\"RetryAt\":{after}}}")
}

/// Queues a delivery of the message from `machine` to `lab_app`, in `state`,
/// with its tries as (stored outcome, seconds before the scene's time), and
/// returns its ID.
pub(super) fn queued(
    scene: &Scene,
    machine: &str,
    lab_app: &str,
    state: &str,
    tries: &[(String, i64)],
) -> Result<uuid::Uuid, Error> {
    let db = rusqlite::Connection::open(scene.database())?;
    let receipt: String = db.query_row(
        "SELECT id FROM receipts WHERE json_extract(metadata,'$.machine')=?1",
        [machine],
        |row| row.get(0),
    )?;
    let delivery = uuid::Uuid::now_v7();
    let id = serde_json::to_string(&delivery.to_string())?;
    db.execute(
        "INSERT INTO outbox(id,kind,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,'result',?2,1,?3,?1,x'00',x'00',1,?4,0,?5)",
        rusqlite::params![id, receipt, lab_app, state, AT - 90_000],
    )?;
    for (outcome, ago) in tries {
        db.execute(
            "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
            rusqlite::params![id, outcome, AT.saturating_sub(*ago)],
        )?;
    }
    Ok(delivery)
}

/// `count` failed tries, `every` seconds apart, the last `last_ago` seconds
/// before the scene's time, each to be tried again a minute later.
pub(super) fn failed(count: i64, every: i64, last_ago: i64) -> Vec<(String, i64)> {
    (0..count)
        .rev()
        .map(|step| {
            let ago = last_ago.saturating_add(step.saturating_mul(every));
            (retry(AT.saturating_sub(ago).saturating_add(60)), ago)
        })
        .collect()
}

/// The words of the line that says `words`, and its key for the live script.
fn line_with<'a>(page: &'a str, words: &str) -> Option<(&'a str, &'a str)> {
    page.split("<li class=\"feed__line\" data-key=\"")
        .skip(1)
        .filter_map(|line| line.split_once('"'))
        .map(|(key, rest)| (key, rest.split("</li>").next().unwrap_or_default()))
        .find(|(_, line)| line.contains(words))
}

/// The time of day, as Activity says it, `ago` seconds before the scene's
/// time, beside a line `last_ago` seconds before it.
fn clock(ago: i64, last_ago: i64) -> Result<String, Error> {
    Ok(crate::admin::view::format::clock(
        jiff::Timestamp::from_second(AT.saturating_sub(ago))?,
        jiff::Timestamp::from_second(AT.saturating_sub(last_ago))?,
    ))
}

/// The scene with one delivery to main-lab failing 35 times while the lab
/// app is down, still waiting for its next try.
pub(super) fn outage(scene: &Scene) -> Result<uuid::Uuid, Error> {
    queued(
        scene,
        "immunoassay-analyzer",
        "main-lab",
        "pending",
        &failed(35, 980, 0),
    )
}

// Owner decision 2026-11-21 (Activity groups retries): a failed try with no
// failed try of the same delivery next to it stays one line, as before.
#[tokio::test(flavor = "multi_thread")]
async fn a_single_failed_try_is_its_own_line() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    seed_activity(&scene)?;
    let page = scene.get("/activity").await?;
    let (_, line) = line_with(&page, "failed, will retry").ok_or("the failed try")?;
    assert!(
        line.ends_with(
            "– sample 261119-031 – 8 results – sending to main-lab failed, will retry</span></span>"
        ),
        "{line}"
    );
    assert!(!page.contains("tried "), "nothing is grouped");
    assert!(!page.contains("Every try"));
    scene.stop().await
}

// Owner decision 2026-11-21 (Activity groups retries): failed tries in a row
// of one delivery are one line: how many, from when to when, the next try
// while it waits, and a link to every try.
#[tokio::test(flavor = "multi_thread")]
async fn failed_tries_in_a_row_are_one_line() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let delivery = outage(&scene)?;
    let page = scene.get("/activity").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    let (key, line) = line_with(&page, "tried 35 times").ok_or("the group")?;
    let words = format!(
        "– sample 261119-031 – 8 results – sending to main-lab failed · tried 35 times, {} – {} · next try after {}</span></span><a class=\"link\" href=\"/activity?delivery={delivery}\">Every try</a>",
        clock(34 * 980, 0)?,
        clock(0, 0)?,
        clock(-60, 0)?
    );
    assert!(line.ends_with(&words), "{line}");
    assert!(line.contains("aria-label=\"Will retry\""));
    assert!(key.starts_with(&format!("tries.{delivery}.")), "{key}");
    assert_eq!(
        lines(&page)
            .iter()
            .filter(|line| line.contains("sending to main-lab"))
            .count(),
        1,
        "the tries are not listed one by one"
    );
    // Every try is still there, one line each, newest first.
    let every = scene.get(&format!("/activity?delivery={delivery}")).await?;
    assert!(every.starts_with("HTTP/1.1 200"), "{every}");
    assert!(every.contains(
        "<span class=\"subtle\">Every try of one delivery, newest first, one line each</span>"
    ));
    assert!(every.contains("<a class=\"link\" href=\"/activity\">Show all activity</a>"));
    assert!(
        !every.contains("name=\"show\""),
        "no filters for one delivery"
    );
    let tries = lines(&every);
    assert_eq!(tries.len(), 35);
    assert!(
        tries
            .iter()
            .all(|line| line.contains("sending to main-lab failed, will retry"))
    );
    assert!(!every.contains("Show earlier activity"));
    let unknown = scene
        .get(&format!("/activity?delivery={}", uuid::Uuid::now_v7()))
        .await?;
    assert!(unknown.contains("<p class=\"empty__title\">No tries kept</p>"));
    let unreadable = scene.get("/activity?delivery=yesterday").await?;
    assert!(unreadable.starts_with("HTTP/1.1 400"), "{unreadable}");
    scene.stop().await
}

// Owner decision 2026-11-21 (Activity groups retries): a success ends the
// tries in a row; their line no longer says a next try.
#[tokio::test(flavor = "multi_thread")]
async fn a_success_ends_the_group() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let mut tries = failed(5, 100, 4_600);
    tries.push(("\"Done\"".into(), 4_500));
    queued(&scene, "chemistry-analyzer", "main-lab", "done", &tries)?;
    let page = scene.get("/activity").await?;
    let shown = lines(&page);
    let delivered = shown
        .iter()
        .position(|line| line.contains("12 results – delivered to main-lab"))
        .ok_or("the delivered try")?;
    let group = shown
        .iter()
        .position(|line| line.contains("tried 5 times"))
        .ok_or("the group")?;
    assert!(
        delivered < group,
        "the success is newer than the tries it ended"
    );
    let group = shown.get(group).ok_or("the group")?;
    assert!(
        group.contains(&format!(
            "sending to main-lab failed · tried 5 times, {} – {}</span>",
            clock(5_000, 4_600)?,
            clock(4_600, 4_600)?
        )),
        "{group}"
    );
    assert!(!page.contains("next try"));
    scene.stop().await
}

// Owner decision 2026-11-21 (Activity groups retries): a park ends the tries
// in a row, and the rejection keeps its own line and link.
#[tokio::test(flavor = "multi_thread")]
async fn a_park_ends_the_group() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let mut tries = failed(4, 300, 3_000);
    tries.push(("{\"Parked\":\"HTTP 422\"}".into(), 2_600));
    queued(&scene, "chemistry-analyzer", "main-lab", "parked", &tries)?;
    let page = scene.get("/activity").await?;
    let shown = lines(&page);
    let rejected = shown
        .iter()
        .position(|line| line.contains("12 results – rejected by main-lab"))
        .ok_or("the rejection")?;
    assert!(shown.get(rejected).is_some_and(|line| {
        line.contains("<a class=\"link\" href=\"/deliveries?show=undelivered\">Deliveries</a>")
    }));
    let group = shown
        .iter()
        .position(|line| line.contains("tried 4 times"))
        .ok_or("the group")?;
    assert!(
        rejected < group,
        "the rejection is newer than the tries it ended"
    );
    let group = shown.get(group).ok_or("the group")?;
    assert!(
        group.contains(&format!(
            "sending to main-lab failed · tried 4 times, {} – {}</span>",
            clock(3_900, 3_000)?,
            clock(3_000, 3_000)?
        )),
        "{group}"
    );
    assert!(!page.contains("next try"));
    assert!(!page.contains("HTTP 422"));
    scene.stop().await
}

/// The scene with the same message failing to two lab apps in turn: one
/// try a minute, main-lab and outpatient-lab taking turns, 6 each.
pub(super) fn in_turn(scene: &Scene) -> Result<(), Error> {
    queued(
        scene,
        "hematology-analyzer",
        "main-lab",
        "pending",
        &failed(6, 120, 60),
    )?;
    queued(
        scene,
        "hematology-analyzer",
        "outpatient-lab",
        "pending",
        &failed(6, 120, 0),
    )?;
    Ok(())
}

// Owner decision 2026-11-21 (Activity groups retries): tries are grouped
// per delivery, so two lab apps failing in turn are two lines, one each,
// even though their tries take turns.
#[tokio::test(flavor = "multi_thread")]
async fn two_lab_apps_failing_in_turn_are_two_lines() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    in_turn(&scene)?;
    let page = scene.get("/activity").await?;
    let shown = lines(&page);
    let tried: Vec<&&str> = shown
        .iter()
        .filter(|line| line.contains("tried "))
        .collect();
    assert_eq!(tried.len(), 2, "{tried:?}");
    let outpatient = format!(
        "sending to outpatient-lab failed · tried 6 times, {} – {} · next try after",
        clock(600, 0)?,
        clock(0, 0)?
    );
    let main = format!(
        "sending to main-lab failed · tried 6 times, {} – {} · next try after",
        clock(660, 60)?,
        clock(60, 60)?
    );
    assert!(tried.first().is_some_and(|line| line.contains(&outpatient)));
    assert!(tried.get(1).is_some_and(|line| line.contains(&main)));
    assert!(!page.contains("failed, will retry"));
    scene.stop().await
}

// Owner decision 2026-11-21 (Activity groups retries): a group never splits
// across pages. It sits at its newest try; the page after it does not list
// its earlier tries, and a page counts lines as shown.
#[tokio::test(flavor = "multi_thread")]
async fn a_group_never_splits_across_pages() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let delivery = queued(
        &scene,
        "immunoassay-analyzer",
        "main-lab",
        "pending",
        &failed(40, 120, 0),
    )?;
    let db = rusqlite::Connection::open(scene.database())?;
    for step in 0..60 {
        db.execute(
            "INSERT INTO audit(evidence) VALUES (?1)",
            [format!(
                "{{\"actor\":\"admin-session-1\",\"action\":\"setup-save\",\"targets\":[\"chemistry-analyzer\"],\"time\":{},\"reason\":\"saved\"}}",
                AT - 30 - step * 60
            )],
        )?;
    }
    drop(db);
    let first = scene.get("/activity").await?;
    assert_eq!(
        lines(&first).len(),
        50,
        "50 lines as shown, the group one of them"
    );
    assert!(
        line_with(&first, "tried 40 times").is_some(),
        "the group is whole on the first page"
    );
    let href = first
        .split("<a class=\"btn btn--quiet btn--sm\" rel=\"next\" href=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or("Show earlier activity")?
        .replace("&#38;", "&");
    let rest = scene.get(&href).await?;
    assert!(rest.starts_with("HTTP/1.1 200"), "{rest}");
    assert!(!lines(&rest).is_empty());
    assert!(
        !rest.contains("sending to main-lab"),
        "none of the group's earlier tries comes back on the next page"
    );
    assert!(!rest.contains(&delivery.to_string()));
    scene.stop().await
}

// Owner decision 2026-11-21 (Activity groups retries): a new failed try
// grows its group instead of adding a line; the line keeps its key, so the
// live script swaps it in place rather than marking a new line.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_try_grows_its_group() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let delivery = outage(&scene)?;
    let before = scene.get("/activity").await?;
    let (key, _) = line_with(&before, "tried 35 times").ok_or("the group")?;
    let db = rusqlite::Connection::open(scene.database())?;
    db.execute(
        "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
        rusqlite::params![
            serde_json::to_string(&delivery.to_string())?,
            retry(AT + 1_040),
            AT + 980
        ],
    )?;
    drop(db);
    let after = scene.get("/activity").await?;
    let (grown, line) = line_with(&after, "tried 36 times").ok_or("the grown group")?;
    assert_eq!(grown, key, "the same line, updated");
    assert!(
        line.contains(&format!(
            "tried 36 times, {} – {} · next try after {}",
            clock(34 * 980, -980)?,
            clock(-980, -980)?,
            clock(-1_040, -980)?
        )),
        "{line}"
    );
    assert_eq!(lines(&after).len(), lines(&before).len());
    assert!(!after.contains("tried 35 times"));
    scene.stop().await
}
