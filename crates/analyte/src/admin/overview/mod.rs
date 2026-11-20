//! Serves the Overview (GET /): whether everything is OK first, then the signal path from machines to lab apps, orders, disk and alerts.
//!
//! The page shows only what Analyte observed or stored: no patient names,
//! result values, sample IDs or raw identifiers. A first run (no machine set
//! up, nothing to do) shows only the welcome. When a read fails or the reads
//! take too long, the error page says which read and keeps the reason for
//! support; it never shows a partial page that could read as "All good".
mod attention;
mod page;
mod signal;

pub(in crate::admin) use signal::{Delivery, Keys, Standing, kept_health, order_list, standing};
#[cfg(test)]
mod tests;

use super::server::Context;
use super::sessions::Session;
use super::view::{self, Tab, Tone, format, frame::Frame, icon::Icon};
use crate::admin::{Error, error::CommandError};
use crate::deadline;
use analyte_model::OrderState;
use analyte_store::{AlertKey, AlertState, FailingDestination, LastDelivered, MachineOrderFilter};
use askama::Template as _;
use axum::{
    Extension,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;
use page::{AlertRow, Disk, ErrorPage, Notice, OrderRow, Page, Verdict, WIZARD_STEPS, Welcome};
use std::{collections::BTreeMap, sync::Arc};

pub(super) async fn show(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let now = Timestamp::now();
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    let failed = match tokio::time::timeout(view::READ_BUDGET, render(&state, session, now)).await {
        Ok(Ok(html)) => return Html(html).into_response(),
        Ok(Err(failure)) => failure,
        Err(_) => Failure {
            reading: Reading::Time,
            error: None,
        },
    };
    // Evidence and errors stay on the authenticated page, never in logs.
    failure(&state, session, now, &failed)
}

/// What the overview was reading when it stopped; the error page names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    /// The store refused or dropped the request itself.
    Store,
    Restore,
    Disk,
    Review,
    Delivered,
    Failing,
    Alerts,
    Orders,
    Receipts,
    LabApps,
    Kept,
    Relays,
    /// Everything was read, but the page could not be written.
    Page,
    /// The reads took longer than [`view::READ_BUDGET`].
    Time,
}

impl Reading {
    /// What Analyte could not do, in plain words.
    const fn sentence(self) -> &'static str {
        match self {
            Self::Store => "Analyte could not read its store.",
            Self::Restore => {
                "Analyte could not read whether order sending is paused after a restore."
            }
            Self::Disk => "Analyte could not read the free disk space.",
            Self::Review => "Analyte could not read how many items need review.",
            Self::Delivered => "Analyte could not read when each lab app last accepted a delivery.",
            Self::Failing => {
                "Analyte could not read which lab apps keep failing to accept deliveries."
            }
            Self::Alerts => "Analyte could not read the active alerts.",
            Self::Orders => "Analyte could not read the machine orders.",
            Self::Receipts => "Analyte could not read when each machine last sent data.",
            Self::LabApps => "Analyte could not read what waits for each lab app.",
            Self::Kept => "Analyte could not read what waits for lab apps no set-up machine uses.",
            Self::Relays => "Analyte could not read the alert relays.",
            Self::Page => "Analyte read everything but could not write the page.",
            Self::Time => format::SLOW_READ,
        }
    }

    /// Ties an error to this reading.
    fn failed<E: Into<Error>>(self) -> impl FnOnce(E) -> Failure {
        move |error| Failure {
            reading: self,
            error: Some(Box::new(error.into())),
        }
    }
}

/// Why the overview could not be shown: the reading and, unless it ran out of
/// time, the error.
struct Failure {
    reading: Reading,
    error: Option<Box<Error>>,
}

impl Failure {
    /// A timeout is a 504 like every slow admin request; any other failure is a 503.
    fn status(&self) -> StatusCode {
        if self.reading == Reading::Time {
            StatusCode::GATEWAY_TIMEOUT
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        }
    }

    /// The exact reason, for support.
    fn detail(&self) -> String {
        self.error.as_ref().map_or_else(
            || {
                format!(
                    "admin overview deadline of {} ms exceeded",
                    view::READ_BUDGET.as_millis()
                )
            },
            ToString::to_string,
        )
    }
}

/// The overview's own error page; should even that fail to render, the plain
/// refusal page carries both reasons.
fn failure(
    state: &Context,
    session: Option<&Session>,
    now: Timestamp,
    failed: &Failure,
) -> Response {
    let page = ErrorPage {
        frame: Frame::new(state, Tab::Overview, "Overview", None, session, now),
        text: error_text(failed.reading),
        detail: vec![failed.detail()],
    };
    match page.render() {
        Ok(html) => (failed.status(), Html(html)).into_response(),
        Err(render) => fallback(failed, &render),
    }
}

/// What the error page tells people: what could not be read and what to do.
fn error_text(reading: Reading) -> String {
    format!(
        "{} Reload the page; if this keeps happening, give the details below to support.",
        reading.sentence()
    )
}

/// The plain refusal page for when the error page itself could not be written.
fn fallback(failed: &Failure, render: &askama::Error) -> Response {
    view::refusal(
        failed.status(),
        "Overview",
        concat!(
            "The overview could not be shown just now. Reload the page; if this ",
            "keeps happening, give the details below to support.",
        ),
        format!("{}; the error page failed too: {render}", failed.detail()),
    )
}

/// What one store request reads for the overview; each part keeps its own
/// error so the error page can say which read failed.
struct Stored {
    restore_paused: Result<bool, CommandError>,
    free: Result<u64, std::io::Error>,
    review: Result<u64, CommandError>,
    delivered: Result<LastDelivered, CommandError>,
    failing: Result<Vec<FailingDestination>, CommandError>,
}

/// [`Stored`] once every part was read.
struct Read {
    restore_paused: bool,
    free: u64,
    review: u64,
    delivered: LastDelivered,
    failing: Vec<FailingDestination>,
}

/// The first store request: the restore flag, free disk space, the Needs
/// review count, when each lab app last accepted a delivery and which keep
/// failing.
async fn stored(
    state: &Context,
    until: std::time::Instant,
    now: Timestamp,
) -> Result<Read, Failure> {
    let directory = state.directory.clone();
    let stored = state
        .store
        .request::<_, CommandError>(until, move |store| {
            Ok::<_, CommandError>(Stored {
                restore_paused: store.restore_unreconciled().map_err(CommandError::from),
                free: fs4::available_space(&directory),
                // The unresolved count the parked-items alert rule reads.
                review: store
                    .alert_value(&AlertKey::ParkedItems, now.as_second(), &[])
                    .map(Option::unwrap_or_default)
                    .map_err(CommandError::from),
                delivered: store.last_delivered().map_err(CommandError::from),
                failing: store.failing_destinations().map_err(CommandError::from),
            })
        })
        .await
        .map_err(Reading::Store.failed())?;
    Ok(Read {
        restore_paused: stored.restore_paused.map_err(Reading::Restore.failed())?,
        free: stored.free.map_err(Reading::Disk.failed())?,
        review: stored.review.map_err(Reading::Review.failed())?,
        delivered: stored.delivered.map_err(Reading::Delivered.failed())?,
        failing: stored.failing.map_err(Reading::Failing.failed())?,
    })
}

async fn render(
    state: &Context,
    session: Option<&Session>,
    now: Timestamp,
) -> Result<String, Failure> {
    let until = deadline().map_err(Reading::Store.failed())?;
    let Read {
        restore_paused,
        free,
        review,
        delivered,
        failing,
    } = stored(state, until, now).await?;
    let alerts = crate::operator::alerts::active(&state.store)
        .await
        .map_err(Reading::Alerts.failed())?;
    let mut facts = attention::Facts {
        review,
        disk_full: free <= state.reserve,
        restore_paused,
        cancelled: state
            .store
            .request(until, |store| store.analyzer_cancelled_count())
            .await
            .map_err(Reading::Orders.failed())?,
        uncertain: uncertain_orders(state, until)
            .await
            .map_err(Reading::Orders.failed())?,
        ..attention::Facts::default()
    };
    let activity = activity(state, until)
        .await
        .map_err(Reading::Receipts.failed())?;
    let machines = signal::machines(state, &activity, &mut facts, now);
    let (labs, figures) = signal::lab_apps(state, &delivered, &failing, &mut facts, now)
        .await
        .map_err(Reading::LabApps.failed())?;
    let kept = super::removed::lines(state)
        .await
        .map_err(Reading::Kept.failed())?;
    facts
        .blocked
        .extend(kept.iter().filter_map(super::removed::Line::blocked));
    let relays = crate::operator::notification_status::snapshot(
        &state.store,
        &state.alert_endpoints,
        now.as_second(),
    )
    .await
    .map_err(Reading::Relays.failed())?;
    let alert_rows = alerts.iter().map(|alert| alert_row(alert, now)).collect();
    let alert_messages = alerts
        .iter()
        .filter_map(crate::operator::alerts::message)
        .collect();
    let orders = orders(&facts);
    facts.alerts = alerts;
    let things = attention::things(&facts);
    let frame = Frame::new(state, Tab::Overview, "Overview", Some(review), session, now);
    // First run: no machine is set up, nothing needs anyone and nothing waits for a
    // lab app (a removed machine's results still being sent keep the full page).
    let html = if state.machines.is_empty() && things.is_empty() && kept.is_empty() {
        Welcome {
            frame,
            steps: WIZARD_STEPS,
        }
        .render()
    } else {
        Page {
            frame,
            today: format::today(now),
            counts: format::machines_and_lab_apps(machines.len(), labs.len()),
            verdict: Verdict::of(&things),
            machines,
            labs,
            figures,
            orders,
            restore: restore_paused.then(|| Notice {
                tone: Tone::Attention,
                title: "Order sending is paused after a restore".into(),
                text: format::RESTORE_ADVICE.into(),
            }),
            kept: kept
                .iter()
                .filter(|line| !line.on_path())
                .map(|line| Notice {
                    tone: line.tone(),
                    title: line.text().to_owned(),
                    text: String::new(),
                })
                .collect(),
            disk: Disk::of(free, state.reserve),
            alerts: alert_rows,
            alert_messages,
            relays,
        }
        .render()
    };
    html.map_err(Reading::Page.failed())
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
            .request::<_, CommandError>(until, move |store| {
                Ok::<_, CommandError>(store.machine_orders(&MachineOrderFilter {
                    after,
                    machine: None,
                    state: Some(OrderState::Uncertain),
                })?)
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

/// Reads every stored receipt once and gives, by machine name, when the latest
/// stored message from each set-up machine was received.
pub(super) async fn activity(
    state: &Context,
    until: std::time::Instant,
) -> Result<BTreeMap<String, Timestamp>, Error> {
    let mut seen = BTreeMap::<String, Timestamp>::new();
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
                let last = seen
                    .entry(receipt.machine.as_str().to_owned())
                    .or_insert(receipt.received_at);
                *last = (*last).max(receipt.received_at);
            }
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod webhook_tests {
    use super::*;
    use crate::Error;
    use std::time::Duration;

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
                limits.busy_timeout = Duration::from_millis(100);
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
