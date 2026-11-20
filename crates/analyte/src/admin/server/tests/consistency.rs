//! Tests that the admin pages agree with each other: each delivery and order state reads the same on every page that shows it, every signed-in page's sidebar shows the Needs review count, and an order item's notes show only after the audited view.
use super::machines::AT;
use super::more::Scene;
use super::*;
use crate::admin::view::format::{StateLook, attempt_look, delivery_look, order_look};
use analyte_store::{AttemptOutcome, DeliveryState, Parked};

/// Every delivery state with its stored spelling.
const DELIVERIES: [(DeliveryState, &str); 6] = [
    (DeliveryState::Pending, "pending"),
    (DeliveryState::InFlight, "in_flight"),
    (DeliveryState::Done, "done"),
    (DeliveryState::Parked, "parked"),
    (DeliveryState::Paused, "paused"),
    (DeliveryState::Conflict, "conflict"),
];

/// Every order state with its stored spelling.
const ORDERS: [(OrderState, &str); 9] = [
    (OrderState::Requested, "Requested"),
    (OrderState::Sending, "Sending"),
    (OrderState::Sent, "Sent"),
    (OrderState::Accepted, "Accepted"),
    (OrderState::Done, "Done"),
    (OrderState::Rejected, "Rejected"),
    (OrderState::CancelRequested, "CancelRequested"),
    (OrderState::CancelConfirmed, "CancelConfirmed"),
    (OrderState::Uncertain, "Uncertain"),
];

/// Every way a delivery try ends, as the store keeps it.
const ATTEMPTS: [(AttemptOutcome, &str); 6] = [
    (AttemptOutcome::Delivered, "\"Done\""),
    (AttemptOutcome::RetryAt(1), "{\"RetryAt\":1}"),
    (AttemptOutcome::Interrupted, "\"uncertain (interrupted)\""),
    (AttemptOutcome::Rejected, "{\"Parked\":\"HTTP 422\"}"),
    (AttemptOutcome::Paused, "{\"Paused\":\"keys missing\"}"),
    (AttemptOutcome::Conflict, "{\"Conflict\":\"HTTP 409\"}"),
];

/// Whether `page` draws `look` as a status: its word right after its own
/// icon, inside a status of its tone.
fn shows(page: &str, look: StateLook) -> bool {
    let drawn = format!("<path d=\"{}\"/></svg>{}", look.icon.path(), look.word);
    let tone = format!("<span class=\"status status--{}\"", look.tone.class());
    page.match_indices(&drawn).any(|(at, _)| {
        page.get(..at)
            .and_then(|before| before.rfind("<span class=\"status "))
            .and_then(|start| page.get(start..at))
            .is_some_and(|span| span.starts_with(&tone) && !span.contains("</span>"))
    })
}

/// The icon a line or row draws for `look`, in its tone: Activity's lines
/// with the word a screen reader says, and the Overview's rows.
fn icon(look: StateLook, label: Option<&str>) -> String {
    let path = look.icon.path();
    match label {
        Some(label) => format!(
            "<svg class=\"icon tone-{}\" viewBox=\"0 0 24 24\" role=\"img\" aria-label=\"{label}\"><path d=\"{path}\"/></svg>",
            look.tone.class()
        ),
        None => format!(
            "<svg class=\"icon tone-{}\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"{path}\"/></svg>",
            look.tone.class()
        ),
    }
}

/// `words` with its first letter in lower case, as a sentence goes on.
fn lower_first(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

// Owner decisions 2026-11-20 (review items 7 and B5): one tone, icon and word
// per state, from one table, on every page. A delivery the lab app rejected
// is a problem (red); an order sent and not yet accepted is waiting (grey).
#[test]
fn each_state_has_one_tone_icon_and_word() {
    use crate::admin::view::{Tone, icon::Icon};
    let deliveries = [
        (Tone::Neutral, Icon::Waiting, "Waiting to be sent"),
        (Tone::Neutral, Icon::Send, "Sending"),
        (Tone::Good, Icon::Ok, "Delivered"),
        (Tone::Problem, Icon::Problem, "Rejected by the lab app"),
        (
            Tone::Problem,
            Icon::Problem,
            "Paused: lab app settings need repair",
        ),
        (Tone::Problem, Icon::Problem, "Lab app reports a conflict"),
    ];
    for ((state, _), (tone, icon, word)) in DELIVERIES.iter().zip(deliveries) {
        assert_eq!(
            delivery_look(*state),
            StateLook { tone, icon, word },
            "{state}"
        );
    }
    let orders = [
        (Tone::Neutral, Icon::Waiting, "Not sent yet"),
        (Tone::Neutral, Icon::Send, "Sending"),
        (Tone::Neutral, Icon::Waiting, "Sent, not yet accepted"),
        (Tone::Good, Icon::Check, "Accepted by the machine"),
        (Tone::Good, Icon::Ok, "Done"),
        (Tone::Problem, Icon::Problem, "Rejected by the machine"),
        (Tone::Neutral, Icon::Waiting, "Cancel requested"),
        (Tone::Neutral, Icon::Cancelled, "Cancelled"),
        (
            Tone::Attention,
            Icon::Unknown,
            "Outcome unknown: check the machine",
        ),
    ];
    for ((state, _), (tone, icon, word)) in ORDERS.iter().zip(orders) {
        assert_eq!(
            order_look(*state),
            StateLook { tone, icon, word },
            "{state}"
        );
    }
    // A try that ended a delivery reads as the state it left; a retry is a
    // look of its own.
    let retry = StateLook {
        tone: Tone::Attention,
        icon: Icon::Retry,
        word: "Will retry",
    };
    for (outcome, look) in [
        (
            AttemptOutcome::Delivered,
            delivery_look(DeliveryState::Done),
        ),
        (AttemptOutcome::RetryAt(1), retry),
        (AttemptOutcome::Interrupted, retry),
        (
            AttemptOutcome::Rejected,
            delivery_look(DeliveryState::Parked),
        ),
        (AttemptOutcome::Paused, delivery_look(DeliveryState::Paused)),
        (
            AttemptOutcome::Conflict,
            delivery_look(DeliveryState::Conflict),
        ),
    ] {
        assert_eq!(attempt_look(outcome), look, "{outcome:?}");
    }
}

// Owner decisions 2026-11-20 (review B5): every delivery state reads the same
// on Deliveries and on its machine's page, and every delivery try in Activity
// takes the look of the state it left.
#[tokio::test(flavor = "multi_thread")]
async fn each_delivery_state_reads_the_same_on_every_page() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let rejected = scene.seeded.rejected.ok_or("rejected delivery")?;
    let key = serde_json::to_string(&rejected.to_string())?;
    for (state, stored) in DELIVERIES {
        rusqlite::Connection::open(scene.database())?.execute(
            "UPDATE outbox SET state=?1 WHERE id=?2",
            rusqlite::params![stored, key],
        )?;
        let look = delivery_look(state);
        let deliveries = scene.get("/deliveries").await?;
        assert!(
            shows(&deliveries, look),
            "Deliveries, {state}: {deliveries}"
        );
        // The chemistry analyzer's message has this one delivery.
        let machine = scene.get("/machines/chemistry-analyzer").await?;
        assert!(shows(&machine, look), "machine page, {state}: {machine}");
    }
    let db = rusqlite::Connection::open(scene.database())?;
    for (_, stored) in ATTEMPTS {
        db.execute(
            "INSERT INTO attempts(delivery,outcome,time) VALUES (?1,?2,?3)",
            rusqlite::params![key, stored, AT - 60],
        )?;
    }
    drop(db);
    let activity = scene.get("/activity").await?;
    for (outcome, _) in ATTEMPTS {
        let look = attempt_look(outcome);
        assert!(
            activity.contains(&icon(look, Some(look.word))),
            "Activity, {outcome:?}: {activity}"
        );
    }
    scene.stop().await
}

// Owner decisions 2026-11-20 (review B5): every order state reads the same on
// Machine orders, on the order item's page and in Activity; the Overview's
// Orders card draws unknown outcomes and cancels in their states' looks.
#[tokio::test(flavor = "multi_thread")]
async fn each_order_state_reads_the_same_on_every_page() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let overview = scene.get("/").await?;
    for state in [OrderState::Uncertain, OrderState::CancelConfirmed] {
        assert!(
            overview.contains(&icon(order_look(state), None)),
            "Overview, {state}: {overview}"
        );
    }
    let order = scene.seeded.done;
    let item = format!("/awos/{order}/1");
    // The store keeps no machine order as Requested (its schema refuses it),
    // so no page shows that state; the table above holds its look.
    let kept = || {
        ORDERS
            .into_iter()
            .filter(|(state, _)| *state != OrderState::Requested)
    };
    for (state, stored) in kept() {
        let db = rusqlite::Connection::open(scene.database())?;
        db.execute(
            "UPDATE machine_orders SET state=?1 WHERE id=?2",
            rusqlite::params![stored, order],
        )?;
        // A new snapshot in this state, as the store keeps one per change.
        db.execute(
            "INSERT INTO machine_order_history(machine_order,evidence) SELECT machine_order,json_set(evidence,'$.state',?1) FROM machine_order_history WHERE machine_order=?2 ORDER BY sequence DESC LIMIT 1",
            rusqlite::params![stored, order],
        )?;
        drop(db);
        let look = order_look(state);
        let orders = scene.get("/orders").await?;
        assert!(shows(&orders, look), "Machine orders, {state}: {orders}");
        let page = scene.get(&item).await?;
        assert!(
            page.contains("<dt>Order status</dt><dd>") && shows(&page, look),
            "order item, {state}: {page}"
        );
    }
    let activity = scene.get("/activity").await?;
    for (state, _) in kept() {
        let look = order_look(state);
        assert!(
            activity.contains(&icon(look, Some("Order"))),
            "Activity, {state}: {activity}"
        );
        assert!(
            activity.contains(&format!(
                "– order ORD-1040 – sample 261119-040 – 1 test – {}</span>",
                lower_first(look.word)
            )),
            "Activity, {state}: {activity}"
        );
    }
    scene.stop().await
}

// Owner decision 2026-11-20 (review item 8, B4): the sidebar's Needs review
// count is read once for every signed-in page, so it never disappears; the
// pages shown before sign-in name no count, and the live script reloads a
// page for the count only when the count changed.
#[tokio::test(flavor = "multi_thread")]
async fn every_signed_in_page_shows_the_needs_review_count() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    scene
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| {
            for reason in ["synthetic one", "synthetic two"] {
                s.park(&Parked {
                    kind: Identifier::new("mapping_finding").map_err(FixtureError::Text)?,
                    reason: Reason::new(reason).map_err(FixtureError::Text)?,
                    targets: Vec::new(),
                })?;
            }
            Ok(())
        })
        .await?;
    let badge = "<span class=\"nav__count\"><span class=\"sr-only\">, </span>2</span>";
    let held = uuid::Uuid::from(scene.seeded.held.ok_or("held")?);
    let delivered = scene.seeded.delivered.ok_or("delivered")?;
    let pages = [
        "/".to_owned(),
        "/activity".to_owned(),
        "/parked".to_owned(),
        "/parked/1".to_owned(),
        "/machines".to_owned(),
        "/machines/chemistry-analyzer".to_owned(),
        format!("/receipts/{held}/departures?version=1"),
        "/setup".to_owned(),
        "/setup/add".to_owned(),
        "/setup/lab-apps".to_owned(),
        "/orders".to_owned(),
        "/analyzer-cancelled".to_owned(),
        format!("/awos/{}/1", scene.seeded.cancelled),
        "/deliveries".to_owned(),
        format!("/deliveries/{delivered}/resend"),
        "/retention".to_owned(),
        "/settings".to_owned(),
        // A page that only says what went wrong keeps the sidebar too.
        "/machines/no-such-machine".to_owned(),
    ];
    for path in &pages {
        let page = scene.get(path).await?;
        assert!(page.contains("<nav class=\"sidebar\""), "{path}: {page}");
        assert_eq!(page.matches(badge).count(), 1, "{path}: {page}");
        // The count sits beside Needs review and nowhere else.
        assert!(
            page.contains(&format!(
                "<span class=\"nav__label\">Needs review</span>{badge}"
            )),
            "{path}"
        );
    }
    // The admin key alone gets the same sidebar.
    let with_key = scene.get_with_key("/settings").await?;
    assert!(with_key.contains(badge), "{with_key}");
    // Sign-in is shown before any session and names no count.
    let login = request(&scene.started.3, "/login", "", None).await?;
    assert!(!login.contains("nav__count"), "{login}");
    let script = request(
        &scene.started.3,
        &format!(
            "{}live.js",
            crate::admin::view::assets::Root::of_this_build().as_str()
        ),
        "",
        None,
    )
    .await?;
    assert!(script.contains("return area === 'count' || shows.indexOf(area) >= 0;"));
    assert!(!script.contains("area === 'review'"));
    scene.stop().await
}

/// How many times the audit recorded a view of a machine's free text.
fn note_views(database: &std::path::Path) -> Result<i64, Error> {
    Ok(rusqlite::Connection::open(database)?.query_row(
        "SELECT count(*) FROM audit WHERE json_extract(evidence,'$.action')='admin-raw-view' AND json_extract(evidence,'$.targets[1]')='notes'",
        [],
        |row| row.get(0),
    )?)
}

// Owner decision 2026-11-20 (review B6): a machine's notes are free text that
// may hold patient data, so an order item's page hides them until someone
// asks through the audited "Show the original message", as Needs review does
// for original bytes; the view is recorded before anything shows.
#[tokio::test(flavor = "multi_thread")]
async fn order_notes_show_only_after_the_audited_view() -> Result<(), Error> {
    let scene = Scene::start(true).await?;
    let order = scene.seeded.cancelled;
    let path = format!("/awos/{order}/1");
    let note = "NTE|||Sample &#60;not&#62; found on the rack";
    let hidden = scene.get(&path).await?;
    assert!(hidden.starts_with("HTTP/1.1 200"), "{hidden}");
    assert!(!hidden.contains(note), "{hidden}");
    assert!(!hidden.contains("May contain patient data"));
    for part in [
        "<p class=\"muted more-row__text\">1 note from the machine, hidden</p>",
        "Show the original message</summary>",
        "The machine&#39;s notes are hidden because free text may contain patient data.",
        &format!("<form class=\"stack\" method=\"post\" action=\"/awos/{order}/1/raw\">"),
        "<input type=\"hidden\" name=\"target\" value=\"notes\"><input type=\"hidden\" name=\"after\" value=\"0\"><input type=\"hidden\" name=\"offset\" value=\"0\">",
        "<button type=\"submit\" class=\"btn btn--outline btn--sm\" name=\"source\" value=\"notes\">Show the notes</button>",
    ] {
        assert!(hidden.contains(part), "missing {part}: {hidden}");
    }
    assert_eq!(note_views(&scene.database())?, 0);
    // The admin key alone has no session to record the view with.
    let with_key = scene.get_with_key(&path).await?;
    assert!(
        with_key.contains(
            "<a class=\"link\" href=\"/login\">Sign in</a> to show the original message."
        )
    );
    assert!(!with_key.contains(note));
    let csrf = scene.token().await?;
    let raw = format!("{path}/raw");
    // A form this page did not send shows nothing and records nothing.
    let forged = scene
        .post(
            &raw,
            &format!("csrf={csrf}&target=receipt&source=notes&after=0&offset=0&reason="),
        )
        .await?;
    assert!(forged.starts_with("HTTP/1.1 409"), "{forged}");
    assert!(!forged.contains(note));
    assert_eq!(note_views(&scene.database())?, 0);
    let shown = scene
        .post(
            &raw,
            &format!(
                "csrf={csrf}&target=notes&source=notes&after=0&offset=0&reason=checking+the+rack"
            ),
        )
        .await?;
    assert!(shown.starts_with("HTTP/1.1 200"), "{shown}");
    for part in [
        note,
        "<p class=\"alert__title\">May contain patient data</p>",
        "Notes the machine sent, as sent</summary>",
    ] {
        assert!(shown.contains(part), "missing {part}: {shown}");
    }
    assert!(!shown.contains("Show the original message</summary>"));
    assert_eq!(note_views(&scene.database())?, 1);
    let recorded: String = rusqlite::Connection::open(scene.database())?.query_row(
        "SELECT evidence FROM audit WHERE json_extract(evidence,'$.action')='admin-raw-view'",
        [],
        |row| row.get(0),
    )?;
    let recorded: serde_json::Value = serde_json::from_str(&recorded)?;
    assert_eq!(
        recorded["targets"],
        serde_json::json!([format!("awos:{order}:1"), "notes", "notes-after:0"])
    );
    assert_eq!(recorded["reason"], "checking the rack");
    assert!(
        recorded["actor"]
            .as_str()
            .is_some_and(|actor| actor.starts_with("admin-session-")),
        "{recorded}"
    );
    // A view is not a change, so Activity does not list it.
    let activity = scene.get("/activity").await?;
    assert!(!activity.contains("notes"), "{activity}");
    // An item whose messages carry no notes records nothing.
    let quiet = scene
        .post(
            &format!("/awos/{}/1/raw", scene.seeded.done),
            &format!("csrf={csrf}&target=notes&source=notes&after=0&offset=0&reason="),
        )
        .await?;
    assert!(quiet.starts_with("HTTP/1.1 200"), "{quiet}");
    assert_eq!(note_views(&scene.database())?, 1);
    scene.stop().await
}
