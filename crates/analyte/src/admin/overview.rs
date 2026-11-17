//! Renders a read-only HTML overview of alerts, disk reserve, uncertain orders, destinations and machines.
use super::{escape, server::Context};
use crate::admin::Error;
use crate::deadline;
use analyte_model::OrderState;
use analyte_store::MachineOrderFilter;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
};

fn row(html: &mut String, name: &str, value: &str) {
    html.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>",
        escape(name),
        escape(value)
    ));
}

pub(super) async fn show(State(state): State<Arc<Context>>) -> Response {
    match render(&state).await {
        Ok(html) => Html(html).into_response(),
        // Evidence and errors stay on the authenticated page, never in logs.
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(format!(
                "<h1>Overview unavailable</h1><p>{}</p>",
                escape(&error.to_string())
            )),
        )
            .into_response(),
    }
}

async fn render(state: &Context) -> Result<String, Error> {
    let until = deadline();
    let directory = state.directory.clone();
    let (paused, free) = state
        .store
        .request(until, move |store| {
            Ok((
                store.restore_unreconciled()?,
                fs4::available_space(&directory)?,
            ))
        })
        .await?;
    let mut html = "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Analyte overview</title><h1>Analyte overview</h1><p>Operator actions require an authenticated session and a reason.</p><dl>".to_owned();
    html.push_str("</dl>");
    html.push_str(super::pages::NAV);
    html.push_str("<dl>");
    let now = jiff::Timestamp::now().as_second();
    render_alerts(state, &mut html, now).await?;
    render_alert_endpoints(&state.store, &state.alert_endpoints, &mut html, now).await?;
    row(
        &mut html,
        "Order sending paused after restore",
        &paused.to_string(),
    );
    row(&mut html, "Free disk bytes", &free.to_string());
    row(&mut html, "Disk reserve bytes", &state.reserve.to_string());
    row(
        &mut html,
        "Disk capacity",
        if free > state.reserve {
            "above reserve"
        } else {
            "reserve reached: receiving refused"
        },
    );
    let cancelled = state
        .store
        .request(until, |s| s.analyzer_cancelled_count())
        .await?;
    html.push_str(&format!("<dt><strong>Analyzer-cancelled AWOSs</strong></dt><dd><strong>{cancelled}</strong> ? <a href=\"/analyzer-cancelled\">Review cancellation evidence and reconcile app intent</a></dd>"));
    render_uncertain_orders(state, &mut html, until).await?;
    render_destinations(state, &mut html, now).await?;
    render_machines(state, &mut html, until).await?;
    html.push_str("</dl></html>");
    Ok(html)
}

async fn render_alerts(state: &Context, html: &mut String, now: i64) -> Result<(), Error> {
    let alerts = crate::operator::alerts::active(&state.store).await?;
    row(html, "Active alerts", &alerts.len().to_string());
    for alert in alerts {
        let (kind, machine) = crate::operator::alerts::signal(&alert.key);
        row(html, "Alert", kind);
        if let Some(machine) = machine {
            row(html, "Alert machine", machine);
        }
        row(html, "Responder", &alert.responder);
        row(
            html,
            "Raised age (seconds)",
            &crate::operator::alerts::age(&alert, now),
        );
        row(
            html,
            "Last alert value",
            &alert
                .value
                .map_or_else(|| "unknown".into(), |value| value.to_string()),
        );
    }
    Ok(())
}

async fn render_uncertain_orders(
    state: &Context,
    html: &mut String,
    until: std::time::Instant,
) -> Result<(), Error> {
    let mut after = 0;
    let mut uncertain = 0u64;
    loop {
        let page = state
            .store
            .request(until, move |store| {
                store.machine_orders(&MachineOrderFilter {
                    after,
                    machine: None,
                    state: Some(OrderState::Uncertain),
                })
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
    row(html, "Uncertain orders", &uncertain.to_string());
    Ok(())
}

async fn render_destinations(state: &Context, html: &mut String, now: i64) -> Result<(), Error> {
    let mut oldest = None;
    let mut pending = false;
    let mut unknown_age = false;
    for destination in &state.destinations {
        let health = destination.health(&state.store, now).await?;
        pending |= health.pending_count > 0;
        unknown_age |= health.pending_count > 0 && health.oldest_pending_age.is_none();
        oldest = oldest.max(health.oldest_pending_age);
        row(html, "Destination", destination.id().as_str());
        row(html, "Pending items", &health.pending_count.to_string());
        row(
            html,
            "Cache last successful poll (Unix seconds)",
            &health
                .last_successful_poll
                .map_or_else(|| "unknown".into(), |v| v.to_string()),
        );
        row(
            html,
            "Cache fresh and trusted",
            &health.cache_trusted.to_string(),
        );
        row(
            html,
            "Cache age (seconds)",
            &health
                .last_successful_poll
                .and_then(|time| now.checked_sub(time))
                .filter(|age| *age >= 0)
                .map_or_else(|| "unknown".into(), |age| age.to_string()),
        );
        row(html, "Delivery paused", &health.paused.to_string());
    }
    row(
        html,
        "Oldest undelivered age (seconds)",
        &if !pending {
            "none".into()
        } else if unknown_age {
            "unknown".into()
        } else {
            oldest.map_or_else(|| "unknown".into(), |v| v.to_string())
        },
    );
    Ok(())
}

async fn render_machines(
    state: &Context,
    html: &mut String,
    until: std::time::Instant,
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
                    .and_modify(|time: &mut jiff::Timestamp| {
                        *time = (*time).max(receipt.received_at);
                    })
                    .or_insert(receipt.received_at);
            }
        }
    }
    for machine in &state.machines {
        row(html, "Machine", machine.id.as_str());
        row(
            html,
            "Machine owner",
            if machine.running.load(Ordering::Acquire) {
                "running"
            } else {
                "stopped"
            },
        );
        row(
            html,
            "Connection state",
            "unknown (not inferred from receipt age)",
        );
        row(
            html,
            "Last received activity",
            &last_activity
                .get(machine.id.as_str())
                .map_or_else(|| "unknown".into(), ToString::to_string),
        );
    }
    Ok(())
}

async fn render_alert_endpoints(
    store: &analyte_store::Handle,
    configured: &[analyte_model::Identifier],
    html: &mut String,
    now: i64,
) -> Result<(), Error> {
    let lines = crate::operator::notification_status::snapshot(store, configured, now).await?;
    row(html, "Alert endpoints", &lines.len().to_string());
    for line in lines {
        row(html, "Alert endpoint delivery", &line);
    }
    Ok(())
}

#[cfg(test)]
mod webhook_tests {
    use super::*;
    use crate::Error;
    #[tokio::test]
    async fn adr0014_overview_exposes_failed_and_full_webhook_queue() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let mut store = analyte_store::Handle::start(
            temp.path().join("status.db"),
            analyte_store::Limits {
                reserve_bytes: 0,
                database_bytes: 100_000_000,
                record_bytes: 2_097_152,
                page_rows: 100,
                busy_timeout: std::time::Duration::from_millis(100),
            },
            Box::new(crate::runtime::space::Volume),
            8,
            crate::test_startup_deadline()?,
        )
        .await?;
        store
            .request(deadline(), |s| {
                let endpoint =
                    analyte_model::Identifier::new("relay").map_err(analyte_store::Error::Text)?;
                s.register_alert_endpoint(&endpoint)?;
                s.evaluate_alert(
                    &analyte_store::AlertKey::ParkedItems,
                    "Duty officer",
                    true,
                    Some(1),
                    100,
                )?;
                s.prepare_alert_event(&endpoint, 100, 60, 1)?;
                s.prepare_alert_event(&endpoint, 100, 60, 1)?;
                let event = s
                    .claim_alert_attempt(&endpoint, 100, 1, 5, 10)?
                    .ok_or(analyte_store::Error::Corruption)?;
                s.fail_alert_attempt(event.id(), analyte_store::AlertFailure::Network)
            })
            .await?;
        let mut html = String::new();
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
        assert!(html.contains("alert_endpoint=missing state=unavailable pending_count=unknown"));
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
        store.shutdown(deadline()).await?;
        Ok(())
    }
}
