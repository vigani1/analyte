//! Renders bounded operator pages and gates raw evidence behind audited POST requests.
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::admin::{server::Context, sessions::Session};
use crate::deadline;
use analyte_model::{Identifier, ReceiptId};
use analyte_store::{Parked, Store};
use axum::{
    Extension,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::{Arc, atomic::Ordering};

pub(super) const PAGE: usize = 50;
pub(super) const NAV: &str = concat!(
    concat!(
        "<nav><a href=\"/\">Overview</a> <a ",
        "href=\"/machines\">Machines</a> <a ",
    ),
    concat!(
        "href=\"/parked\">Parked items</a> <a ",
        "href=\"/orders\">Machine orders</a> <a ",
    ),
    concat!(
        "href=\"/analyzer-cancelled\">Analyzer ",
        "cancellations</a> <a ",
    ),
    concat!(
        "href=\"/deliveries\">Deliveries</a> <a ",
        "href=\"/retention\">Retention and holds</a></nav>",
    ),
);

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    #[serde(default, deserialize_with = "cursor_value")]
    pub(super) after: u64,
}

fn cursor_value<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(deserializer)?;
    i64::try_from(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub(super) fn start(title: &str) -> Markup {
    Markup::new()
        .markup(concat!(
            "<!doctype html><html lang=\"en\"><meta ",
            "charset=\"utf-8\"><title>",
        ))
        .text(title)
        .markup("</title>")
        .markup(NAV)
        .markup("<h1>")
        .text(title)
        .markup("</h1>")
}

pub(super) fn finish(result: Result<Markup, Error>) -> Response {
    match result {
        Ok(mut html) => {
            html.push_markup("</html>");
            Html(html).into_response()
        }
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(
                Markup::new()
                    .markup(NAV)
                    .markup("<h1>Read failed</h1><p>")
                    .text(error.to_string())
                    .markup("</p>"),
            ),
        )
            .into_response(),
    }
}

pub(super) fn next(html: &mut Markup, route: &str, after: impl std::fmt::Display) {
    html.push_fragment(
        &Markup::new()
            .markup("<a rel=\"next\" href=\"")
            .text(route)
            .markup("?after=")
            .text(after.to_string())
            .markup("\">Next page</a>"),
    );
}

async fn pause(state: &Context, html: &mut Markup) -> Result<(), Error> {
    let paused = state
        .store
        .request(deadline()?, |s| Ok(s.restore_unreconciled()?))
        .await?;
    html.push_markup(if paused {
        concat!(
            "<p>Order sending paused after restore: true. CLI ",
            "reconciliation required.</p>",
        )
    } else {
        "<p>Order sending paused after restore: false.</p>"
    });
    Ok(())
}

pub(super) async fn machines(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(
        async {
            let mut html = start("Machines");
            pause(&state, &mut html).await?;
            let after = usize::try_from(cursor.after)?;
            let page: Vec<_> = state.machines.iter().skip(after).take(PAGE).collect();
            for machine in &page {
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><h2>")
                        .text(machine.id.as_str())
                        .markup("</h2><p>Owner: ")
                        .text(if machine.running.load(Ordering::Acquire) {
                            "running"
                        } else {
                            "stopped"
                        })
                        .markup(". Connection state: unknown.</p>"),
                );
                html.push_fragment(&machine.departure_html);
                let mut cursor = String::new();
                let until = deadline()?;
                let mut last = None;
                let mut evidence_links = 0usize;
                loop {
                    let rows = state
                        .store
                        .request(until, move |s| Ok(s.receipts(&cursor)?))
                        .await?;
                    let Some(end) = rows.last() else {
                        break;
                    };
                    cursor = serde_json::to_string(&end.id)?;
                    for receipt in rows {
                        if receipt.machine == machine.id {
                            last = last.max(Some(receipt.received_at));
                            if evidence_links < PAGE {
                                evidence_links = evidence_links.saturating_add(1);
                                html.push_fragment(
                                    &Markup::new()
                                        .markup("<p><a href=\"/receipts/")
                                        .text(uuid::Uuid::from(receipt.id))
                                        .markup("/departures\">Receipt ")
                                        .text(uuid::Uuid::from(receipt.id))
                                        .markup(" interpretation evidence</a></p>"),
                                );
                            }
                        }
                    }
                }
                html.push_fragment(
                    &Markup::new()
                        .markup("<p>Last received activity: ")
                        .text(last.map_or_else(|| "unknown".into(), |time| time.to_string()))
                        .markup("</p></section>"),
                );
            }
            if !page.is_empty() {
                next(
                    &mut html,
                    "/machines",
                    after
                        .checked_add(page.len())
                        .ok_or(Error::MachineCursorOverflow)?,
                );
            }
            Ok(html)
        }
        .await,
    )
}

fn receipt_id(target: &Identifier) -> Option<ReceiptId> {
    target
        .as_str()
        .parse::<uuid::Uuid>()
        .ok()
        .and_then(|id| ReceiptId::try_from(id).ok())
}

fn metadata(store: &Store, parked: &Parked) -> Result<Markup, analyte_store::Error> {
    let mut html = Markup::new();
    for target in parked.targets.iter().take(PAGE) {
        let receipt = receipt_id(target)
            .map(|id| store.receipt_metadata(id))
            .transpose()?
            .flatten();
        html.push_fragment(
            &Markup::new()
                .markup("<p>Target: ")
                .text(target.as_str())
                .markup(". Receipt time: ")
                .text(receipt.map_or_else(
                    || "unknown (no linked receipt)".into(),
                    |m| m.received_at.to_string(),
                ))
                .markup(".</p>"),
        );
    }
    if parked.targets.is_empty() {
        html.push_markup("<p>Receipt ID and time: not recorded.</p>");
    }
    if parked.targets.len() > PAGE {
        html.push_markup(concat!(
            "<p>First 50 targets shown; use the detail target ",
            "pages for the rest.</p>",
        ));
    }
    Ok(html)
}

pub(super) async fn parked(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    let result = parked_page(&state, cursor).await;

    finish(result)
}

pub(super) async fn detail(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let until = match deadline() {
        Ok(until) => until,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let result = state
        .store
        .request(until, move |s| {
            let Some(row) = s.parked_item(id)? else {
                return Ok(None);
            };
            let mut evidence = row.details()?;
            evidence.targets = evidence
                .targets
                .into_iter()
                .skip(usize::try_from(cursor.after).map_err(analyte_store::Error::Integer)?)
                .take(PAGE)
                .collect();
            Ok(Some((metadata(s, &evidence)?, evidence, row.resolution)))
        })
        .await;
    let (meta, evidence, resolution) = match result {
        Ok(Some(row)) => row,
        Ok(None) => return (StatusCode::NOT_FOUND, "parked item not found").into_response(),
        Err(error) => return finish(Err(error.into())),
    };
    let mut html = start("Parked item");
    html.push_fragment(
        &Markup::new()
            .markup("<p>Category: ")
            .text(evidence.kind.as_str())
            .markup(". Reason: ")
            .text(evidence.reason.as_str())
            .markup(".</p>")
            .fragment(&(meta)),
    );
    if resolution.is_none() {
        html.push_fragment(
            &Markup::new()
                .markup("<p><a href=\"/parked/")
                .text(id)
                .markup("/reprocess\">Reprocess this parked item</a></p>"),
        );
    }
    if let Some(resolution) = resolution {
        html.push_fragment(
            &Markup::new()
                .markup("<p>Resolution: ")
                .text(&resolution)
                .markup("</p>"),
        );
    }
    html.push_markup(concat!(
        "<p>Raw bytes are hidden. Each raw page is ",
        "disclosed only after its audit entry is ",
        "durable.</p>",
    ));
    raw_links(&mut html, id, &evidence, session);
    if !evidence.targets.is_empty() {
        let Some(after) = u64::try_from(evidence.targets.len())
            .ok()
            .and_then(|len| cursor.after.checked_add(len))
        else {
            return finish(Err(Error::ParkedTargetCursorOverflow));
        };
        next(&mut html, &format!("/parked/{id}"), after);
    }
    finish(Ok(html))
}

async fn parked_page(state: &Context, cursor: Cursor) -> Result<Markup, Error> {
    let after = i64::try_from(cursor.after)?;
    let (rows, last) = state
        .store
        .request(deadline()?, move |s| {
            let mut html = Markup::new();
            let mut last = None;
            for row in s.parked(after)?.into_iter().take(PAGE) {
                let evidence = row.details()?;
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><h2><a href=\"/parked/")
                        .text(row.id)
                        .markup("\">Parked ")
                        .text(row.id)
                        .markup("</a></h2><p>Category: ")
                        .text(evidence.kind.as_str())
                        .markup(". Reason: ")
                        .text(evidence.reason.as_str())
                        .markup(".</p>"),
                );
                html.push_fragment(&metadata(s, &evidence)?);
                html.push_markup(if row.resolution.is_some() {
                    "<p>Resolved</p></section>"
                } else {
                    "<p>Unresolved</p></section>"
                });
                last = Some(row.id);
            }
            Ok((html, last))
        })
        .await?;
    let mut html = start("Parked items");
    html.push_markup(concat!(
        concat!(
            "<p>No raw bytes are shown on this list. Times ",
            "below are receipt times; parking time ",
        ),
        "is not recorded by the store.</p>",
    ));
    html.push_fragment(&rows);
    if let Some(last) = last {
        next(&mut html, "/parked", last);
    }
    Ok(html)
}

mod orders;
pub(super) use orders::orders;

mod raw;
pub(super) use raw::raw;
use raw::raw_links;
