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
use std::sync::Arc;
mod orders;
pub(super) use orders::orders;
mod raw;
use crate::admin::view::{self, Tab, plain};
pub(super) use raw::RawError;
pub(super) use raw::raw;
use raw::raw_links;

pub(super) const PAGE: usize = 50;
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

pub(super) fn finish(result: Result<Markup, Error>) -> Response {
    match result {
        Ok(html) => Html(html.markup(view::END)).into_response(),
        Err(error) => view::refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "This page could not be shown",
            concat!(
                "Analyte could not read its stored data just now. Reload the page; if this ",
                "keeps happening, give the details below to support.",
            ),
            error,
        ),
    }
}

pub(super) fn next(html: &mut Markup, route: &str, after: impl std::fmt::Display) {
    html.push_fragment(
        &Markup::new()
            .markup("<p class=\"actions-row\"><a class=\"button secondary\" rel=\"next\" href=\"")
            .text(route)
            .markup("?after=")
            .text(after.to_string())
            .markup("\">Next page</a></p>"),
    );
}

/// Says, only while it is true, that order sending is paused after a restore.
async fn pause(state: &Context, html: &mut Markup) -> Result<(), Error> {
    let paused = state
        .store
        .request(deadline()?, |s| s.restore_unreconciled())
        .await?;
    if paused {
        html.push_markup(concat!(
            "<p class=\"notice\" title=\"Order sending paused after restore: true\">",
            "<strong>Order sending is paused after a restore.</strong> ",
            "Ask IT to reconcile orders with the command line; until then no order is sent ",
            "to a machine.</p>",
        ));
    }
    Ok(())
}

pub(super) async fn machines(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
) -> Response {
    finish(
        async {
            let mut html = view::page(Tab::Machines, "Machines");
            pause(&state, &mut html).await?;
            let after = usize::try_from(cursor.after)?;
            let page: Vec<_> = state.machines.iter().skip(after).take(PAGE).collect();
            let now = jiff::Timestamp::now();
            for machine in &page {
                let mut cursor = String::new();
                let until = deadline()?;
                let mut last = None;
                let mut evidence = Markup::new();
                let mut evidence_links = 0usize;
                loop {
                    let rows = state
                        .store
                        .request(until, move |s| s.receipts(&cursor))
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
                                let id = uuid::Uuid::from(receipt.id);
                                evidence.push_fragment(
                                    &Markup::new()
                                        .markup("<li><a href=\"/receipts/")
                                        .text(id)
                                        .markup("/departures\" title=\"Receipt interpretation evidence\">Received ")
                                        .text(plain::time(receipt.received_at))
                                        .markup("</a> <small class=\"muted\" title=\"Receipt ID\">")
                                        .text(id)
                                        .markup("</small></li>"),
                                );
                            }
                        }
                    }
                }
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><h2>")
                        .text(machine.id.as_str())
                        .markup("</h2><dl>")
                        .fragment(&super::overview::LinkLight::of(machine, last, now).rows())
                        .markup("</dl>")
                        .fragment(&machine.departure_html)
                        .markup("<h3>Messages received</h3>"),
                );
                if evidence_links == 0 {
                    html.push_markup("<p class=\"muted\">Nothing yet.</p>");
                } else {
                    html.push_fragment(
                        &Markup::new()
                            .markup("<ul>")
                            .fragment(&evidence)
                            .markup("</ul>"),
                    );
                }
                html.push_markup("</section>");
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

/// Key and value rows for a parked item's targets and their receipt times.
fn metadata(store: &Store, parked: &Parked) -> Result<Markup, analyte_store::OperatorError> {
    let now = jiff::Timestamp::now();
    let mut html = Markup::new();
    for target in parked.targets.iter().take(PAGE) {
        let receipt = receipt_id(target)
            .map(|id| store.receipt_metadata(id))
            .transpose()?
            .flatten();
        html.push_fragment(&view::row(
            "Linked record",
            Some("Target"),
            &Markup::new()
                .markup("<code>")
                .text(target.as_str())
                .markup("</code>"),
        ));
        html.push_fragment(&view::row(
            "Received",
            Some("Receipt time"),
            &receipt.map_or_else(
                || Markup::new().text("Not known (no linked message)"),
                |m| plain::when(m.received_at, now),
            ),
        ));
    }
    if parked.targets.is_empty() {
        html.push_fragment(&view::row(
            "Linked record",
            Some("Receipt"),
            &Markup::new().text("Not recorded"),
        ));
    }
    Ok(html)
}

/// Says when only the first page of a parked item's targets is listed.
fn more_targets(html: &mut Markup, parked: &Parked) {
    if parked.targets.len() > PAGE {
        html.push_markup(concat!(
            "<p class=\"muted\">The first 50 linked records are shown; open the item ",
            "and use Next page for the rest.</p>",
        ));
    }
}

fn kind_and_reason(parked: &Parked) -> Markup {
    let kind = parked.kind.as_str();
    let category = category(kind).map_or_else(
        || Markup::new().text(kind),
        |words| view::exact(words, kind),
    );
    view::row("Category", None, &category).fragment(&view::row(
        "Reason",
        None,
        &Markup::new().text(parked.reason.as_str()),
    ))
}

/// A parked item's category in plain words; an unknown category shows as written.
fn category(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "mapping_finding" => "Test or unit not mapped",
        "interpretation_failure" => "Could not read message",
        "result_reconciliation" => "Result could not be matched to earlier results",
        "incomplete_transfer" => "Message from the machine was incomplete",
        "parked" => "Lab app rejected a delivery",
        "conflict" => "Lab app reported a conflicting delivery",
        "order_version_refused" => "Order from the lab app was refused",
        "law_cancel_refusal_conflict" => "Cancel conflicts with what the machine reported",
        "poll_intent" => "Machine asked for orders; the outcome is not known yet",
        _ => return None,
    })
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
            return finish(Err(error.into()));
        }
    };
    let after = match usize::try_from(cursor.after) {
        Ok(after) => after,
        Err(error) => return finish(Err(error.into())),
    };
    let result = state
        .store
        .request::<_, crate::admin::error::CommandError>(until, move |s| {
            let Some(row) = s.parked_item(id)? else {
                return Ok(None);
            };
            let mut evidence = row.details().map_err(analyte_store::OperatorError::from)?;
            evidence.targets = evidence
                .targets
                .into_iter()
                .skip(after)
                .take(PAGE)
                .collect();
            Ok(Some((metadata(s, &evidence)?, evidence, row.resolution)))
        })
        .await;
    let (meta, evidence, resolution) = match result {
        Ok(Some(row)) => row,
        Ok(None) => {
            return view::refusal(
                StatusCode::NOT_FOUND,
                "Item not found",
                "There is no item to review with this number. Go back to Needs review and pick one from the list.",
                "parked item not found",
            );
        }
        Err(error) => return finish(Err(error.into())),
    };
    let mut html = view::page(Tab::NeedsReview, "Item to review");
    html.push_fragment(
        &Markup::new()
            .markup("<section><dl>")
            .fragment(&kind_and_reason(&evidence))
            .fragment(&meta),
    );
    match resolution {
        None => html.push_fragment(
            &view::row(
                "Status",
                None,
                &view::pill(view::Tone::Attention, "Waiting for review"),
            )
            .markup("</dl>")
            .markup(
                "<p class=\"actions-row\"><a class=\"button\" title=\"Reprocess\" href=\"/parked/",
            )
            .text(id)
            .markup("/reprocess\">Process again with current settings</a></p>"),
        ),
        Some(resolution) => html.push_fragment(
            &view::row(
                "How it was resolved",
                Some("Resolution"),
                &Markup::new().text(&resolution),
            )
            .markup("</dl>"),
        ),
    }
    more_targets(&mut html, &evidence);
    html.push_markup(concat!(
        "</section><section><h2 title=\"Raw evidence\">Original message</h2><p class=\"lead\">",
        "The original bytes are hidden because they may contain patient data. Each time ",
        "they are shown, Analyte first records who asked and why.</p>",
    ));
    raw_links(&mut html, id, &evidence, session);
    html.push_markup("</section>");
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
        .request::<_, crate::admin::error::CommandError>(deadline()?, move |s| {
            let mut html = Markup::new();
            let mut last = None;
            for row in s.parked(after)?.into_iter().take(PAGE) {
                let evidence = row.details().map_err(analyte_store::OperatorError::from)?;
                html.push_fragment(
                    &Markup::new()
                        .markup("<section><div class=\"heading\"><h2><a href=\"/parked/")
                        .text(row.id)
                        .markup("\" title=\"Parked item\">Item ")
                        .text(row.id)
                        .markup("</a></h2>")
                        .fragment(&if row.resolution.is_some() {
                            view::pill(view::Tone::Good, "Resolved")
                        } else {
                            view::pill(view::Tone::Attention, "Waiting for review")
                        })
                        .markup("</div><dl>")
                        .fragment(&kind_and_reason(&evidence))
                        .fragment(&metadata(s, &evidence)?)
                        .markup("</dl>"),
                );
                more_targets(&mut html, &evidence);
                html.push_markup("</section>");
                last = Some(row.id);
            }
            Ok::<_, crate::admin::error::CommandError>((html, last))
        })
        .await?;
    let mut html = view::page(Tab::NeedsReview, "Needs review");
    html.push_markup(concat!(
        "<p class=\"lead\" title=\"Parked items\">Messages and results Analyte kept but ",
        "could not process on its own. Nothing here is deleted; each item waits until ",
        "someone reviews it. Times are when the message was received.</p>",
    ));
    if last.is_none() {
        html.push_markup("<p class=\"muted\">Nothing needs review.</p>");
    }
    html.push_fragment(&rows);
    if let Some(last) = last {
        next(&mut html, "/parked", last);
    }
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner review 2026-11-19: parked categories read in plain words, and the
    // stored category stays on hover; an unknown one shows exactly as written.
    #[test]
    fn categories_read_plainly_and_keep_the_stored_value() -> Result<(), analyte_model::TextError> {
        let parked = |kind: &str| -> Result<Parked, analyte_model::TextError> {
            Ok(Parked {
                kind: Identifier::new(kind)?,
                reason: analyte_model::Reason::new("synthetic reason")?,
                targets: Vec::new(),
            })
        };
        let mapped = kind_and_reason(&parked("mapping_finding")?);
        assert!(mapped.contains(
            "Category</dt><dd><span title=\"mapping_finding\">Test or unit not mapped</span></dd>"
        ));
        let failed = kind_and_reason(&parked("interpretation_failure")?);
        assert!(failed.contains(">Could not read message</span>"));
        let unknown = kind_and_reason(&parked("future_kind")?);
        assert!(unknown.contains("Category</dt><dd>future_kind</dd>"));
        Ok(())
    }
}
