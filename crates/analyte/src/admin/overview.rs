//! Renders a read-only HTML overview in sections: machines, sending to the lab app, orders, disk and alerts.
use super::server::{Context, Machine};
use super::view::{self, Tone, exact, pill, plain, row};
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::deadline;
use crate::machine::link::status::{Light, Link};
use analyte_model::OrderState;
use analyte_store::{AlertKey, MachineOrderFilter};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use jiff::Timestamp;
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
};

pub(super) async fn show(State(state): State<Arc<Context>>) -> Response {
    match render(&state).await {
        Ok(html) => Html(html).into_response(),
        // Evidence and errors stay on the authenticated page, never in logs.
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(view::message("Overview unavailable", error)),
        )
            .into_response(),
    }
}

async fn render(state: &Context) -> Result<Markup, Error> {
    let until = deadline()?;
    let directory = state.directory.clone();
    let (paused, free) = state
        .store
        .request::<_, crate::admin::error::CommandError>(until, move |store| {
            Ok::<_, crate::admin::error::CommandError>((
                store.restore_unreconciled()?,
                fs4::available_space(&directory)?,
            ))
        })
        .await?;
    let now = Timestamp::now();
    let alerts = crate::operator::alerts::active(&state.store).await?;
    let mut html = view::page("Overview").markup(
        "<p class=\"lead\">Operator actions require an authenticated session and a reason.</p>",
    );
    if !alerts.is_empty() {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\"><strong>")
                .text(plain::amount(
                    u64::try_from(alerts.len())?,
                    "active alert",
                    "active alerts",
                ))
                .markup("</strong>: see Disk and alerts below.</p>"),
        );
    }
    if free <= state.reserve {
        html.push_markup(concat!(
            "<p class=\"notice bad\"><strong>The disk reserve is reached:</strong> Analyte ",
            "refuses new data from machines until disk space is freed.</p>",
        ));
    }
    render_machines(state, &mut html, until, now).await?;
    render_destinations(state, &mut html, now).await?;
    render_orders(state, &mut html, until, paused).await?;
    html.push_markup("<section><h2>Disk and alerts</h2><dl>");
    render_disk(&mut html, free, state.reserve);
    render_alerts(&mut html, &alerts, now);
    render_alert_endpoints(
        &state.store,
        &state.alert_endpoints,
        &mut html,
        now.as_second(),
    )
    .await?;
    html.push_markup("</section>");
    Ok(html.markup(view::END))
}

fn render_disk(html: &mut Markup, free: u64, reserve: u64) {
    html.push_fragment(&row(
        "Disk space",
        Some("Free disk bytes"),
        &exact(
            &format!("{} free", plain::bytes(free)),
            &format!("{free} bytes free"),
        ),
    ));
    html.push_fragment(&row(
        "Reserve",
        Some("Disk reserve bytes"),
        &exact(&plain::bytes(reserve), &format!("{reserve} bytes"))
            .markup(" <span class=\"muted\">Analyte stops receiving below this</span>"),
    ));
    html.push_fragment(&row(
        "Disk status",
        Some("Disk capacity"),
        &if free > reserve {
            pill(Tone::Good, "Above reserve")
        } else {
            pill(Tone::Problem, "Reserve reached: receiving refused")
        },
    ));
}

fn render_alerts(html: &mut Markup, alerts: &[analyte_store::AlertState], now: Timestamp) {
    html.push_fragment(&row(
        "Active alerts",
        None,
        &if alerts.is_empty() {
            pill(Tone::Good, "None")
        } else {
            pill(Tone::Problem, &alerts.len().to_string())
        },
    ));
    html.push_markup("</dl>");
    if alerts.is_empty() {
        return;
    }
    html.push_markup(concat!(
        "<div class=\"table\"><table><tr><th>Alert</th><th>Machine</th><th>Responder</th>",
        "<th>Raised</th><th>Last value</th></tr>",
    ));
    for alert in alerts {
        let (kind, machine) = crate::operator::alerts::signal(&alert.key);
        html.push_fragment(
            &Markup::new()
                .markup("<tr><td>")
                .fragment(&exact(alert_words(&alert.key), kind))
                .markup("</td><td>")
                .text(machine.unwrap_or("—"))
                .markup("</td><td>")
                .text(&alert.responder)
                .markup("</td><td>")
                .fragment(&alert.raised_at.map_or_else(
                    || Markup::new().text("unknown"),
                    |raised| plain::when_unix(raised, now),
                ))
                .markup("</td><td>")
                .text(
                    alert
                        .value
                        .map_or_else(|| "unknown".into(), |value| value.to_string()),
                )
                .markup("</td></tr>"),
        );
    }
    html.push_markup("</table></div>");
    for message in alerts.iter().filter_map(crate::operator::alerts::message) {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\">")
                .text(message)
                .markup("</p>"),
        );
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
        AlertKey::ParkedItems => "Flagged items need review",
        AlertKey::RestorePaused => "Order sending paused after restore",
        AlertKey::UndeliverableDestination(..) => "Results cannot be delivered",
    }
}

async fn render_orders(
    state: &Context,
    html: &mut Markup,
    until: std::time::Instant,
    paused: bool,
) -> Result<(), Error> {
    let cancelled = state
        .store
        .request(until, |s| s.analyzer_cancelled_count())
        .await?;
    let uncertain = uncertain_orders(state, until).await?;
    html.push_markup("<section><h2>Orders</h2><dl>");
    html.push_fragment(&row(
        "Orders with unknown outcome (check the machine)",
        Some("Uncertain orders"),
        &count_link(uncertain, "/orders", "Machine orders"),
    ));
    html.push_fragment(&row(
        "Orders the analyzer cancelled",
        Some("Analyzer-cancelled AWOSs"),
        &count_link(
            cancelled,
            "/analyzer-cancelled",
            "Review cancellation evidence and reconcile app intent",
        ),
    ));
    html.push_fragment(&row(
        "Order sending paused after restore",
        None,
        &if paused {
            pill(Tone::Attention, "Yes: reconcile with the command line")
        } else {
            pill(Tone::Good, "No")
        },
    ));
    html.push_markup("</dl></section>");
    Ok(())
}

/// A count, marked when above zero, with a link to the page that lists the items.
fn count_link(count: impl Into<u64>, href: &'static str, label: &'static str) -> Markup {
    let count = count.into();
    let tone = if count == 0 {
        Tone::Good
    } else {
        Tone::Attention
    };
    pill(tone, &count.to_string())
        .markup(" <a href=\"")
        .markup(href)
        .markup("\">")
        .markup(label)
        .markup("</a>")
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

async fn render_destinations(
    state: &Context,
    html: &mut Markup,
    now: Timestamp,
) -> Result<(), Error> {
    html.push_markup("<section><h2>Sending to the lab app</h2>");
    let mut oldest = None;
    let mut pending = false;
    let mut unknown_age = false;
    for destination in &state.destinations {
        let health = destination.health(&state.store, now.as_second()).await?;
        pending |= health.pending_count > 0;
        unknown_age |= health.pending_count > 0 && health.oldest_pending_age.is_none();
        oldest = oldest.max(health.oldest_pending_age);
        html.push_fragment(
            &Markup::new()
                .markup("<h3 title=\"Destination\">Lab app destination ")
                .text(destination.id().as_str())
                .markup("</h3><dl>")
                .fragment(&row(
                    "Waiting to be sent",
                    Some("Pending items"),
                    &Markup::new().text(plain::amount(
                        health.pending_count,
                        "result or order message",
                        "results or order messages",
                    )),
                ))
                .fragment(&row(
                    "Order list up to date",
                    Some("Cache fresh and trusted"),
                    &if health.cache_trusted {
                        pill(Tone::Good, "Yes")
                    } else {
                        pill(Tone::Attention, "No")
                    },
                ))
                .fragment(&row(
                    "Last order list check",
                    Some("Cache last successful poll"),
                    &health.last_successful_poll.map_or_else(
                        || Markup::new().text("No successful check recorded"),
                        |time| plain::when_unix(time, now),
                    ),
                ))
                .fragment(&row(
                    "Sending paused",
                    Some("Delivery paused"),
                    &if health.paused {
                        pill(Tone::Attention, "Yes: see Deliveries")
                    } else {
                        pill(Tone::Good, "No")
                    },
                ))
                .markup("</dl>"),
        );
    }
    if state.destinations.is_empty() {
        html.push_markup("<p class=\"muted\">No running machine sends to a lab app.</p>");
    }
    let oldest = if !pending {
        Markup::new().text("None waiting")
    } else if unknown_age {
        Markup::new().text("unknown")
    } else {
        oldest.map_or_else(
            || Markup::new().text("unknown"),
            |age| exact(&plain::duration(age), &format!("{age} seconds")),
        )
    };
    html.push_fragment(
        &Markup::new()
            .markup("<dl>")
            .fragment(&row(
                "Oldest result waiting to be sent",
                Some("Oldest undelivered age"),
                &oldest,
            ))
            .markup("</dl>"),
    );
    for line in super::removed::lines(state).await? {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice\"><strong>Delivery after removal:</strong> ")
                .text(line)
                .markup("</p>"),
        );
    }
    html.push_markup("</section>");
    Ok(())
}

async fn render_machines(
    state: &Context,
    html: &mut Markup,
    until: std::time::Instant,
    now: Timestamp,
) -> Result<(), Error> {
    let mut last_activity = BTreeMap::new();
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
                last_activity
                    .entry(receipt.machine.as_str().to_owned())
                    .and_modify(|time: &mut Timestamp| {
                        *time = (*time).max(receipt.received_at);
                    })
                    .or_insert(receipt.received_at);
            }
        }
    }
    html.push_markup(concat!(
        "<section><div class=\"heading\"><h2>Machines</h2>",
        "<a href=\"/machines\">Machine details</a></div>",
    ));
    if state.machines.is_empty() {
        html.push_markup(concat!(
            "<p class=\"muted\">No machine is running. Add or set one up on the ",
            "<a href=\"/setup\">Setup</a> page.</p></section>",
        ));
        return Ok(());
    }
    html.push_markup(concat!(
        "<div class=\"table\"><table><tr><th>Machine</th><th>Connection</th>",
        "<th>Last data received</th><th>State since</th><th>Connection type</th>",
        "<th>Service</th></tr>",
    ));
    for machine in &state.machines {
        let stored = last_activity.get(machine.id.as_str()).copied();
        let light = LinkLight::of(machine, stored, now);
        html.push_fragment(
            &Markup::new()
                .markup("<tr><td><strong>")
                .text(machine.id.as_str())
                .markup("</strong></td><td>")
                .fragment(&light.state)
                .markup("</td><td>")
                .fragment(&light.last_data)
                .markup("</td><td>")
                .fragment(&light.changed)
                .markup("</td><td>")
                .fragment(&light.kind)
                .markup("</td><td>")
                .fragment(&light.service)
                .markup("</td></tr>"),
        );
    }
    html.push_markup("</table></div></section>");
    Ok(())
}

/// One machine's link light, ready to show. "Last data" is the later of the
/// runtime's last read and the last stored receipt, so it survives a restart. It
/// never shows payloads, sample IDs or peer addresses (safety rule 10).
pub(super) struct LinkLight {
    kind: Markup,
    state: Markup,
    last_data: Markup,
    changed: Markup,
    service: Markup,
}

impl LinkLight {
    pub(super) fn of(machine: &Machine, stored: Option<Timestamp>, now: Timestamp) -> Self {
        let status = machine.status.borrow().clone();
        let tone = match status.light() {
            Light::Live => Tone::Good,
            Light::Waiting => Tone::Attention,
            Light::Off => Tone::Neutral,
        };
        Self {
            kind: exact(link_words(status.link()), status.link().as_str()),
            state: pill(tone, status.state()),
            last_data: status.last_data().max(stored).map_or_else(
                || Markup::new().text("unknown"),
                |time| plain::when(time, now),
            ),
            changed: plain::when(status.changed_at(), now),
            service: if machine.running.load(Ordering::Acquire) {
                exact("Running", "machine owner: running")
            } else {
                exact("Stopped", "machine owner: stopped")
            },
        }
    }

    /// The light as key and value rows, for a machine's own card.
    pub(super) fn rows(&self) -> Markup {
        Markup::new()
            .fragment(&row("Connection", Some("Connection state"), &self.state))
            .fragment(&row("Last data received", None, &self.last_data))
            .fragment(&row(
                "State since",
                Some("Last state change"),
                &self.changed,
            ))
            .fragment(&row("Connection type", Some("Link"), &self.kind))
            .fragment(&row("Service", Some("Machine owner"), &self.service))
    }
}

/// How the machine is attached, in plain words; the technical link kind shows on hover.
const fn link_words(link: Link) -> &'static str {
    match link {
        Link::TcpServer => "Network: the machine connects to Analyte",
        Link::TcpClient => "Network: Analyte connects to the machine",
        Link::Serial => "Serial cable",
        Link::Folder => "Watched folder",
    }
}

async fn render_alert_endpoints(
    store: &analyte_store::Handle,
    configured: &[analyte_model::Identifier],
    html: &mut Markup,
    now: i64,
) -> Result<(), Error> {
    let lines = crate::operator::notification_status::snapshot(store, configured, now).await?;
    html.push_fragment(
        &Markup::new()
            .markup("<dl>")
            .fragment(&row(
                "Alert relays",
                Some("Alert endpoints"),
                &Markup::new().text(lines.len()),
            ))
            .markup("</dl>"),
    );
    if !lines.is_empty() {
        html.push_markup("<ul>");
        for line in lines {
            html.push_fragment(
                &Markup::new()
                    .markup("<li><code>")
                    .text(line)
                    .markup("</code></li>"),
            );
        }
        html.push_markup("</ul>");
    }
    Ok(())
}

#[cfg(test)]
mod webhook_tests {
    use super::*;
    use crate::Error;
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
        let mut html = Markup::new();
        render_alert_endpoints(
            &store,
            &[analyte_model::Identifier::new("missing")?],
            &mut html,
            120,
        )
        .await?;
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
