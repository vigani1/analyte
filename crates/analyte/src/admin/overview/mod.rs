//! Serves the Overview (GET /): whether everything is OK first, then the signal path from machines to lab apps, orders, disk and alerts.
//!
//! The page shows only what Analyte observed or stored: no patient names,
//! result values, sample IDs or raw identifiers. A first run (no machine set
//! up, nothing to do) shows only the welcome. When a read fails or the reads
//! take too long, the error page says which read and keeps the reason for
//! support; it never shows a partial page that could read as "All good".
pub(crate) mod answer;
mod attention;
mod page;
mod signal;

use answer::Answer;
pub(in crate::admin) use attention::{Reason, Summary};
pub(in crate::admin) use signal::{Delivery, Keys, Standing, kept_health, order_list, standing};
#[cfg(test)]
mod tests;

use super::server::Context;
use super::server::sessions::Session;
use super::view::parts::Notice;
use super::view::report::{Problem, Section, read_within};
use super::view::{self, Tab, Tone, format, frame::Frame, icon::Icon};
use crate::admin::{Error, error::CommandError};
use crate::deadline;
use analyte_model::OrderState;
use analyte_store::{AlertKey, AlertState, FailingDestination, LastDelivered};
use askama::Template as _;
use axum::{
    Extension,
    extract::State,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;
use page::{AlertRow, Disk, OrderRow, Page, Verdict, WIZARD_STEPS, Welcome};
use std::{collections::BTreeMap, sync::Arc};

pub(super) async fn show(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let now = Timestamp::now();
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    match read_within(render(&state, session, now)).await {
        Ok(html) => Html(html).into_response(),
        // Evidence and errors stay on the authenticated page, never in logs.
        Err(problem) => problem.page(&state, session, Section::Overview),
    }
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

    /// Ties an error to this reading; a read stopped at its deadline is the
    /// overview running out of time.
    fn failed<E: Into<Error>>(self) -> impl FnOnce(E) -> Failure {
        move |error| {
            let error = error.into();
            if error.read_stopped() {
                return Failure {
                    reading: Self::Time,
                    error: None,
                };
            }
            Failure {
                reading: self,
                error: Some(Box::new(error)),
            }
        }
    }
}

/// Why the overview could not be shown: the reading and, unless it ran out of
/// time, the error.
struct Failure {
    reading: Reading,
    error: Option<Box<Error>>,
}

impl From<Failure> for Problem {
    /// The error page's problem: the reads ran out of time, or this reading
    /// failed and why.
    fn from(failure: Failure) -> Self {
        match failure.error {
            Some(error) => Self::Unread(failure.reading.sentence(), error.to_string()),
            None => Self::Time,
        }
    }
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
        .read::<_, CommandError>(until, move |store| {
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

/// What the overview read: everything its verdict depends on, and the signal
/// path's nodes and figures built from the same reads.
struct Observed {
    restore_paused: bool,
    free: u64,
    review: u64,
    /// Every measured fact, the active alerts included.
    facts: attention::Facts,
    machines: Vec<page::MachineNode>,
    labs: Vec<page::LabNode>,
    figures: Vec<page::Figure>,
    kept: Vec<super::removed::Line>,
}

/// What the overview reads besides the verdict's facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nodes {
    /// When each machine last sent data, for the machine nodes: the page.
    WithActivity,
    /// The facts only: `GET /status`. Receipts never change a fact.
    FactsOnly,
}

/// Reads what decides the verdict, once, for the page and for `GET /status`,
/// in the page's order, so the first read that fails is the one it names.
async fn observe(
    state: &Context,
    until: std::time::Instant,
    nodes: Nodes,
    now: Timestamp,
) -> Result<Observed, Failure> {
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
            .read(until, analyte_store::Store::analyzer_cancelled_count)
            .await
            .map_err(Reading::Orders.failed())?,
        uncertain: uncertain_orders(state, until, now)
            .await
            .map_err(Reading::Orders.failed())?,
        ..attention::Facts::default()
    };
    let checks: Vec<_> = state
        .machines
        .iter()
        .filter_map(|machine| Some((machine.id.clone(), machine.software.clone()?)))
        .collect();
    facts.software = super::machines::software_version::warnings(&state.store, until, checks)
        .await
        .map_err(Reading::Alerts.failed())?;
    let activity = match nodes {
        Nodes::WithActivity => activity(state, until)
            .await
            .map_err(Reading::Receipts.failed())?,
        Nodes::FactsOnly => BTreeMap::new(),
    };
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
    facts.alerts = alerts;
    Ok(Observed {
        restore_paused,
        free,
        review,
        facts,
        machines,
        labs,
        figures,
        kept,
    })
}

/// The verdict's lines, from the same reads and rules as the page, within
/// [`view::READ_BUDGET`]; otherwise the reading that failed or ran out of
/// time. It reads no receipts, which only draw the machine nodes.
async fn things(state: &Context, now: Timestamp) -> Result<Vec<attention::Thing>, Reading> {
    let read = async {
        let until = deadline().map_err(Reading::Store.failed())?;
        observe(state, until, Nodes::FactsOnly, now).await
    };
    match tokio::time::timeout(view::READ_BUDGET, read).await {
        Ok(Ok(observed)) => Ok(attention::things(&observed.facts)),
        Ok(Err(failure)) => Err(failure.reading),
        Err(_) => Err(Reading::Time),
    }
}

/// The overall state the verdict shows and the kinds of thing behind it, from
/// the same reads and rules; none when it could not be read in time. Why it
/// failed goes to the log as the reading's name only, never its values.
pub(in crate::admin) async fn summary(state: &Context, now: Timestamp) -> Option<Summary> {
    match things(state, now).await {
        Ok(things) => Some(Summary::of(&things)),
        Err(reading) => {
            tracing::warn!(?reading, "the overall state could not be read");
            None
        }
    }
}

/// Serves `GET /overview/verdict`, signed in or with the admin key: the
/// verdict with every line, its page and its command, for `analyte overview`
/// and `analyte doctor`. An unread state answers 503 with what could not be
/// read, never an empty list that could read as "All good".
pub(in crate::admin) async fn verdict(State(state): State<Arc<Context>>) -> Response {
    match things(&state, Timestamp::now()).await {
        Ok(things) => axum::Json(Answer::of(&things)).into_response(),
        Err(reading) => {
            tracing::warn!(?reading, "the overall state could not be read");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(Answer::unknown(reading.sentence())),
            )
                .into_response()
        }
    }
}

async fn render(
    state: &Context,
    session: Option<&Session>,
    now: Timestamp,
) -> Result<String, Failure> {
    let until = deadline().map_err(Reading::Store.failed())?;
    let Observed {
        restore_paused,
        free,
        review,
        facts,
        machines,
        labs,
        figures,
        kept,
    } = observe(state, until, Nodes::WithActivity, now).await?;
    let relays = crate::operator::notification_status::snapshot(
        &state.store,
        &state.alert_endpoints,
        now.as_second(),
    )
    .await
    .map_err(Reading::Relays.failed())?;
    let alert_rows = facts
        .alerts
        .iter()
        .map(|alert| alert_row(alert, now))
        .collect();
    let alert_messages = facts
        .alerts
        .iter()
        .filter_map(crate::operator::alerts::message)
        .collect();
    let orders = orders(&facts);
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
                title: format::RESTORE_PAUSED.into(),
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
            format::order_look(OrderState::Uncertain),
            "No orders with an unknown outcome",
            &format!(
                "{} with an unknown outcome (check the machine)",
                format::amount(facts.uncertain, "order", "orders")
            ),
            (Tab::MachineOrders.href(), "See machine orders"),
        ),
        order_row(
            facts.cancelled,
            format::order_look(OrderState::CancelConfirmed),
            "No orders cancelled by a machine",
            &format!(
                "{} cancelled by a machine",
                format::amount(facts.cancelled, "order", "orders")
            ),
            (Tab::CancelledByMachine.href(), "Review cancelled orders"),
        ),
    ]
}

/// One Orders line: green with `none` at zero, otherwise `some` in the
/// tone and icon its order state has on every page.
fn order_row(
    count: u64,
    look: format::StateLook,
    none: &str,
    some: &str,
    (href, link): (&'static str, &'static str),
) -> OrderRow {
    let zero = count == 0;
    OrderRow {
        tone: if zero { Tone::Good } else { look.tone },
        icon: if zero { Icon::Ok } else { look.icon },
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
            .map_or_else(|| format::NOT_RECORDED.into(), |value| value.to_string()),
    }
}

/// One active alert as `analyte alerts list` prints it: the Overview's
/// words for it, the line it adds to the verdict, and where it is fixed.
#[derive(serde::Serialize)]
pub(crate) struct PrintedAlert {
    /// What the alert means, as the Disk and alerts card says it.
    pub(crate) alert: &'static str,
    /// Its signal name, such as `parked_items`.
    pub(crate) signal: &'static str,
    /// The machine it is about, when it is about one.
    pub(crate) machine: Option<String>,
    /// Who it goes to, as the alert rule names them.
    pub(crate) responder: String,
    /// When it was raised, as the page shows it; none when not recorded.
    pub(crate) raised: Option<String>,
    /// The reason in the verdict's words, with the latest value.
    pub(crate) reason: String,
    /// What to do, for a kind whose fix varies.
    pub(crate) fix: Option<String>,
    /// The page that fixes it.
    pub(crate) page: &'static str,
    /// The command that shows or fixes it, when one does.
    pub(crate) command: Option<&'static str>,
}

impl PrintedAlert {
    /// `alert` as of `now`.
    pub(crate) fn of(alert: &AlertState, now: Timestamp) -> Self {
        let (signal, machine) = crate::operator::alerts::signal(&alert.key);
        let line = attention::from_alert(alert);
        Self {
            alert: alert_words(&alert.key),
            signal,
            machine: machine.map(str::to_owned),
            responder: alert.responder.clone(),
            raised: alert
                .raised_at
                .map(|raised| format::moment_unix(raised, now).shown),
            reason: line.text,
            fix: crate::operator::alerts::message(alert),
            page: line.fix.label(),
            command: line.fix.command(),
        }
    }
}

/// What an alert means, in plain words; its signal name shows on hover.
const fn alert_words(key: &AlertKey) -> &'static str {
    match key {
        AlertKey::OldestUndelivered => "A result has waited too long to be sent",
        AlertKey::MachineSilent(_) => "Machine has gone quiet",
        AlertKey::MachineSoftwareVersionChanged(_) => "Machine software version changed",
        AlertKey::DiskReserve => "Disk space is low",
        AlertKey::OrderCacheStale => "Order list is out of date",
        AlertKey::UncertainOrders => "Orders with unknown outcome",
        AlertKey::ParkedItems => "Items need review",
        AlertKey::RestorePaused => format::RESTORE_PAUSED,
        AlertKey::UndeliverableDestination(..) => "Results cannot be delivered",
    }
}

/// How many machine orders have an unknown outcome: the count the store keeps
/// with every order state change, which the uncertain-orders alert reads too.
async fn uncertain_orders(
    state: &Context,
    until: std::time::Instant,
    now: Timestamp,
) -> Result<u64, Error> {
    Ok(state
        .store
        .read::<_, CommandError>(until, move |store| {
            Ok(store
                .alert_value(&AlertKey::UncertainOrders, now.as_second(), &[])?
                .unwrap_or_default())
        })
        .await?)
}

/// Gives, by machine name, when the latest stored message from each set-up
/// machine was received, in one store request.
pub(super) async fn activity(
    state: &Context,
    until: std::time::Instant,
) -> Result<BTreeMap<String, Timestamp>, Error> {
    let machines: Vec<_> = state.machines.iter().map(|m| m.id.clone()).collect();
    let newest = state
        .store
        .read(until, move |store| store.newest_receipts(&machines))
        .await?;
    Ok(newest
        .into_iter()
        .map(|(machine, at)| (machine.as_str().to_owned(), at))
        .collect())
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
            // Seeding is fixture work, not a claim about production latency: a cold
            // Windows runner took over the 5 s store budget here.
            .request::<_, analyte_store::OperatorError>(crate::test_startup_deadline()?, |s| {
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
        // The overview reads the relay lines this way and shows them under "Details for support".
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
        store.shutdown(crate::test_startup_deadline()?).await?;
        Ok(())
    }
}
