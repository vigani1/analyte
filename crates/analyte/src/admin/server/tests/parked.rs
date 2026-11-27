//! Tests every state of the restyled Needs review pages through the real handlers: filters, items, history, dialogs, refusals and failures.
use super::overview::{empty_store, stop};
use super::pages::{csrf, encode, login};
use super::*;
use analyte_model::{Digest, NamespacedId, ReceiptId, ReceiptMetadata};
use analyte_store::{Audit, Parked};

/// The session actor the admin pages record, as a test writes it.
const SESSION: &str = "admin-session-0193a1b2-0000-7000-8000-000000000001";

fn id(text: &str) -> Result<Identifier, FixtureError> {
    Identifier::new(text).map_err(FixtureError::Text)
}

/// Stores one message from `machine`, received at Unix second `at`.
fn receive(
    store: &mut analyte_store::Store,
    machine: &str,
    at: i64,
) -> Result<ReceiptId, FixtureError> {
    let raw = b"synthetic message";
    let config = b"synthetic config";
    let receipt = ReceiptId::try_from(uuid::Uuid::now_v7()).map_err(FixtureError::Identity)?;
    store.receive(
        &ReceiptMetadata {
            id: receipt,
            raw_digest: Digest(analyte_contract::content_digest(raw)),
            connection: id("synthetic")?,
            machine: id(machine)?,
            driver: NamespacedId {
                namespace: id("synthetic")?,
                value: id("1")?,
            },
            config_version: id("1")?,
            config_digest: Digest(analyte_contract::content_digest(config)),
            received_at: jiff::Timestamp::from_second(at)
                .map_err(FixtureError::EvidenceTimestamp)?,
        },
        raw,
        config,
    )?;
    Ok(receipt)
}

fn park(
    store: &mut analyte_store::Store,
    kind: &str,
    reason: &str,
    targets: &[String],
) -> Result<i64, FixtureError> {
    Ok(store.park(&Parked {
        kind: id(kind)?,
        reason: Reason::new(reason).map_err(FixtureError::Text)?,
        targets: targets.iter().map(|t| id(t)).collect::<Result<_, _>>()?,
    })?)
}

fn audit(action: &str, targets: &[String], time: i64, reason: &str) -> Result<Audit, FixtureError> {
    Ok(Audit {
        actor: id(SESSION)?,
        action: id(action)?,
        targets: targets.iter().map(|t| id(t)).collect::<Result<_, _>>()?,
        time,
        reason: Reason::new(reason).map_err(FixtureError::Text)?,
    })
}

/// A store, a server and a signed-in session's cookie header.
async fn started(
    root: &std::path::Path,
) -> Result<(Arc<analyte_store::Handle>, Started, String, String), Error> {
    let bearer = crate::admin::rotate_token(root)?;
    let store = empty_store(root).await?;
    let started = start_overview_server(root, &store, Vec::new(), Vec::new()).await?;
    let cookie = login(&started.3, &bearer).await?;
    Ok((store, started, format!("Cookie: {cookie}\r\n"), bearer))
}

/// Every Needs review page is in the new look: sidebar, the design's
/// stylesheets and no old one, Needs review marked, and the strict headers.
fn assert_new_look(page: &str) {
    headers_present(page);
    assert!(
        page.contains("<nav class=\"sidebar\" aria-label=\"Main\">"),
        "{page}"
    );
    assert!(page.contains("app.css"));
    assert!(!page.contains("admin.css"));
    assert!(page.contains("<a href=\"/parked\" aria-current=\"page\">"));
}

// Owner decision 2026-11-19 (admin restyle b1): Open and Resolved are links
// with real counts, the current one marked with aria-current; open items are
// cards with their action, resolved items are rows saying how each was settled
// and by whom, with the reason; the open list never shows resolved items.
#[tokio::test(flavor = "multi_thread")]
async fn filters_count_open_and_resolved_and_show_each_with_its_own_look() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let ids = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let receipt = uuid::Uuid::from(receive(s, "chemistry", 1_795_046_077)?).to_string();
            let open = park(
                s,
                "mapping_finding",
                "Position { record: 3, field: 3 }: MissingTest",
                std::slice::from_ref(&receipt),
            )?;
            let other = park(
                s,
                "order_version_refused",
                "version 2 is older",
                &["order-9".into()],
            )?;
            let done = park(
                s,
                "conflict",
                "lab app holds different content",
                &["d".into()],
            )?;
            s.resolve(
                done,
                &audit(
                    "parked-resolve",
                    &[done.to_string()],
                    1_795_046_100,
                    "settled with the lab app",
                )?,
            )?;
            Ok((open, other, done))
        })
        .await?;
    let open = request(&host, "/parked", &auth, None).await?;
    assert!(open.starts_with("HTTP/1.1 200"), "{open}");
    assert_new_look(&open);
    assert!(open.contains(concat!(
        "<a class=\"chip\" href=\"/parked?show=open\" aria-current=\"page\">Open <span class=\"chip__n\">2</span></a>",
        "<a class=\"chip\" href=\"/parked?show=resolved\">Resolved <span class=\"chip__n\">1</span></a>"
    )), "{open}");
    assert!(open.contains("<span class=\"nav__count\"><span class=\"sr-only\">, </span>2</span>"));
    assert_eq!(open.matches("<article class=\"card\"").count(), 2);
    assert!(open.contains(&format!("<h2 id=\"rv-{0}\" class=\"review__title\">Item <span class=\"mono\">{0}</span> · chemistry</h2>", ids.0)));
    assert!(open.contains("<span class=\"subtle review__meta\">Arrived <time datetime=\""));
    assert!(
        open.contains("<span class=\"status status--warn\" title=\"mapping_finding\">"),
        "{open}"
    );
    assert!(open.contains("Test code missing</span>"));
    assert!(open.contains(
        "<p class=\"muted text\">The machine sent no test code in record 3, field 3.</p>"
    ));
    assert!(!open.contains("MissingTest"), "no enum names on the list");
    assert!(open.contains("<p class=\"muted text\">Reason: version 2 is older</p>"));
    assert!(
        !open.contains("lab app holds different content"),
        "resolved items stay off the open list"
    );
    let resolved = request(&host, "/parked?show=resolved", &auth, None).await?;
    assert!(resolved.starts_with("HTTP/1.1 200"), "{resolved}");
    assert!(resolved.contains(
        "<a class=\"chip\" href=\"/parked?show=resolved\" aria-current=\"page\">Resolved"
    ));
    assert!(!resolved.contains("<article"));
    assert!(resolved.contains(&format!("<b class=\"row__title\">Lab app reported a conflicting delivery</b> <span class=\"muted\">· Item <span class=\"mono\">{}</span></span>", ids.2)));
    assert!(resolved.contains("Marked resolved by admin session at <time"));
    assert!(resolved.contains(": “settled with the lab app”</span>"));
    assert!(resolved.contains(&format!("href=\"/parked/{}\">History</a>", ids.2)));
    assert!(
        !resolved.contains(SESSION),
        "the session ID is for support only"
    );
    for refused in ["/parked?show=all", "/parked?show=open&limit=5"] {
        assert!(
            request(&host, refused, &auth, None)
                .await?
                .starts_with("HTTP/1.1 400")
        );
    }
    stop(state, stopper, owner, store).await
}

// Owner decision 2026-11-19 (admin restyle b1, ReviewEmpty): nothing open is
// good news with a tick and a link to the resolved items when there are some;
// nothing resolved is only a fact; a page past the last item says so.
#[tokio::test(flavor = "multi_thread")]
async fn empty_filters_say_so_and_point_to_the_other_one() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let none = request(&host, "/parked", &auth, None).await?;
    assert!(none.contains("<span class=\"empty__mark\">"), "{none}");
    assert!(none.contains("<p class=\"empty__title\">Nothing needs review.</p>"));
    assert!(!none.contains("See resolved items"));
    assert!(!none.contains("nav__count"));
    let nothing_resolved = request(&host, "/parked?show=resolved", &auth, None).await?;
    assert!(nothing_resolved.contains("<p class=\"empty__title\">No resolved items yet.</p>"));
    assert!(!nothing_resolved.contains("empty__mark"));
    let done = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let done = park(s, "poll_intent", "no answer", &["cobas".into()])?;
            s.resolve(
                done,
                &audit(
                    "parked-resolve",
                    &[done.to_string()],
                    5,
                    "finished on the machine",
                )?,
            )?;
            park(s, "poll_intent", "no answer", &["cobas".into()])
        })
        .await?;
    let past = request(&host, &format!("/parked?after={done}"), &auth, None).await?;
    assert!(
        past.contains("<p class=\"empty__title\">No more items</p>"),
        "{past}"
    );
    assert!(
        past.contains("<a class=\"link\" href=\"/parked?show=open\">Back to the first page</a>")
    );
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            Ok(s.resolve(
                done,
                &audit("parked-resolve", &[done.to_string()], 6, "also finished")?,
            )?)
        })
        .await?;
    let cleared = request(&host, "/parked", &auth, None).await?;
    assert!(
        cleared.contains(
            "<a class=\"link\" href=\"/parked?show=resolved\">See resolved items (2)</a>"
        ),
        "{cleared}"
    );
    stop(state, stopper, owner, store).await
}

// Owner decisions 2026-11-19 (admin restyle b1, ReviewDetail): the item page
// names the category, where the message came from and when, and its history
// from the stored audit: arrival, each reprocess with who and why, and the
// resolution, with no raw JSON; raw IDs only under "Details for support", and the
// actor reads "admin session".
#[tokio::test(flavor = "multi_thread")]
async fn item_page_shows_the_message_and_its_history_from_the_audit() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let (item, receipt) = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let receipt = uuid::Uuid::from(receive(s, "chemistry", 1_795_046_077)?).to_string();
            let item = park(
                s,
                "mapping_finding",
                "Position { record: 3, field: 3 }: MissingTest",
                std::slice::from_ref(&receipt),
            )?;
            let parked = format!("parked:{item}");
            s.audit(&audit(
                "receipt-reprocess",
                &[
                    receipt.clone(),
                    format!("interpretation:{receipt}:2"),
                    "reprocess:parked".into(),
                    parked.clone(),
                ],
                1_795_046_200,
                "mapping repaired",
            )?)?;
            s.resolve(
                item,
                &audit(
                    "parked-resolve",
                    &[item.to_string()],
                    1_795_046_300,
                    "machine sent it by mistake",
                )?,
            )?;
            Ok((item, receipt))
        })
        .await?;
    let page = request(&host, &format!("/parked/{item}"), &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    assert_new_look(&page);
    for text in [
        format!("<ol class=\"crumbs\"><li><a href=\"/parked\">Needs review</a></li><li>Item <span class=\"mono\">{item}</span></li></ol>").as_str(),
        "<h1>Test code missing</h1>",
        "<p class=\"text\">The machine sent no test code in record 3, field 3.</p>",
        "Reason as stored: Position { record: 3, field: 3 }: MissingTest",
        "<p class=\"page-head__lede\">From chemistry · arrived <time datetime=\"",
        "<span class=\"status status--good\">",
        "Resolved</span>",
        "<h2 id=\"rd-why\" class=\"label\">Why it’s here</h2>",
        "<dt>Message</dt><dd>From chemistry · arrived <time",
        "<span class=\"timeline__text\">Arrived from chemistry<span class=\"timeline__who\">Analyte stored the message</span></span>",
        "<span class=\"timeline__text\">Processed again: reading 2 recorded; it still needs review<span class=\"timeline__who\">admin session · “mapping repaired”</span></span>",
        "<span class=\"timeline__dot timeline__dot--good\"></span><span class=\"timeline__text\">Marked resolved<span class=\"timeline__who\">admin session · “machine sent it by mistake”</span></span>",
        "Details for support</summary><pre>",
        format!("Linked record: {receipt}").as_str(),
        format!("receipt-reprocess by {SESSION}").as_str(),
        "Show the original message</summary>",
    ] {
        assert!(page.contains(text), "{text}\n{page}");
    }
    assert!(
        !page.contains("whattodo"),
        "a resolved item has nothing left to do"
    );
    assert!(!page.contains("\"action\":"), "no raw JSON on the page");
    // Every sight of the message ID is under "Details for support", in a form field or in a link.
    for (at, _) in page.match_indices(&receipt) {
        let before = page.get(..at).unwrap_or_default();
        assert!(
            ["Linked record: ", "value=\"", "/receipts/"]
                .iter()
                .any(|p| before.ends_with(p)),
            "{}",
            page.get(at.saturating_sub(60)..at).unwrap_or_default()
        );
    }
    stop(state, stopper, owner, store).await
}

// Owner review 2026-11-20 (PR 275): a reason Analyte wrote reads as a plain
// sentence under a specific title, on the list and on the item page; the
// stored text, and an HTTP status, are under "Details for support" only; a reason
// with no known shape shows as stored, under the category's words.
#[tokio::test(flavor = "multi_thread")]
async fn known_reasons_read_plainly_and_keep_the_stored_text_for_support() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let delivery = uuid::Uuid::now_v7().to_string();
    let (unknown_test, conflict, status, free) = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            let receipt = uuid::Uuid::from(receive(s, "chemistry", 1_795_046_077)?).to_string();
            Ok((
                park(
                    s,
                    "mapping_finding",
                    "Position { record: 6, field: 3 }: MissingTest",
                    std::slice::from_ref(&receipt),
                )?,
                park(
                    s,
                    "conflict",
                    analyte_sink::CONFLICT_REASON,
                    std::slice::from_ref(&delivery),
                )?,
                park(s, "parked", "unexpected HTTP status 418", &[delivery])?,
                park(
                    s,
                    "interpretation_failure",
                    "HL7 syntax: segment too long",
                    &[receipt],
                )?,
            ))
        })
        .await?;
    let list = request(&host, "/parked", &auth, None).await?;
    for text in [
        "<span class=\"status status--warn\" title=\"mapping_finding\">",
        "Test code missing</span>",
        "<p class=\"muted text\">The machine sent no test code in record 6, field 3.</p>",
        "<span class=\"status status--bad\" title=\"conflict\">",
        "Lab app already has different content</span>",
        "<p class=\"muted text\">The lab app already holds different content under this delivery’s ID.</p>",
        "Unexpected answer from the lab app</span>",
        "<p class=\"muted text\">The lab app gave an answer that the lab app contract does not define for a delivery.</p>",
        "Could not read message</span>",
        "<p class=\"muted text\">Reason: HL7 syntax: segment too long</p>",
    ] {
        assert!(list.contains(text), "{text}\n{list}");
    }
    for hidden in ["MissingTest", "HTTP 418", "HTTP 409", "identity conflict"] {
        assert!(!list.contains(hidden), "{hidden} is for support only");
    }
    let item = request(&host, &format!("/parked/{unknown_test}"), &auth, None).await?;
    assert!(item.contains("<h1>Test code missing</h1>"));
    assert!(
        item.contains("<p class=\"text\">The machine sent no test code in record 6, field 3.</p>")
    );
    // The enum name appears once, in the stored reason under "Details for support".
    assert_eq!(item.matches("MissingTest").count(), 1, "{item}");
    let support = item
        .find("Details for support</summary>")
        .unwrap_or(usize::MAX);
    assert!(
        item.find("Reason as stored: Position { record: 6, field: 3 }: MissingTest")
            > Some(support)
    );
    for (id, title, lines) in [
        (
            conflict,
            "Lab app already has different content",
            [
                format!("Reason as stored: {}", analyte_sink::CONFLICT_REASON),
                "Lab app answer: HTTP 409".to_owned(),
            ],
        ),
        (
            status,
            "Unexpected answer from the lab app",
            [
                "Reason as stored: unexpected HTTP status 418".to_owned(),
                "Lab app answer: HTTP 418".to_owned(),
            ],
        ),
    ] {
        let page = request(&host, &format!("/parked/{id}"), &auth, None).await?;
        assert!(page.contains(&format!("<h1>{title}</h1>")), "{page}");
        let support = page
            .find("Details for support</summary>")
            .unwrap_or(usize::MAX);
        for line in lines {
            assert!(page.find(&line) > Some(support), "{line}\n{page}");
        }
        // Above "Details for support" the page names no HTTP status (the response's own status line aside).
        let why = page.get(..support).unwrap_or_default();
        assert!(
            !why.contains("HTTP 4") && !why.contains("Lab app answer"),
            "the status code stays under support"
        );
    }
    let stored = request(&host, &format!("/parked/{free}"), &auth, None).await?;
    assert!(stored.contains("<h1>Could not read message</h1>"));
    assert!(stored.contains("<p class=\"text\">Reason: HL7 syntax: segment too long</p>"));
    stop(state, stopper, owner, store).await
}

/// One open item; one resolved yesterday; eleven resolved today a minute
/// apart, from midnight (`from`); and the newest, settled by Process again an
/// hour after midnight. Returns the open item, yesterday's, today's in order
/// and the reprocessed one.
fn resolved_today(
    s: &mut analyte_store::Store,
    from: i64,
) -> Result<(i64, i64, Vec<i64>, i64), FixtureError> {
    let receipt = uuid::Uuid::from(receive(s, "chemistry", from)?).to_string();
    let open = park(
        s,
        "mapping_finding",
        "Position { record: 6, field: 3 }: MissingTest",
        std::slice::from_ref(&receipt),
    )?;
    let yesterday = park(s, "poll_intent", "no answer", &["cobas".into()])?;
    s.resolve(
        yesterday,
        &audit(
            "parked-resolve",
            &[yesterday.to_string()],
            from.saturating_sub(1),
            "late shift",
        )?,
    )?;
    let mut today = Vec::new();
    for minute in 0..11_i64 {
        let id = park(
            s,
            "mapping_finding",
            "Position { record: 2, field: 0 }: MissingSample",
            std::slice::from_ref(&receipt),
        )?;
        s.resolve(
            id,
            &audit(
                "parked-resolve",
                &[id.to_string()],
                from.saturating_add(minute.saturating_mul(60)),
                "checked",
            )?,
        )?;
        today.push(id);
    }
    // The newest one was settled by Process again.
    let reprocessed = park(
        s,
        "mapping_finding",
        "Position { record: 6, field: 3 }: MissingTest",
        std::slice::from_ref(&receipt),
    )?;
    s.resolve(
        reprocessed,
        &audit(
            "receipt-reprocess",
            &[
                receipt.clone(),
                format!("interpretation:{receipt}:2"),
                "reprocess:succeeded".into(),
                format!("parked:{reprocessed}"),
            ],
            from.saturating_add(3_600),
            "test added to the mapping",
        )?,
    )?;
    Ok((open, yesterday, today, reprocessed))
}

// Owner review 2026-11-20 (PR 275, Review mockup): the first page of open
// items lists the items resolved today under them, newest first and at most
// ten, each with its title, machine, how and when it was settled, who did it
// as recorded and the reason, and a History link; yesterday's are left out.
// The Resolved filter, a later page and an empty Open filter stay as they were.
#[tokio::test(flavor = "multi_thread")]
async fn the_open_list_shows_items_resolved_today_newest_first() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let (from, _) = crate::admin::view::format::today_span(jiff::Timestamp::now())
        .ok_or(FixtureError::Corruption)?;
    let ids = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| resolved_today(s, from))
        .await?;
    let (open, yesterday, today, reprocessed) = ids;
    let list = request(&host, "/parked", &auth, None).await?;
    assert_eq!(list.matches("<article class=\"card\"").count(), 1, "{list}");
    let section = list
        .find("<section class=\"section\" aria-labelledby=\"rv-done\">")
        .ok_or(FixtureError::Corruption)?;
    let rows = list.get(section..).unwrap_or_default();
    assert!(rows.contains("<h2 id=\"rv-done\" class=\"label\">Resolved today</h2>"));
    assert_eq!(
        rows.matches("<li class=\"row\">").count(),
        10,
        "at most ten"
    );
    let order: Vec<usize> = [reprocessed, today[10], today[1]]
        .iter()
        .map(|id| {
            rows.find(&format!("href=\"/parked/{id}\">History</a>"))
                .unwrap_or(usize::MAX)
        })
        .collect();
    assert!(
        order[0] < order[1] && order[1] < order[2],
        "newest first: {order:?}"
    );
    for left_out in [today[0], yesterday, open] {
        assert!(!rows.contains(&format!("href=\"/parked/{left_out}\">History</a>")));
    }
    for text in [
        format!(
            "<b class=\"row__title\">Test code missing</b> <span class=\"muted\">· Item <span class=\"mono\">{reprocessed}</span> · chemistry</span>"
        ),
        "Processed again by admin session at <time".to_owned(),
        ": “test added to the mapping”</span>".to_owned(),
        format!(
            "<b class=\"row__title\">Sample ID missing</b> <span class=\"muted\">· Item <span class=\"mono\">{}</span> · chemistry</span>",
            today[10]
        ),
        "Marked resolved by admin session at <time".to_owned(),
        ": “checked”</span>".to_owned(),
    ] {
        assert!(rows.contains(&text), "{text}\n{rows}");
    }
    assert!(
        !rows.contains("reading 2"),
        "today's rows say only the action"
    );
    assert!(
        !list.contains(SESSION),
        "the session ID is for support only"
    );
    // The Resolved filter keeps its own rows, every one, and no heading.
    let resolved = request(&host, "/parked?show=resolved", &auth, None).await?;
    assert!(!resolved.contains("Resolved today"));
    assert_eq!(resolved.matches("<li class=\"row\">").count(), 13);
    assert!(resolved.contains("Processed again: reading 2 recorded by admin session at <time"));
    // A later page of open items, and an empty Open filter, list none.
    let later = request(&host, &format!("/parked?after={open}"), &auth, None).await?;
    assert!(!later.contains("Resolved today"));
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            Ok(s.resolve(
                open,
                &audit(
                    "parked-resolve",
                    &[open.to_string()],
                    from.saturating_add(60),
                    "done",
                )?,
            )?)
        })
        .await?;
    let cleared = request(&host, "/parked", &auth, None).await?;
    assert!(cleared.contains("Nothing needs review."));
    assert!(!cleared.contains("Resolved today"));
    stop(state, stopper, owner, store).await
}

// Owner request 2026-11-19 (admin restyle b1): an open item says what to do and
// that its history fills in later; an item linked to more than one page of
// records lists them a page at a time with Next page; a category this version
// does not know shows as stored, with Process again and the general advice.
#[tokio::test(flavor = "multi_thread")]
async fn open_items_page_their_linked_records_and_unknown_categories_show_as_stored()
-> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let targets: Vec<String> = (0..51).map(|n| format!("transfer-{n}")).collect();
            park(s, "future_kind", "something new", &targets)
        })
        .await?;
    let first = request(&host, &format!("/parked/{item}"), &auth, None).await?;
    for text in [
        "<h1>future_kind</h1>",
        "<span class=\"status status--warn\">",
        "Open</span>",
        "Read the reason above and open the item for its linked records.",
        format!("<a class=\"btn btn--primary\" href=\"/parked/{item}/reprocess\">Process again</a><a class=\"btn btn--outline\" href=\"/parked/{item}/resolve\">Mark resolved</a>").as_str(),
        "<p class=\"subtle note\">Linked records 1 to 50 of 51.</p>",
        format!("rel=\"next\" href=\"/parked/{item}?after=50\">Next page</a>").as_str(),
        "When someone resolves this item or processes it again, the time, the action and the reason appear here.",
        "<dt>Linked record</dt><dd>Not a stored message; its ID is under Details for support</dd>",
    ] {
        assert!(first.contains(text), "{text}\n{first}");
    }
    assert_eq!(first.matches("<dt>Linked record</dt>").count(), 50);
    let second = request(&host, &format!("/parked/{item}?after=50"), &auth, None).await?;
    assert_eq!(second.matches("<dt>Linked record</dt>").count(), 1);
    assert!(second.contains("Linked records 51 to 51 of 51."));
    assert!(second.contains("Linked record: transfer-50"));
    assert!(!second.contains("Next page"));
    let list = request(&host, "/parked", &auth, None).await?;
    assert!(
        list.contains("<span class=\"status status--warn\"><svg"),
        "an unknown kind has no hover"
    );
    assert!(list.contains("</svg>future_kind</span>"));
    stop(state, stopper, owner, store).await
}

// AGENTS.md safety rule 3 and ADR 0013: a message removed by data keeping reads
// so on the item and is not offered; asking for it anyway says it is gone; the
// pieces as they arrived may still be asked for.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_removed_by_data_keeping_reads_so_and_is_gone() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let receipt = uuid::Uuid::now_v7().to_string();
    let target = receipt.clone();
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            park(s, "mapping_finding", "MissingTest", &[target])
        })
        .await?;
    rusqlite::Connection::open(directory.path().join("analyte.db"))?.execute(
        "INSERT INTO retention_tombstones VALUES (json_quote(?1),'m','d','1',zeroblob(32),0,0,0)",
        [&receipt],
    )?;
    let page = request(&host, &format!("/parked/{item}"), &auth, None).await?;
    assert!(
        page.contains("<dt>Message</dt><dd>Removed by data keeping</dd>"),
        "{page}"
    );
    assert!(page.contains("Message removed by data keeping"));
    assert!(
        !page.contains("value=\"receipt\""),
        "the stored message is not offered"
    );
    assert!(
        page.contains("name=\"source\" value=\"frames\">Show the pieces as they arrived</button>")
    );
    let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let form = format!(
        "csrf={}&target={receipt}&source=receipt&after=0",
        encode(&csrf(&page)?)
    );
    let gone = request(&host, &format!("/parked/{item}/raw"), &post, Some(&form)).await?;
    assert!(gone.starts_with("HTTP/1.1 410"), "{gone}");
    assert_new_look(&gone);
    assert!(
        gone.contains("<p class=\"alert__title\">Data keeping removed the original message</p>")
    );
    let nothing = request(
        &host,
        &format!("/parked/{item}/raw"),
        &post,
        Some(&form.rewrite("source=receipt", "source=frames")),
    )
    .await?;
    assert!(nothing.starts_with("HTTP/1.1 404"), "{nothing}");
    assert!(nothing.contains("<h1>Nothing more to show</h1>"));
    let reprocess = request(&host, &format!("/parked/{item}/reprocess"), &auth, None).await?;
    assert!(reprocess.starts_with("HTTP/1.1 410"), "{reprocess}");
    stop(state, stopper, owner, store).await
}

// Owner decision 2026-11-19 (admin restyle b1, ReviewResolve): the confirm
// pages are the design's dialog over the item page: a real form with the form
// token, Cancel back to the item and a required reason of at most 256
// characters; a refused reason opens the dialog again with what was typed and
// changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn dialogs_draw_over_the_item_and_ask_again_for_a_refused_reason() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            park(s, "conflict", "different content", &["d".into()])
        })
        .await?;
    let path = format!("/parked/{item}/resolve");
    let dialog = request(&host, &path, &auth, None).await?;
    assert!(dialog.starts_with("HTTP/1.1 200"), "{dialog}");
    assert_new_look(&dialog);
    for text in [
        "<h1>Lab app reported a conflicting delivery</h1>",
        "<div class=\"scrim\">",
        format!("<form class=\"dialog\" role=\"dialog\" aria-modal=\"true\" aria-labelledby=\"dlg-title\" method=\"post\" action=\"{path}\">").as_str(),
        "<input type=\"hidden\" name=\"confirm\" value=\"yes\">",
        "<textarea id=\"dlg-reason\" class=\"textarea\" name=\"reason\" required maxlength=\"256\" autofocus",
        format!("<a class=\"btn btn--outline\" href=\"/parked/{item}\">Cancel</a><button type=\"submit\" class=\"btn btn--primary\">Mark resolved</button>").as_str(),
        "Required, up to 256 characters. Recorded in the history with the time.",
    ] {
        assert!(dialog.contains(text), "{text}\n{dialog}");
    }
    assert!(!dialog.contains("your name"));
    // The conflicting delivery is never offered Send again: no resend link
    // and no such button. The words appear only in the always-shown
    // "What each choice means" help.
    assert!(!dialog.contains("/resend"), "{dialog}");
    assert!(!dialog.contains(">Send again</a>"), "{dialog}");
    assert!(!dialog.contains(">Send again</button>"), "{dialog}");
    assert_eq!(dialog.matches("Send again").count(), 1, "{dialog}");
    let token = encode(&csrf(&dialog)?);
    let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    for (reason, error) in [
        ("+++", "Write a reason. It is recorded in the history."),
        (&"x".repeat(257), "Keep the reason to 256 characters."),
    ] {
        let again = request(
            &host,
            &path,
            &post,
            Some(&format!("csrf={token}&confirm=yes&reason={reason}")),
        )
        .await?;
        assert!(again.starts_with("HTTP/1.1 400"), "{again}");
        assert!(again.contains("<span id=\"dlg-error\" class=\"field__error\">"));
        assert!(again.contains(error), "{error}\n{again}");
        assert!(again.contains("aria-invalid=\"true\" aria-describedby=\"dlg-error\""));
    }
    let typed = request(
        &host,
        &path,
        &post,
        Some(&format!(
            "csrf={token}&confirm=yes&reason={}",
            "y".repeat(257)
        )),
    )
    .await?;
    assert!(
        typed.contains(&format!(">{}</textarea>", "y".repeat(257))),
        "what was typed comes back"
    );
    let open = store
        .request::<_, FixtureError>(fixture_deadline()?, move |s| {
            Ok(s.parked_item(item)?
                .ok_or(FixtureError::Corruption)?
                .resolution
                .is_none())
        })
        .await?;
    assert!(open, "a refused reason changes nothing");
    let reprocess = request(&host, &format!("/parked/{item}/reprocess"), &auth, None).await?;
    assert!(reprocess.starts_with("HTTP/1.1 409"), "{reprocess}");
    assert!(reprocess.contains(
        "<p class=\"alert__title\">This item is not linked to exactly one stored message</p>"
    ));
    stop(state, stopper, owner, store).await
}

// Plan §11 and owner decision 2026-11-19 (admin restyle b1): refusals keep
// their status and come in the new look with the way back: an unknown item, an
// item already resolved, a dialog without a signed-in session; with the admin
// key alone the original message asks to sign in.
#[tokio::test(flavor = "multi_thread")]
async fn refusals_keep_their_status_in_the_new_look() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, bearer) = started(directory.path()).await?;
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let item = park(s, "conflict", "different content", &["d".into()])?;
            s.resolve(
                item,
                &audit("parked-resolve", &[item.to_string()], 5, "settled")?,
            )?;
            Ok(item)
        })
        .await?;
    let missing = request(&host, "/parked/999", &auth, None).await?;
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert_new_look(&missing);
    assert!(missing.contains("<h1>Item not found</h1>"));
    assert!(
        missing.contains("<a class=\"btn btn--primary\" href=\"/parked\">Back to Needs review</a>")
    );
    for action in ["resolve", "reprocess"] {
        let again = request(&host, &format!("/parked/{item}/{action}"), &auth, None).await?;
        assert!(again.starts_with("HTTP/1.1 409"), "{again}");
        assert!(again.contains("<h1>Already resolved</h1>"));
        assert!(again.contains(&format!(
            "<a class=\"btn btn--primary\" href=\"/parked/{item}\">Open the item</a>"
        )));
    }
    let key = format!("Authorization: Bearer {bearer}\r\n");
    let unsigned = request(&host, &format!("/parked/{item}/resolve"), &key, None).await?;
    assert!(unsigned.starts_with("HTTP/1.1 403"), "{unsigned}");
    assert_new_look(&unsigned);
    assert!(unsigned.contains("<h1>Sign in first</h1>"));
    let page = request(&host, &format!("/parked/{item}"), &key, None).await?;
    assert!(
        page.contains(
            "<a class=\"link\" href=\"/login\">Sign in</a> to show the original message."
        )
    );
    assert!(!page.contains("name=\"source\""));
    stop(state, stopper, owner, store).await
}

// Owner request 2026-11-19 (admin restyle b1): when the store cannot be read,
// or reads take longer than the 1.5 s every restyled page waits, Needs review
// answers in its own look and keeps the reason for support.
#[tokio::test(flavor = "multi_thread")]
async fn store_failures_and_slow_reads_answer_in_the_new_look() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let blocker = hold_store(&store, PAST_READ_BUDGET).await?;
    let slow = request(&host, "/parked", &auth, None).await?;
    assert!(slow.starts_with("HTTP/1.1 504"), "{slow}");
    assert_new_look(&slow);
    assert!(slow.contains("Analyte did not finish reading within 1.5 seconds. Reload the page;"));
    assert!(slow.contains("admin Needs review deadline of 1500 ms exceeded"));
    blocker.await??;
    stop(state, stopper, owner, store).await?;
    let directory = crate::admin::private_tempdir()?;
    let bearer = crate::admin::rotate_token(directory.path())?;
    let mut closed = Arc::try_unwrap(empty_store(directory.path()).await?).map_err(|_| "shared")?;
    closed.shutdown(fixture_deadline()?).await?;
    let store = Arc::new(closed);
    let (state, stopper, owner, host) =
        start_overview_server(directory.path(), &store, Vec::new(), Vec::new()).await?;
    let key = format!("Authorization: Bearer {bearer}\r\n");
    for path in ["/parked", "/parked/1"] {
        let failed = request(&host, path, &key, None).await?;
        assert!(failed.starts_with("HTTP/1.1 503"), "{failed}");
        assert_new_look(&failed);
        assert!(failed.contains(
            "<p class=\"alert__title\">Analyte could not read its stored data just now</p>"
        ));
        assert!(failed.contains("Details for support</summary><pre>"));
        assert!(!failed.contains("nav__count"));
    }
    drop(state);
    stopper.send_replace(true);
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, owner).await???;
    Ok(())
}

// AGENTS.md rule 7 (bound every wait): a Mark resolved that outlasts the
// request budget gets the "This took too long" page, which warns that the
// action may already be saved; it used to be cut off with the connection,
// whose own 2 s lifetime ended first. Only setup writes get the longer
// write budget, which covers the store's own deadline.
#[tokio::test(flavor = "multi_thread")]
async fn a_slow_action_answers_before_the_connection_closes() -> Result<(), Error> {
    assert!(crate::admin::view::READ_BUDGET < REQUEST_BUDGET);
    assert!(REQUEST_BUDGET.saturating_add(ANSWER_TIME) <= listener::LIFETIME);
    assert!(listener::LIFETIME < WRITE_BUDGET);
    assert!(crate::STORE_OPERATION_BUDGET < WRITE_BUDGET);
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            park(s, "conflict", "different content", &["d".into()])
        })
        .await?;
    let page = request(&host, &format!("/parked/{item}"), &auth, None).await?;
    let form = format!("csrf={}&confirm=yes&reason=settled", encode(&csrf(&page)?));
    let (started, holding) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let deadline = fixture_deadline()?;
    let busy = store.clone();
    let blocker = tokio::spawn(async move {
        busy.request::<_, FixtureError>(deadline, move |_| {
            let _ = started.send(());
            Ok(released.recv_timeout(analyte_fixtures::STORE_BUDGET))
        })
        .await
    });
    tokio::time::timeout(analyte_fixtures::HANG_GUARD, holding).await??;
    let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let slow = request(
        &host,
        &format!("/parked/{item}/resolve"),
        &post,
        Some(&form),
    )
    .await?;
    assert!(slow.starts_with("HTTP/1.1 504"), "{slow:?}");
    assert!(slow.contains("an action may already have been saved."));
    release.send(())?;
    blocker.await???;
    stop(state, stopper, owner, store).await
}

// ADR 0035: signed-in operators compare original values and confirm one decision.
#[tokio::test(flavor = "multi_thread")]
async fn measurement_comparison_requires_sign_in_and_confirms_once() -> Result<(), Error> {
    use analyte_model::{IdentityStatus, SourceText};
    use analyte_store::{Interpretation, Reading, ReconcileAction, ReconcileInput};
    const VALUE: &str = "synthetic-held-payload-6281";
    const SAMPLE: &str = "000123-SYN-HELD-COMPARISON";
    let logs = crate::runtime::log_capture::init();
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, bearer) = started(directory.path()).await?;
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            for (second, status) in [(5, "early"), (6, "held")] {
                let receipt = receive(s, "chemistry", second)?;
                let retained = s.receipt(receipt)?.ok_or(FixtureError::UnknownReceipt)?;
                let mut result = machines::result(receipt, Some(SAMPLE))
                    .map_err(|_| FixtureError::Corruption)?;
                let IdentityStatus::Resolved(identity) = &mut result.identity else {
                    return Err(FixtureError::Corruption);
                };
                identity.observation.run = SourceText::new(&format!("{}:{SAMPLE}", SAMPLE.len()))?;
                let mut parts = result.parts.as_slice().to_vec();
                let part = parts.first_mut().ok_or(FixtureError::Corruption)?;
                part.source_status = SourceText::new(status)?;
                part.values =
                    analyte_model::EvidenceList::new(vec![analyte_model::ResultValue::new(
                        SourceText::new(VALUE)?,
                        analyte_model::ValueInterpretation::Text,
                    )])
                    .map_err(|_| FixtureError::Corruption)?;
                result.parts = analyte_model::NonEmptyEvidenceList::new(
                    analyte_model::EvidenceList::new(parts)
                        .map_err(|_| FixtureError::Corruption)?,
                )
                .map_err(|_| FixtureError::Corruption)?;
                s.reconcile(
                    ReconcileAction::Interpret(receipt),
                    Interpretation {
                        driver: retained.metadata.driver,
                        config_version: retained.metadata.config_version,
                        config: retained.config,
                        outcome: Reading::Results(vec![result]),
                        departures: None,
                    },
                    ReconcileInput::standard(&[false], &[]),
                    |_, _| Ok(Vec::new()),
                )?;
            }
            Ok(s.parked(0)?.first().ok_or(FixtureError::Corruption)?.id)
        })
        .await?;
    let path = format!("/parked/{item}/measurement");
    let bearer = format!("Authorization: Bearer {bearer}\r\n");
    let unsigned = request(&host, &path, &bearer, None).await?;
    assert!(!unsigned.contains(SAMPLE));
    let page = request(&host, &path, &auth, None).await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    for value in [
        "Held measurement",
        "Earlier measurement",
        SAMPLE,
        VALUE,
        "This is a resend",
        "This is a rerun",
    ] {
        assert!(page.contains(value), "{value}");
    }
    let snapshot = page
        .split_once("name=\"snapshot\" value=\"")
        .and_then(|(_, rest)| rest.split('"').next())
        .ok_or(FixtureError::Corruption)?;
    let token = encode(&csrf(&page)?);
    let post = format!("{auth}Content-Type: application/x-www-form-urlencoded\r\n");
    let form = format!("csrf={token}&snapshot={snapshot}&decision=resend&reason=Verified");
    let confirmed = request(&host, &path, &post, Some(&form)).await?;
    assert!(confirmed.starts_with("HTTP/1.1 303"), "{confirmed}");
    let repeated = request(&host, &path, &post, Some(&form)).await?;
    assert!(
        repeated.contains("measurement comparison changed"),
        "{repeated}"
    );
    let resolved = request(&host, &format!("/parked/{item}"), &auth, None).await?;
    assert!(resolved.contains("Recorded as a resend"));
    assert!(resolved.contains("Verified"));
    stop(state, stopper, owner, store).await?;
    let logs = logs.lock().map_err(|_| FixtureError::Corruption)?;
    assert!(!String::from_utf8_lossy(&logs).contains(VALUE));
    Ok(())
}

// Owner request 2026-11-27 (ADRs 0035 and 0038): the Needs review list and an
// item page explain each choice beside the decisions, in the page's language.
#[tokio::test(flavor = "multi_thread")]
async fn choices_help_renders_beside_the_decisions_in_english_and_german() -> Result<(), Error> {
    let directory = crate::admin::private_tempdir()?;
    let (store, (state, stopper, owner, host), auth, _) = started(directory.path()).await?;
    let item = store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            let receipt = uuid::Uuid::from(receive(s, "chemistry", 1_795_046_077)?).to_string();
            park(
                s,
                "result_reconciliation",
                "result reconciliation: Conflict",
                std::slice::from_ref(&receipt),
            )
        })
        .await?;
    let german = format!("{auth}Accept-Language: de-AT, en;q=0.5\r\n");
    for path in ["/parked".to_owned(), format!("/parked/{item}")] {
        let page = request(&host, &path, &auth, None).await?;
        assert!(page.starts_with("HTTP/1.1 200"), "{path}: {page}");
        assert_eq!(
            page.matches("What each choice means</summary>").count(),
            1,
            "{path}"
        );
        for shown in [
            "<dt>Rerun</dt><dd>The same sample and test were measured again.</dd>",
            "<dt>This is a rerun</dt><dd>Sends the held result to the lab app once, as a new result. The earlier result stays; the lab app decides which value to report.</dd>",
            "<dt>Send again</dt><dd>Queues the same delivery for the lab app again, unchanged and with the same ID. The item stays open until someone marks it resolved.</dd>",
            "An item that nobody decides on stays here. Analyte never sends or deletes it by itself.",
        ] {
            assert!(page.contains(shown), "{path}: {shown}");
        }
        let page = request(&host, &path, &german, None).await?;
        assert!(page.starts_with("HTTP/1.1 200"), "{path}: {page}");
        assert!(page.contains("<html lang=\"de\""), "{path}");
        assert_eq!(
            page.matches("Was jede Wahl bedeutet</summary>").count(),
            1,
            "{path}"
        );
        for shown in [
            "<dt>Wiederholungsmessung</dt><dd>Dieselbe Probe und derselbe Test wurden noch einmal gemessen.</dd>",
            "<dt>Dies ist eine Wiederholungsmessung</dt><dd>Sendet das zurückgehaltene Ergebnis einmal als neues Ergebnis an die Labor-App.",
            "Ein Eintrag, über den niemand entscheidet, bleibt hier.",
        ] {
            assert!(page.contains(shown), "{path}: {shown}");
        }
        assert!(!page.contains("What each choice means"), "{path}");
    }
    stop(state, stopper, owner, store).await
}
