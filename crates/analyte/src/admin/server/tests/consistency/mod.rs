//! Tests that the admin pages agree with each other: each delivery and order state reads the same on every page that shows it; the sidebar's count and an order item's audited notes are tested below.
use super::machines::AT;
use super::more::Scene;
use super::*;
use crate::admin::view::format::{StateLook, attempt_look, delivery_look, order_look};
use analyte_store::{AttemptOutcome, DeliveryState};

mod notes;
mod sidebar;

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
