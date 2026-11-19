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
    assert!(attention.contains("class=\"verdict__title\">1 thing needs you</h2>"));
    let disk = attention
        .split("<li>")
        .find(|item| {
            item.contains("<p class=\"todo__title\">Disk space is low: 512 bytes free</p>")
        })
        .ok_or("disk line missing")?;
    assert!(disk.contains("<a class=\"btn btn--outline\" href=\"/#disk\">Disk</a>"));
    assert!(attention.contains("id=\"disk\""));
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
    let machines = vec![machine("cobas-e411", true, link.subscribe(), "lab")?];
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
    let main = page
        .split_once("<main class=\"main\">")
        .map(|(_, main)| main)
        .ok_or("overview without main")?;
    let (head, rest) = main
        .split_once("</header>")
        .ok_or("overview without a page head")?;
    assert!(head.contains("<h1>Overview</h1>"), "{head}");
    assert!(
        rest.trim_start()
            .starts_with("<section class=\"verdict verdict--good\""),
        "{rest}"
    );
    assert!(rest.contains("class=\"verdict__title\">All good</h2>"));
    assert!(page.contains("<span class=\"node__state tone-good\">Port open</span>"));
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
        overview.contains("<p class=\"todo__title\">1 item needs review</p>"),
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

/// Queues one order message for `lab_app` at `now` and, when `delivered`,
/// records that the lab app accepted it.
fn queue(
    store: &mut analyte_store::Store,
    lab_app: &str,
    order: &str,
    delivered: bool,
    now: i64,
) -> Result<(), FixtureError> {
    let event = analyte_store::OrderEventDelivery {
        id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7())?,
        destination: Identifier::new(lab_app)?,
        order_id: Identifier::new(order)?,
        payload: b"synthetic-order-message".to_vec(),
        contract_version: 1,
    };
    store.enqueue_order_event(&event, now)?;
    if delivered {
        let claim = store
            .claim_destination(now, &event.destination)?
            .ok_or(FixtureError::UnknownDelivery)?;
        store.attempted(claim.id, &analyte_store::Attempt::Done, now)?;
    }
    Ok(())
}

/// The signal path item (machine or lab app) whose node is named `name`.
fn node<'a>(page: &'a str, name: &str) -> &'a str {
    page.split("<li class=\"path__item\">")
        .find(|item| {
            item.contains(&format!(
                "<span class=\"node__name\" title=\"{name}\">{name}</span>"
            ))
        })
        .unwrap_or_default()
}

// Owner request 2026-11-19 (admin restyle, PR a): the signal path has one node
// per set-up machine (its model, the observed state, last data, how it is
// attached, and a live, waiting or cut wire) and one per lab app (last
// delivered, what waits and since when, who sends to it, and for a kept lab
// app without keys which key is missing and what to do), all from real state.
// It shows no order or delivery identifiers and no payload.
#[tokio::test(flavor = "multi_thread")]
async fn signal_path_shows_machines_and_lab_apps_from_real_state() -> Result<(), Error> {
    use crate::machine::link::status::{Link, Reporter};
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let store = empty_store(directory.path()).await?;
    let now = jiff::Timestamp::now().as_second();
    let review = review_items("unused", &uuid::Uuid::now_v7().to_string())?
        .into_iter()
        .take(1)
        .collect::<Vec<_>>();
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            queue(s, "main-lab", "order-delivered", true, now - 60)?;
            queue(s, "old-lab", "order-waiting", false, now - 120)?;
            for item in &review {
                s.park(item)?;
            }
            Ok(())
        })
        .await?;
    let serial = Reporter::new(Link::Serial);
    (serial.observer())(analyte_transport::Activity::Opened);
    let usb = Reporter::new(Link::Serial);
    let listening = Reporter::new(Link::TcpServer);
    listening.ready(true);
    let folder = Reporter::new(Link::Folder);
    folder.ready(true);
    let mut adapter = machine("hematology-2", true, usb.subscribe(), "main-lab")?;
    adapter.connection = Connection::UsbAdapter("A9X".into());
    let machines = vec![
        machine("chemistry-1", true, serial.subscribe(), "main-lab")?,
        adapter,
        machine("immunoassay-3", true, listening.subscribe(), "main-lab")?,
        machine("urine-4", true, folder.subscribe(), "main-lab")?,
    ];
    let retired = crate::admin::Draining::from([(
        "old-lab".to_owned(),
        crate::admin::Kept {
            delivering: Arc::new(AtomicBool::new(true)),
            keys_missing: true,
            api_key: Identifier::new("old-lab-api-key")?,
            signing_key: Identifier::new("old-lab-signing-key")?,
        },
    )]);
    let (state, stopper, owner, host) = start_with(
        directory.path(),
        &store,
        (machines, vec![loopback_destination("main-lab")?]),
        retired,
        directory.path(),
    )
    .await?;
    let cookie = login(&host, &bearer).await?;
    let page = request(&host, "/", &format!("Cookie: {cookie}\r\n"), None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert!(page.contains("4 machines · 2 lab apps"));
    assert_machine_nodes(&page);
    assert_lab_app_nodes(&page);
    assert_figures_and_sidebar(&page, &state, &cookie)?;
    for hidden in [
        "order-delivered",
        "order-waiting",
        "synthetic-order-message",
    ] {
        assert!(!page.contains(hidden), "{hidden} shown");
    }
    drop((serial, usb, listening, folder));
    stop(state, stopper, owner, store).await
}

/// Each machine node: model, observed state, last data, attachment and wire;
/// the closed port is also a to-do line with what to check.
fn assert_machine_nodes(page: &str) {
    let open = node(page, "chemistry-1");
    for part in [
        "<span class=\"node__model\">Synthetic analyzer</span>",
        "<span class=\"node__state tone-good\">Port open</span>",
        "<span class=\"mono\">Nothing yet</span>",
        "</svg>Serial</span>",
        "<span class=\"wire \" aria-hidden=\"true\"><span class=\"wire__dot",
    ] {
        assert!(open.contains(part), "{part}\n{open}");
    }
    let closed = node(page, "hematology-2");
    for part in [
        "<a class=\"node node--bad\" href=\"/machines\">",
        "<span class=\"node__state tone-bad\">Port not open</span>",
        "</svg>USB adapter</span>",
        "<span class=\"node__note\">Port of USB adapter A9X not open since ",
        "<span class=\"wire wire--cut\" aria-hidden=\"true\"><span class=\"wire__break\">",
    ] {
        assert!(closed.contains(part), "{part}\n{closed}");
    }
    let waiting = node(page, "immunoassay-3");
    assert!(waiting.contains("<span class=\"node__state tone-warn\">Waiting</span>"));
    assert!(waiting.contains("Listening on port 5600, nothing connected since "));
    assert!(waiting.contains("<span class=\"wire wire--wait\""));
    let watching = node(page, "urine-4");
    assert!(watching.contains("<span class=\"node__state tone-good\">Watching</span>"));
    assert!(watching.contains("</svg>Watched folder</span>"));

    // The closed port is a to-do line: what Analyte observed and what to check.
    assert!(page.contains("<p class=\"todo__title\">hematology-2: Port not open</p>"));
    assert!(page.contains("<p class=\"todo__detail\">Port of USB adapter A9X not open since "));
    assert!(page.contains(". Check the cable and the machine&#39;s host settings.</p>"));
    assert!(page.contains("<section class=\"verdict verdict--bad\""));
}

/// The lab app a machine sends to, and the kept one whose keys are missing.
fn assert_lab_app_nodes(page: &str) {
    let main = node(page, "main-lab");
    for part in [
        "<span class=\"status status--good\">",
        "Up to date</span>",
        "<p class=\"node__line\">Last delivered ",
        "<p class=\"node__line\">Nothing waiting</p>",
        "<p class=\"node__line\">From chemistry-1, hematology-2, immunoassay-3, urine-4</p>",
    ] {
        assert!(main.contains(part), "{part}\n{main}");
    }
    let kept = node(page, "old-lab");
    for part in [
        "<span class=\"wire wire--cut\" aria-hidden=\"true\"><span class=\"wire__break\">",
        "<div class=\"node node--lab node--bad\">",
        "Cannot deliver</span>",
        "<p class=\"node__line\">Nothing delivered yet</p>",
        "<p class=\"node__line\">1 item waiting · oldest ",
        "<p class=\"node__line\">No set-up machine sends here</p>",
        "<p class=\"node__line\">Keys missing: API key and signing key.</p>",
        "Waiting items go out after Analyte restarts with the keys.",
        "<a class=\"btn btn--primary btn--sm\" href=\"/setup/lab-apps\">Add keys</a>",
    ] {
        assert!(kept.contains(part), "{part}\n{kept}");
    }
}

/// The figures under the path, the review count and this session's Sign out.
fn assert_figures_and_sidebar(page: &str, state: &Context, cookie: &str) -> Result<(), Error> {
    // Figures: what waits for every lab app, and the order list only where it is polled.
    assert!(page.contains("<dt>Items waiting to send</dt>\n<dd>1 "));
    let oldest = page
        .split("<div class=\"figure\">")
        .find(|figure| figure.contains("<dt>Oldest waiting</dt>"))
        .unwrap_or_default();
    assert!(oldest.contains("<time datetime="), "{oldest}");
    assert!(page.contains("<dt>Order list · main-lab</dt>"));
    assert!(!page.contains("<dt>Order list · old-lab</dt>"));

    // The sidebar: open review items beside Needs review, and Sign out with this session's token.
    assert!(page.contains("<span class=\"nav__count\"><span class=\"sr-only\">, </span>1</span>"));
    let token = state
        .sessions
        .lookup(cookie.strip_prefix("analyte_session="))?
        .ok_or("session missing")?
        .csrf
        .form_value()
        .to_owned();
    assert!(page.contains(&format!(
        "<form class=\"sidebar__signout\" method=\"post\" action=\"/logout\"><input type=\"hidden\" name=\"csrf\" value=\"{token}\">"
    )));
    Ok(())
}

// Owner request 2026-11-19 (admin restyle): when the overview cannot read its
// data it says so in the new look, keeps the sidebar, answers 503 and keeps the
// exact reason under "Details for support".
#[tokio::test(flavor = "multi_thread")]
async fn overview_error_page_keeps_the_reason_for_support() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    // A store whose worker has stopped refuses every read.
    let mut closed = Arc::try_unwrap(empty_store(directory.path()).await?)
        .map_err(|_| "fixture store shared")?;
    closed.shutdown(fixture_deadline()?).await?;
    let store = Arc::new(closed);
    let (state, stop, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let page = request(
        &host,
        "/",
        &format!("Authorization: Bearer {bearer}\r\n"),
        None,
    )
    .await?;
    assert!(page.starts_with("HTTP/1.1 503"), "{page}");
    headers_present(&page);
    assert!(page.contains("<nav class=\"sidebar\" aria-label=\"Main\">"));
    assert!(page.contains("<h1>Overview</h1>"));
    assert!(page.contains("<div class=\"alert alert--bad\" role=\"note\">"));
    assert!(
        page.contains("<p class=\"alert__title\">The overview could not be shown just now</p>")
    );
    assert!(page.contains("Details for support</summary><pre>"));
    assert!(page.contains("app.css"));
    assert!(!page.contains("admin.css"));
    assert!(
        !page.contains("nav__count"),
        "no count when nothing could be read"
    );
    drop(state);
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    Ok(())
}
