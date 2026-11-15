use super::{escape, server::Context};
use crate::{Error, deadline};
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
    let mut html = "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Analyte overview</title><h1>Analyte overview</h1><p>Read-only. Operator actions remain CLI-only.</p><dl>".to_owned();
    html.push_str("</dl>");
    html.push_str(super::pages::NAV);
    html.push_str("<dl>");
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
            .ok_or_else(|| crate::invalid("uncertain order count overflow"))?;
    }
    row(&mut html, "Uncertain orders", &uncertain.to_string());
    let now = jiff::Timestamp::now().as_second();
    let mut oldest = None;
    let mut pending = false;
    let mut unknown_age = false;
    for destination in &state.destinations {
        let health = destination.health(&state.store, now).await?;
        pending |= health.pending_count > 0;
        unknown_age |= health.pending_count > 0 && health.oldest_pending_age.is_none();
        oldest = oldest.max(health.oldest_pending_age);
        row(&mut html, "Destination", destination.id().as_str());
        row(
            &mut html,
            "Pending items",
            &health.pending_count.to_string(),
        );
        row(
            &mut html,
            "Cache last successful poll (Unix seconds)",
            &health
                .last_successful_poll
                .map_or_else(|| "unknown".into(), |v| v.to_string()),
        );
        row(
            &mut html,
            "Cache fresh and trusted",
            &health.cache_trusted.to_string(),
        );
        row(
            &mut html,
            "Cache age (seconds)",
            &health
                .last_successful_poll
                .and_then(|time| now.checked_sub(time))
                .filter(|age| *age >= 0)
                .map_or_else(|| "unknown".into(), |age| age.to_string()),
        );
        row(&mut html, "Delivery paused", &health.paused.to_string());
    }
    row(
        &mut html,
        "Oldest undelivered age (seconds)",
        &if !pending {
            "none".into()
        } else if unknown_age {
            "unknown".into()
        } else {
            oldest.map_or_else(|| "unknown".into(), |v| v.to_string())
        },
    );
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
        row(&mut html, "Machine", machine.id.as_str());
        row(
            &mut html,
            "Machine owner",
            if machine.running.load(Ordering::Acquire) {
                "running"
            } else {
                "stopped"
            },
        );
        row(
            &mut html,
            "Connection state",
            "unknown (not inferred from receipt age)",
        );
        row(
            &mut html,
            "Last received activity",
            &last_activity
                .get(machine.id.as_str())
                .map_or_else(|| "unknown".into(), ToString::to_string),
        );
    }
    html.push_str("</dl></html>");
    Ok(html)
}
