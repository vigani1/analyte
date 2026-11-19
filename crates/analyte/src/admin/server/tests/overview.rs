//! Tests the overview's first-run welcome and the guided Needs review list through the real handlers.
use super::pages::{csrf, encode, login};
use super::*;
use analyte_store::Parked;

async fn empty_store(root: &std::path::Path) -> Result<Arc<analyte_store::Handle>, Error> {
    let limits = {
        let mut limits = analyte_fixtures::store_limits();
        limits.reserve_bytes = 1;
        limits.page_rows = 100;
        limits.busy_timeout = Duration::from_millis(100);
        limits
    };
    Ok(Arc::new(
        analyte_store::Handle::start(
            root.join("analyte.db"),
            limits,
            Box::new(Space),
            4,
            fixture_deadline()?,
        )
        .await
        .map_err(|error| format!("overview fixture migration: {error}"))?,
    ))
}

async fn stop(
    state: Arc<Context>,
    stop: watch::Sender<bool>,
    owner: tokio::task::JoinHandle<Result<(), crate::admin::Error>>,
    store: Arc<analyte_store::Handle>,
) -> Result<(), Error> {
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    let mut store = Arc::try_unwrap(store).map_err(|_| "admin retained store")?;
    store.shutdown(fixture_deadline()?).await?;
    Ok(())
}

// Owner request 2026-11-19 (overview item 2): with no machine set up and nothing
// that needs anyone, the overview is only a welcome card with one button to the
// setup page; the menu stays. Once something needs someone, the full overview
// with its status line comes back even without a machine.
#[tokio::test(flavor = "multi_thread")]
async fn first_run_overview_is_only_the_welcome_card() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let auth = format!("Authorization: Bearer {bearer}\r\n");
    let page = request(&host, "/", &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    headers_present(&page);
    let main = page
        .split_once("<main>")
        .map(|(_, main)| main)
        .ok_or("overview without main")?;
    assert_eq!(
        main,
        concat!(
            "<h1>Welcome to Analyte</h1><div class=\"welcome\"><h2>Set up your first ",
            "machine</h2><p>Analyte is running, but no machine is set up yet. On the Setup ",
            "page you choose the machine model, how it is connected and the lab app that ",
            "receives its results.</p><p class=\"actions-row\"><a class=\"button\" ",
            "href=\"/setup/first\">Set up a machine</a></p></div></main></body></html>",
        )
    );
    assert!(page.contains("<nav aria-label=\"Admin pages\">"));
    assert!(page.contains("<a href=\"/\" aria-current=\"page\">Overview</a>"));
    store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            s.evaluate_alert(
                &analyte_store::AlertKey::DiskReserve,
                "duty",
                true,
                Some(512),
                100,
            )?;
            Ok::<_, FixtureError>(())
        })
        .await?;
    let attention = request(&host, "/", &auth, None).await?;
    assert!(!attention.contains("Set up your first machine"));
    assert!(attention.contains("<p class=\"verdict\">1 thing needs you</p>"));
    assert!(
        attention
            .contains("<span>Disk space is low: 512 bytes free</span> <a href=\"/#disk\">Disk</a>")
    );
    assert!(attention.contains("No machine is running. Add or set one up"));
    stop(state, stopper, owner, store).await
}

// Owner request 2026-11-19 (overview item 1): a running machine that is
// connected and nothing else waiting reads "All good".
#[tokio::test(flavor = "multi_thread")]
async fn overview_with_nothing_wrong_reads_all_good() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    let link =
        crate::machine::link::status::Reporter::new(crate::machine::link::status::Link::Serial);
    (link.observer())(analyte_transport::Activity::Opened);
    let machines = vec![Machine {
        departure_html: Markup::new(),
        id: Identifier::new("cobas-e411")?,
        running: Arc::new(AtomicBool::new(true)),
        status: link.subscribe(),
    }];
    let (state, stopper, owner, host) = start_overview_server(
        directory.path(),
        &store,
        machines,
        vec![loopback_destination("lab")?],
    )
    .await?;
    let page = request(
        &host,
        "/",
        &format!("Authorization: Bearer {bearer}\r\n"),
        None,
    )
    .await?;
    assert!(page.contains(concat!(
        "<main><h1>Overview</h1><div class=\"status good\" role=\"status\">",
        "<p class=\"verdict\">All good</p>"
    )));
    assert!(page.contains("<span class=\"pill ok\">Port open</span>"));
    drop(link);
    stop(state, stopper, owner, store).await
}

/// One synthetic item per category, plus one whose category this version does not know.
fn review_items(receipt: &str, delivery: &str) -> Result<Vec<Parked>, Error> {
    let item = |kind: &str, reason: &str, target: &str| -> Result<Parked, Error> {
        Ok(Parked {
            kind: Identifier::new(kind)?,
            reason: Reason::new(reason)?,
            targets: vec![Identifier::new(target)?],
        })
    };
    Ok(vec![
        item(
            "mapping_finding",
            "Position { record: 3, field: 3 }: UnknownTest",
            receipt,
        )?,
        item(
            "interpretation_failure",
            "checksum mismatch in frame 2",
            receipt,
        )?,
        item(
            "result_reconciliation",
            "result reconciliation: Conflict",
            receipt,
        )?,
        item(
            "incomplete_transfer",
            "link lost before end of transmission",
            "transfer-7",
        )?,
        item("parked", "lab app refused: unknown test code", delivery)?,
        item("conflict", "lab app holds different content", delivery)?,
        item(
            "order_version_refused",
            "version 2 is older than cached version 3",
            "order-9",
        )?,
        item(
            "law_cancel_refusal_conflict",
            "LAB-28 UC conflicts with retained analyzer CA",
            "awos-1",
        )?,
        item(
            "poll_intent",
            "query sent; no answer before the deadline",
            "cobas-e411",
        )?,
        item("future_kind", "something new", "x")?,
    ])
}

/// The main button an item carries on the Needs review list.
#[derive(Clone, Copy)]
enum Button {
    Process,
    Send,
    Resolve,
}

/// What each item of [`review_items`] says to do, in order, and its main button.
const STEPS: [(&str, Button); 10] = [
    (
        "The machine sent a test code that this machine's test mapping does not list. Add it to the test mapping, then use Process again.",
        Button::Process,
    ),
    (
        "Check the machine's settings on the Setup page (machine model and connection) against the reason above, then use Process again.",
        Button::Process,
    ),
    (
        "Compare this result with the earlier results for the same sample on the machine. If a setting caused the mismatch, fix it, then use Process again.",
        Button::Process,
    ),
    (
        "The machine stopped part-way through sending. Check the cable or network and have the machine send the results again, then use Process again here.",
        Button::Process,
    ),
    (
        "Fix the problem the lab app reported (see Reason), then use Send again.",
        Button::Send,
    ),
    (
        "The lab app already holds different content for this delivery; Analyte does not send it again on its own. Check the delivery in the lab app and settle it there, then Mark resolved.",
        Button::Resolve,
    ),
    (
        "The lab app sent an older or changed copy of an order Analyte already has; Analyte kept its copy. Correct the order in the lab app, then Mark resolved.",
        Button::Resolve,
    ),
    (
        "The lab app cancelled tests the machine has already reported on; Analyte sends the machine nothing more. Check both on the machine and in the lab app, then Mark resolved.",
        Button::Resolve,
    ),
    (
        "Analyte starts no other exchange with this machine until this item is resolved. Check on the machine whether this one finished, then Mark resolved.",
        Button::Resolve,
    ),
    (
        "Read the reason above and open the item for its linked records. If it is not clear what to do, give the item number to support.",
        Button::Process,
    ),
];

fn process_button(id: i64) -> String {
    format!(
        "<a class=\"button\" title=\"Reprocess\" href=\"/parked/{id}/reprocess\">Process again</a>"
    )
}

/// The Mark resolved button: the main one, or secondary beside another action.
fn resolve_button(id: i64, main: bool) -> String {
    let class = if main { "button" } else { "button secondary" };
    format!(
        "<a class=\"{class}\" title=\"Resolve\" href=\"/parked/{id}/resolve\">Mark resolved</a>"
    )
}

/// One list item says what to do, carries its buttons and links to its own page.
fn assert_item(section: &str, id: i64, delivery: &str, (todo, button): (&str, Button)) {
    let todo = todo.replace('\'', "&#39;");
    assert!(
        section.contains(&format!(
            "<p class=\"todo\"><strong>What to do:</strong> {todo}</p>"
        )),
        "{todo}\n{section}"
    );
    let buttons = match button {
        Button::Process => process_button(id) + &resolve_button(id, false),
        Button::Send => format!(
            "<a class=\"button\" title=\"Resend this delivery\" href=\"/deliveries/{delivery}/resend\">Send again</a>{}",
            resolve_button(id, false)
        ),
        Button::Resolve => format!("<p class=\"actions-row\">{}", resolve_button(id, true)),
    };
    assert!(section.contains(&buttons), "{buttons}\n{section}");
    assert_eq!(
        section.matches("class=\"button").count(),
        match button {
            Button::Resolve => 1,
            Button::Process | Button::Send => 2,
        }
    );
    assert!(section.contains(&format!("<a href=\"/parked/{id}\">Open item</a>")));
}

// Owner request 2026-11-19 (Needs review item 3): every item says what to do in
// plain words for its category, carries its action button, and the list shows no
// linked record IDs or receipt times; those stay on the item's page.
#[tokio::test(flavor = "multi_thread")]
async fn review_list_says_what_to_do_with_the_action_and_no_ids() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    let receipt = uuid::Uuid::now_v7().to_string();
    let delivery = uuid::Uuid::now_v7().to_string();
    let items = review_items(&receipt, &delivery)?;
    let ids = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            items
                .iter()
                .map(|item| s.park(item).map_err(FixtureError::from))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?;
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let cookie = login(&host, &bearer).await?;
    let auth = format!("Cookie: {cookie}\r\n");
    let list = request(&host, "/parked", &auth, None).await?;
    assert!(list.starts_with("HTTP/1.1 200"), "{list}");
    let sections: Vec<&str> = list.split("<section>").skip(1).collect();
    assert_eq!(sections.len(), ids.len());
    for ((section, step), id) in sections.iter().zip(STEPS).zip(&ids) {
        assert_item(section, *id, &delivery, step);
    }
    for hidden in [
        receipt.as_str(),
        "transfer-7",
        "order-9",
        "Linked record",
        "Received</dt>",
        "Target",
    ] {
        assert!(!list.contains(hidden), "{hidden} shown on the list");
    }
    // The ID and the receipt time stay on the item's own page.
    let detail = request(&host, &format!("/parked/{}", ids[0]), &auth, None).await?;
    assert!(detail.contains(&format!("<code>{receipt}</code>")));
    assert!(
        detail.contains(
            "<p class=\"todo\"><strong>What to do:</strong> The machine sent a test code"
        )
    );
    assert!(detail.contains(&(process_button(ids[0]) + &resolve_button(ids[0], false))));
    stop(state, stopper, owner, store).await
}

// Owner request 2026-11-19 (Needs review follow-up) and ADR 0003 rule 2: Mark
// resolved on the page needs the session's CSRF token and a reason, uses the same
// store resolution and audit as `analyte parked resolve`, and never deletes the
// item: it leaves the unresolved count and stays in the history, resolved.
#[tokio::test(flavor = "multi_thread")]
async fn mark_resolved_is_audited_needs_csrf_and_keeps_the_item() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    // The conflicting delivery and the refused order from the category list.
    let items: Vec<Parked> = review_items("unused", &uuid::Uuid::now_v7().to_string())?
        .into_iter()
        .skip(5)
        .take(2)
        .collect();
    let ids = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            items
                .iter()
                .map(|item| s.park(item).map_err(FixtureError::from))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?;
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let cookie = login(&host, &bearer).await?;
    let auth = format!("Cookie: {cookie}\r\n");
    let post_auth = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let id = ids[0];
    let path = format!("/parked/{id}/resolve");
    let list = request(&host, "/parked", &auth, None).await?;
    assert!(list.contains(&resolve_button(id, true)), "{list}");
    let form = confirm_needs_csrf(&host, &auth, &path).await?;
    let unresolved = |store: Arc<analyte_store::Handle>| async move {
        store
            .request::<_, FixtureError>(fixture_deadline()?, |s| {
                Ok(
                    s.alert_value(&analyte_store::AlertKey::ParkedItems, 0, &[])?
                        .unwrap_or_default(),
                )
            })
            .await
            .map_err(Error::from)
    };
    assert_eq!(
        unresolved(store.clone()).await?,
        2,
        "refused posts change nothing"
    );
    let done = request(&host, &path, &post_auth, Some(&form)).await?;
    assert!(done.starts_with("HTTP/1.1 200"), "{done}");
    assert!(done.contains("<h1>Marked resolved</h1>"));
    let again = request(&host, &path, &post_auth, Some(&form)).await?;
    assert!(again.starts_with("HTTP/1.1 409"), "resolves only once");
    assert_recorded(&store, &state, &cookie, id).await?;
    assert_eq!(unresolved(store.clone()).await?, 1);
    // Still in the history: listed as resolved, with who and why on its page.
    let history = request(&host, "/parked", &auth, None).await?;
    let section = history
        .split("<section>")
        .find(|section| section.contains(&format!("href=\"/parked/{id}\" title")))
        .ok_or("resolved item missing from the list")?;
    assert!(
        section.contains("<span class=\"pill ok\">Resolved</span>"),
        "{section}"
    );
    assert!(!section.contains("Mark resolved"), "{section}");
    let detail = request(&host, &format!("/parked/{id}"), &auth, None).await?;
    assert!(detail.contains("How it was resolved"));
    assert!(detail.contains("settled with the lab app"));
    let overview = request(&host, "/", &auth, None).await?;
    assert!(
        overview.contains("<span>1 item needs review</span>"),
        "{overview}"
    );
    stop(state, stopper, owner, store).await
}

/// The confirmation says the item stays in the history; posting needs the
/// session's CSRF token and "confirm=yes". Returns a valid form body.
async fn confirm_needs_csrf(host: &str, auth: &str, path: &str) -> Result<String, Error> {
    let confirm = request(host, path, auth, None).await?;
    assert!(confirm.starts_with("HTTP/1.1 200"), "{confirm}");
    assert!(confirm.contains("<h1>Mark this item resolved?</h1>"));
    assert!(
        confirm.contains("The item stays in the history, marked resolved by you with your reason.")
    );
    assert!(confirm.contains("method=\"post\""));
    let token = csrf(&confirm)?;
    let form = format!(
        "csrf={}&confirm=yes&reason=settled%20with%20the%20lab%20app",
        encode(&token)
    );
    let post_auth = format!(
        "{auth}Content-Type: application/x-www-form-urlencoded
"
    );
    for refused in [
        "confirm=yes&reason=no%20token".to_owned(),
        form.rewrite(&encode(&token), "wrong"),
        form.rewrite("confirm=yes", "confirm=no"),
    ] {
        let response = request(host, path, &post_auth, Some(&refused)).await?;
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    }
    Ok(form)
}

/// The item carries its resolution and the audit trail one matching row: this
/// session as the actor, the CLI's action name, the item and the typed reason.
async fn assert_recorded(
    store: &analyte_store::Handle,
    state: &Context,
    cookie: &str,
    id: i64,
) -> Result<(), Error> {
    let actor = state
        .sessions
        .lookup(cookie.strip_prefix("analyte_session="))?
        .ok_or("session missing")?
        .actor
        .clone();
    let (resolution, audits) = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            let item = s.parked_item(id)?.ok_or(FixtureError::Corruption)?;
            Ok((item.resolution, s.audits(0)?))
        })
        .await?;
    let resolution: analyte_store::Audit =
        serde_json::from_str(&resolution.ok_or("item not resolved")?)?;
    assert_eq!(audits.len(), 1, "one audit row");
    let (_, audit) = audits.first().ok_or("audit row missing")?;
    for recorded in [&resolution, audit] {
        assert_eq!(recorded.actor, actor);
        assert_eq!(recorded.action.as_str(), "parked-resolve");
        assert_eq!(recorded.targets, vec![Identifier::new(&id.to_string())?]);
        assert_eq!(recorded.reason.as_str(), "settled with the lab app");
    }
    Ok(())
}
