//! Tests the overview's first-run welcome and the guided Needs review list through the real handlers.
use super::pages::login;
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
            "href=\"/setup\">Set up a machine</a></p></div></main></body></html>",
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

/// The button an item carries on the Needs review list.
#[derive(Clone, Copy)]
enum Button {
    Process,
    Send,
    None,
}

/// What each item of [`review_items`] says to do, in order, and its button.
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
        "The lab app already holds different content for this delivery. Check it in the lab app; Analyte does not send it again on its own. Then ask IT to mark this item resolved with the command line.",
        Button::None,
    ),
    (
        "The lab app sent an older or changed copy of an order Analyte already has; Analyte kept its copy. Correct the order in the lab app, then ask IT to mark this item resolved with the command line.",
        Button::None,
    ),
    (
        "The lab app cancelled tests the machine has already reported on. Check both on the machine and in the lab app; Analyte sends the machine nothing more. Then ask IT to mark this item resolved with the command line.",
        Button::None,
    ),
    (
        "Check on the machine whether this exchange finished. Until this item is resolved, Analyte does not start another one with that machine. Then ask IT to mark this item resolved with the command line.",
        Button::None,
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

/// One list item says what to do, carries its button and links to its own page.
fn assert_item(section: &str, id: i64, delivery: &str, (todo, button): (&str, Button)) {
    let todo = todo.replace('\'', "&#39;");
    assert!(
        section.contains(&format!(
            "<p class=\"todo\"><strong>What to do:</strong> {todo}</p>"
        )),
        "{todo}
{section}"
    );
    let expected = match button {
        Button::Process => Some(process_button(id)),
        Button::Send => Some(format!(
            "<a class=\"button\" title=\"Resend this delivery\" href=\"/deliveries/{delivery}/resend\">Send again</a>"
        )),
        Button::None => None,
    };
    match expected {
        Some(button) => assert!(
            section.contains(&button),
            "{button}
{section}"
        ),
        None => assert!(!section.contains("class=\"button\""), "{section}"),
    }
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
    assert!(detail.contains(&process_button(ids[0])));
    stop(state, stopper, owner, store).await
}
