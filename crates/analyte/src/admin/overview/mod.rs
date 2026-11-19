//! Serves the Overview (GET /): whether everything is OK first, then the signal path from machines to lab apps, orders, disk and alerts.
//!
//! The page shows only what Analyte observed or stored: no patient names,
//! result values, sample IDs or raw identifiers. A first run with nothing set
//! up keeps the old welcome card until its own restyle.
mod attention;
mod page;
mod signal;

use super::server::Context;
use super::sessions::Session;
use super::view::{self, Tab, Tone, format, frame::Frame, icon::Icon};
use crate::admin::Error;
use crate::deadline;
use analyte_model::{OrderState, ReceiptId};
use analyte_store::{AlertKey, AlertState, LastDelivered, MachineOrderFilter};
use askama::Template as _;
use axum::{
    Extension,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;
use page::{AlertRow, Disk, ErrorPage, Notice, OrderRow, Page, Verdict};
use std::{collections::BTreeMap, sync::Arc};

pub(super) async fn show(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let now = Timestamp::now();
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    match render(&state, session, now).await {
        Ok(html) => Html(html).into_response(),
        // Evidence and errors stay on the authenticated page, never in logs.
        Err(error) => failure(&state, session, now, &error),
    }
}

/// The overview's own error page; should even that fail to render, the plain
/// refusal page carries both reasons.
fn failure(state: &Context, session: Option<&Session>, now: Timestamp, error: &Error) -> Response {
    let page = ErrorPage {
        frame: Frame::new(state, Tab::Overview, "Overview", None, session, now),
        detail: vec![error.to_string()],
    };
    match page.render() {
        Ok(html) => (StatusCode::SERVICE_UNAVAILABLE, Html(html)).into_response(),
        Err(render) => view::refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "Overview",
            concat!(
                "The overview could not be shown just now. Reload the page; if this ",
                "keeps happening, give the details below to support.",
            ),
            format!("{error}; the error page failed too: {render}"),
        ),
    }
}

/// What one store request reads for the overview.
struct Stored {
    restore_paused: bool,
    free: u64,
    review: u64,
    delivered: LastDelivered,
}

async fn render(
    state: &Context,
    session: Option<&Session>,
    now: Timestamp,
) -> Result<String, Error> {
    let until = deadline()?;
    let directory = state.directory.clone();
    let stored = state
        .store
        .request::<_, crate::admin::error::CommandError>(until, move |store| {
            Ok::<_, crate::admin::error::CommandError>(Stored {
                restore_paused: store.restore_unreconciled()?,
                free: fs4::available_space(&directory)?,
                // The unresolved count the parked-items alert rule reads.
                review: store
                    .alert_value(&AlertKey::ParkedItems, now.as_second(), &[])?
                    .unwrap_or_default(),
                delivered: store.last_delivered()?,
            })
        })
        .await?;
    let alerts = crate::operator::alerts::active(&state.store).await?;
    let mut facts = attention::Facts {
        review: stored.review,
        disk_full: stored.free <= state.reserve,
        restore_paused: stored.restore_paused,
        cancelled: state
            .store
            .request(until, |store| store.analyzer_cancelled_count())
            .await?,
        uncertain: uncertain_orders(state, until).await?,
        ..attention::Facts::default()
    };
    let activity = activity(state, until, 0).await?;
    let machines = signal::machines(state, &activity, &mut facts, now);
    let (labs, figures) = signal::lab_apps(state, &stored.delivered, &mut facts, now).await?;
    let kept = super::removed::lines(state).await?;
    facts
        .blocked
        .extend(kept.iter().filter_map(super::removed::Line::blocked));
    let relays = crate::operator::notification_status::snapshot(
        &state.store,
        &state.alert_endpoints,
        now.as_second(),
    )
    .await?;
    let alert_rows = alerts.iter().map(|alert| alert_row(alert, now)).collect();
    let alert_messages = alerts
        .iter()
        .filter_map(crate::operator::alerts::message)
        .collect();
    let orders = orders(&facts);
    facts.alerts = alerts;
    let things = attention::things(&facts);
    // First run: no machine is set up, nothing needs anyone and nothing waits for a
    // lab app (a removed machine's results still being sent keep the full page).
    if state.machines.is_empty() && things.is_empty() && kept.is_empty() {
        return Ok(view::page(Tab::Overview, "Welcome to Analyte")
            .fragment(&attention::welcome())
            .markup(view::END)
            .to_string());
    }
    Ok(Page {
        frame: Frame::new(
            state,
            Tab::Overview,
            "Overview",
            Some(stored.review),
            session,
            now,
        ),
        today: format::today(now),
        counts: format::machines_and_lab_apps(machines.len(), labs.len()),
        verdict: Verdict::of(&things),
        machines,
        labs,
        figures,
        orders,
        restore: stored.restore_paused.then(|| Notice {
            tone: Tone::Attention,
            title: "Order sending is paused after a restore".into(),
            text: format::RESTORE_ADVICE.into(),
        }),
        kept: kept
            .iter()
            .map(|line| Notice {
                tone: line.tone(),
                title: line.text().to_owned(),
                text: String::new(),
            })
            .collect(),
        disk: Disk {
            free: format!("{} free", format::bytes(stored.free)),
            free_exact: format!("{} bytes free", stored.free),
            reserve: format::bytes(state.reserve),
            reserve_exact: format!("{} bytes", state.reserve),
            enough: stored.free > state.reserve,
        },
        alerts: alert_rows,
        alert_messages,
        relays,
    }
    .render()?)
}

/// The Orders card: orders with an unknown outcome and orders a machine
/// cancelled, each with the page that lists them.
fn orders(facts: &attention::Facts) -> Vec<OrderRow> {
    vec![
        order_row(
            facts.uncertain,
            (Icon::Unknown, Tone::Attention),
            "No orders with an unknown outcome",
            &format!(
                "{} with an unknown outcome (check the machine)",
                format::amount(facts.uncertain, "order", "orders")
            ),
            (Tab::MachineOrders.href(), "See machine orders"),
        ),
        order_row(
            facts.cancelled,
            (Icon::Cancelled, Tone::Neutral),
            "No orders cancelled by a machine",
            &format!(
                "{} cancelled by a machine",
                format::amount(facts.cancelled, "order", "orders")
            ),
            (Tab::CancelledByMachine.href(), "Review cancelled orders"),
        ),
    ]
}

/// One Orders line: green with `none` at zero, otherwise `some` in its own icon and tone.
fn order_row(
    count: u64,
    (icon, tone): (Icon, Tone),
    none: &str,
    some: &str,
    (href, link): (&'static str, &'static str),
) -> OrderRow {
    let zero = count == 0;
    OrderRow {
        tone: if zero { Tone::Good } else { tone },
        icon: if zero { Icon::Ok } else { icon },
        text: if zero { none } else { some }.to_owned(),
        href,
        link,
    }
}

fn alert_row(alert: &AlertState, now: Timestamp) -> AlertRow {
    let (kind, machine) = crate::operator::alerts::signal(&alert.key);
    AlertRow {
        words: alert_words(&alert.key),
        kind,
        machine: machine.unwrap_or("—").to_owned(),
        responder: alert.responder.clone(),
        raised: alert
            .raised_at
            .map(|raised| format::moment_unix(raised, now)),
        value: alert
            .value
            .map_or_else(|| "Not known".into(), |value| value.to_string()),
    }
}

/// What an alert means, in plain words; its signal name shows on hover.
const fn alert_words(key: &AlertKey) -> &'static str {
    match key {
        AlertKey::OldestUndelivered => "A result has waited too long to be sent",
        AlertKey::MachineSilent(_) => "Machine has gone quiet",
        AlertKey::DiskReserve => "Disk space is low",
        AlertKey::OrderCacheStale => "Order list is out of date",
        AlertKey::UncertainOrders => "Orders with unknown outcome",
        AlertKey::ParkedItems => "Items need review",
        AlertKey::RestorePaused => "Order sending paused after restore",
        AlertKey::UndeliverableDestination(..) => "Results cannot be delivered",
    }
}

async fn uncertain_orders(state: &Context, until: std::time::Instant) -> Result<u64, Error> {
    let mut after = 0;
    let mut uncertain = 0u64;
    loop {
        let page = state
            .store
            .request::<_, crate::admin::error::CommandError>(until, move |store| {
                Ok::<_, crate::admin::error::CommandError>(store.machine_orders(
                    &MachineOrderFilter {
                        after,
                        machine: None,
                        state: Some(OrderState::Uncertain),
                    },
                )?)
            })
            .await?;
        let Some(last) = page.last() else {
            break;
        };
        after = last.id;
        uncertain = uncertain
            .checked_add(u64::try_from(page.len())?)
            .ok_or_else(|| Error::UncertainOrderCountOverflow)?;
    }
    Ok(uncertain)
}

/// What the stored receipts say about one set-up machine.
#[derive(Default)]
pub(super) struct Activity {
    /// When the latest stored message from it was received.
    pub(super) last: Option<Timestamp>,
    /// Its first stored messages in store order, at most the number asked for.
    pub(super) first: Vec<(ReceiptId, Timestamp)>,
}

/// Reads every stored receipt once and gathers each set-up machine's activity
/// by machine name, keeping up to `keep` of its first receipts.
pub(super) async fn activity(
    state: &Context,
    until: std::time::Instant,
    keep: usize,
) -> Result<BTreeMap<String, Activity>, Error> {
    let mut seen = BTreeMap::<String, Activity>::new();
    let mut cursor = String::new();
    loop {
        let page = state
            .store
            .request(until, move |store| store.receipts(&cursor))
            .await?;
        let Some(last) = page.last() else {
            break;
        };
        cursor = serde_json::to_string(&last.id)?;
        for receipt in page {
            if state.machines.iter().any(|m| m.id == receipt.machine) {
                let machine = seen.entry(receipt.machine.as_str().to_owned()).or_default();
                machine.last = machine.last.max(Some(receipt.received_at));
                if machine.first.len() < keep {
                    machine.first.push((receipt.id, receipt.received_at));
                }
            }
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod webhook_tests {
    use super::*;
    use crate::Error;

    /// The overview's relay disclosure on its own.
    #[derive(askama::Template)]
    #[template(
        source = "{% import \"components/feedback.html\" as feedback %}{% call feedback::support(\"Relay status for support\", relays) %}{% endcall %}",
        ext = "html"
    )]
    struct Relays {
        relays: Vec<String>,
    }

    // ADR 0014
    #[tokio::test]
    async fn overview_shows_failed_and_full_webhook_queue() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let mut store = analyte_store::Handle::start(
            temp.path().join("status.db"),
            {
                let mut limits = analyte_fixtures::store_limits();
                limits.reserve_bytes = 0;
                limits.record_bytes = 2_097_152;
                limits.busy_timeout = std::time::Duration::from_millis(100);
                limits
            },
            Box::new(crate::runtime::space::Volume),
            8,
            crate::test_startup_deadline()?,
        )
        .await?;
        store
            .request::<_, analyte_store::OperatorError>(deadline()?, |s| {
                let endpoint = analyte_model::Identifier::new("relay").map_err(|cause| {
                    analyte_store::OperatorError::Storage(analyte_store::StorageError::Text(cause))
                })?;
                s.register_alert_endpoint(&endpoint)?;
                s.evaluate_alert(&AlertKey::ParkedItems, "Duty officer", true, Some(1), 100)?;
                s.prepare_alert_event(&endpoint, 100, 60, 1)?;
                s.prepare_alert_event(&endpoint, 100, 60, 1)?;
                let event = s
                    .claim_alert_attempt(&endpoint, 100, 1, 5, 10)?
                    .ok_or(analyte_store::OperatorError::Corruption)?;
                s.fail_alert_attempt(event.id(), analyte_store::AlertFailure::Network)
            })
            .await?;
        // The overview reads the relay lines this way and shows them under "For support".
        let relays = crate::operator::notification_status::snapshot(
            &store,
            &[analyte_model::Identifier::new("missing")?],
            120,
        )
        .await?;
        let html = Relays { relays }.render()?;
        assert!(html.contains("<summary>"), "{html}");
        for field in [
            "pending_count=1",
            "oldest_age_seconds=20",
            "exhausted=1",
            "queue_full=true",
            "failed=1",
        ] {
            assert!(html.contains(field));
        }
        assert!(html.contains(concat!(
            "alert_endpoint=missing state=unavailable ",
            "pending_count=unknown",
        )));
        let program = crate::installation::config::Program::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/analyte.toml"),
        )?;
        let lines = crate::operator::status::snapshot(&program, &store).await?;
        assert!(
            lines
                .iter()
                .any(|line| line.contains("alert_endpoint=relay")
                    && line.contains("queue_full=true")
                    && line.contains("exhausted=1"))
        );
        store.shutdown(deadline()?).await?;
        Ok(())
    }
}
