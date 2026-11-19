//! Renders bounded operator pages and gates raw evidence behind audited POST requests.
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::admin::{server::Context, sessions::Session};
use crate::deadline;
use analyte_mapping::{Finding, Problem};
use analyte_model::{Identifier, ReceiptId};
use analyte_store::{Parked, ParkedKind, Store};
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

/// The "Next page" link; call it only when rows exist after `after`.
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
            let shown = after
                .checked_add(page.len())
                .ok_or(Error::MachineCursorOverflow)?;
            if shown < state.machines.len() {
                next(&mut html, "/machines", shown);
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
    let category = ParkedKind::from_stored(kind).map_or_else(
        || Markup::new().text(kind),
        |known| view::exact(words(known), kind),
    );
    view::row("Category", None, &category).fragment(&view::row(
        "Reason",
        None,
        &plain::reason(parked.reason.as_str()),
    ))
}

/// The audited action that settles an item from the admin pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    /// Read the original message again with the current settings.
    ProcessAgain,
    /// Queue the rejected delivery again, unchanged.
    SendAgain,
    /// Mark the item resolved after someone checked it; it stays in the history.
    Resolve,
}

/// The category in plain words.
const fn words(kind: ParkedKind) -> &'static str {
    match kind {
        ParkedKind::MappingFinding => "Test or unit not mapped",
        ParkedKind::InterpretationFailure => "Could not read message",
        ParkedKind::ResultReconciliation => "Result could not be matched to earlier results",
        ParkedKind::IncompleteTransfer => "Message from the machine was incomplete",
        ParkedKind::DeliveryRejected => "Lab app rejected a delivery",
        ParkedKind::DeliveryConflict => "Lab app reported a conflicting delivery",
        ParkedKind::OrderVersionRefused => "Order from the lab app was refused",
        ParkedKind::CancelConflict => "Cancel conflicts with what the machine reported",
        ParkedKind::PollIntent => "Machine asked for orders; the outcome is not known yet",
    }
}

/// The action that exists today for this category. Process again reads the
/// item's original message, so only items linked to a message offer it.
const fn main_action(kind: ParkedKind) -> Action {
    match kind {
        ParkedKind::MappingFinding
        | ParkedKind::InterpretationFailure
        | ParkedKind::ResultReconciliation
        | ParkedKind::IncompleteTransfer => Action::ProcessAgain,
        ParkedKind::DeliveryRejected => Action::SendAgain,
        ParkedKind::DeliveryConflict
        | ParkedKind::OrderVersionRefused
        | ParkedKind::CancelConflict
        | ParkedKind::PollIntent => Action::Resolve,
    }
}

/// What someone does about the item. It says only what the category and the
/// problem read back from the stored reason say; a mapping finding names the
/// problem, never the test code, which stays in the protected original message.
const fn guidance(kind: ParkedKind, problem: Option<&Problem>) -> &'static str {
    match kind {
        ParkedKind::MappingFinding if matches!(problem, Some(Problem::UnknownTest)) => concat!(
            "The machine sent a test code that this machine's test mapping does not ",
            "list. Add it to the test mapping, then use Process again.",
        ),
        ParkedKind::MappingFinding
            if matches!(problem, Some(Problem::ValueKindMismatch { .. })) =>
        {
            concat!(
                "The value does not fit what the test mapping expects for this test. ",
                "Check that test in this machine's test mapping, then use Process again.",
            )
        }
        ParkedKind::MappingFinding if matches!(problem, Some(Problem::MissingSample)) => concat!(
            "The message has no sample ID. Check how the machine sends sample IDs; once ",
            "its settings are fixed, use Process again.",
        ),
        ParkedKind::MappingFinding => concat!(
            "Check this machine's test mapping and settings against the reason above, ",
            "then use Process again.",
        ),
        ParkedKind::InterpretationFailure => concat!(
            "Check the machine's settings on the Setup page (machine model and ",
            "connection) against the reason above, then use Process again.",
        ),
        ParkedKind::ResultReconciliation => concat!(
            "Compare this result with the earlier results for the same sample on the ",
            "machine. If a setting caused the mismatch, fix it, then use Process again.",
        ),
        ParkedKind::IncompleteTransfer => concat!(
            "The machine stopped part-way through sending. Check the cable or network ",
            "and have the machine send the results again, then use Process again here.",
        ),
        ParkedKind::DeliveryRejected => {
            "Fix the problem the lab app reported (see Reason), then use Send again."
        }
        ParkedKind::DeliveryConflict => concat!(
            "The lab app already holds different content for this delivery; Analyte does ",
            "not send it again on its own. Check the delivery in the lab app and settle it ",
            "there, then Mark resolved.",
        ),
        ParkedKind::OrderVersionRefused => concat!(
            "The lab app sent an older or changed copy of an order Analyte already has; ",
            "Analyte kept its copy. Correct the order in the lab app, then Mark resolved.",
        ),
        ParkedKind::CancelConflict => concat!(
            "The lab app cancelled tests the machine has already reported on; Analyte sends ",
            "the machine nothing more. Check both on the machine and in the lab app, then ",
            "Mark resolved.",
        ),
        ParkedKind::PollIntent => concat!(
            "Analyte starts no other exchange with this machine until this item is ",
            "resolved. Check on the machine whether this one finished, then Mark resolved.",
        ),
    }
}

/// The plain "What to do" line for an item that waits for review.
fn todo(parked: &Parked) -> Markup {
    let finding = Finding::from_stored(parked.reason.as_str());
    Markup::new()
        .markup("<p class=\"todo\"><strong>What to do:</strong> ")
        .text(ParkedKind::from_stored(parked.kind.as_str()).map_or(
            concat!(
                "Read the reason above and open the item for its linked records. If it is ",
                "not clear what to do, give the item number to support.",
            ),
            |kind| guidance(kind, finding.as_ref().map(|f| &f.problem)),
        ))
        .markup("</p>")
}

/// The buttons for an item's audited actions: its main action first, then Mark
/// resolved as a secondary button when the main action is something else, for
/// when the lab decides nothing more is needed. Each opens its confirmation page,
/// which asks for a reason and posts with the session's CSRF token. An unknown
/// category keeps the Process again it always had.
fn action_buttons(id: i64, parked: &Parked) -> Markup {
    let action =
        ParkedKind::from_stored(parked.kind.as_str()).map_or(Action::ProcessAgain, main_action);
    let main = match action {
        Action::ProcessAgain => Markup::new()
            .markup("<a class=\"button\" title=\"Reprocess\" href=\"/parked/")
            .text(id)
            .markup("/reprocess\">Process again</a>"),
        // A rejected delivery's target is its delivery ID; anything else gets no button.
        Action::SendAgain => parked
            .targets
            .first()
            .and_then(|target| uuid::Uuid::parse_str(target.as_str()).ok())
            .map_or_else(Markup::new, |delivery| {
                Markup::new()
                    .markup(
                        "<a class=\"button\" title=\"Resend this delivery\" href=\"/deliveries/",
                    )
                    .text(delivery)
                    .markup("/resend\">Send again</a>")
            }),
        Action::Resolve => Markup::new(),
    };
    let resolve = Markup::new()
        .markup(match action {
            Action::Resolve => "<a class=\"button\" title=\"Resolve\" href=\"/parked/",
            Action::ProcessAgain | Action::SendAgain => {
                "<a class=\"button secondary\" title=\"Resolve\" href=\"/parked/"
            }
        })
        .text(id)
        .markup("/resolve\">Mark resolved</a>");
    main.fragment(&resolve)
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
            let total = evidence.targets.len();
            evidence.targets = evidence
                .targets
                .into_iter()
                .skip(after)
                .take(PAGE)
                .collect();
            Ok(Some((
                metadata(s, &evidence)?,
                evidence,
                row.resolution,
                total,
            )))
        })
        .await;
    let (meta, evidence, resolution, total) = match result {
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
            .fragment(&todo(&evidence))
            .markup("<p class=\"actions-row\">")
            .fragment(&action_buttons(id, &evidence))
            .markup("</p>"),
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
    let Some(shown) = after.checked_add(evidence.targets.len()) else {
        return finish(Err(Error::ParkedTargetCursorOverflow));
    };
    if shown < total {
        next(&mut html, &format!("/parked/{id}"), shown);
    }
    finish(Ok(html))
}

async fn parked_page(state: &Context, cursor: Cursor) -> Result<Markup, Error> {
    let after = i64::try_from(cursor.after)?;
    let (rows, last, more) = state
        .store
        .request::<_, crate::admin::error::CommandError>(deadline()?, move |s| {
            let mut html = Markup::new();
            let mut last = None;
            let fetched = s.parked(after)?;
            let mut more = fetched.len() > PAGE;
            for row in fetched.into_iter().take(PAGE) {
                let evidence = row.details().map_err(analyte_store::OperatorError::from)?;
                html.push_fragment(&review_item(row.id, &evidence, row.resolution.is_some()));
                last = Some(row.id);
            }
            if let Some(last) = last
                && !more
            {
                more = !s.parked(last)?.is_empty();
            }
            Ok::<_, crate::admin::error::CommandError>((html, last, more))
        })
        .await?;
    let mut html = view::page(Tab::NeedsReview, "Needs review");
    html.push_markup(concat!(
        "<p class=\"lead\" title=\"Parked items\">Messages and results Analyte kept but ",
        "could not process on its own. Nothing here is deleted; each item waits until ",
        "someone reviews it.</p>",
    ));
    if last.is_none() {
        html.push_markup("<p class=\"muted\">Nothing needs review.</p>");
    }
    html.push_fragment(&rows);
    if let Some(last) = last
        && more
    {
        next(&mut html, "/parked", last);
    }
    Ok(html)
}

/// One item on the Needs review list: its category, reason, what to do and its
/// action. Linked record IDs and receipt times stay on the item's own page.
fn review_item(id: i64, parked: &Parked, resolved: bool) -> Markup {
    let html = Markup::new()
        .markup("<section><div class=\"heading\"><h2><a href=\"/parked/")
        .text(id)
        .markup("\" title=\"Parked item\">Item ")
        .text(id)
        .markup("</a></h2>")
        .fragment(&if resolved {
            view::pill(view::Tone::Good, "Resolved")
        } else {
            view::pill(view::Tone::Attention, "Waiting for review")
        })
        .markup("</div><dl>")
        .fragment(&kind_and_reason(parked))
        .markup("</dl>");
    if resolved {
        return html
            .markup("<p class=\"actions-row\"><a href=\"/parked/")
            .text(id)
            .markup("\">Open item</a></p></section>");
    }
    html.fragment(&todo(parked))
        .markup("<p class=\"actions-row\">")
        .fragment(&action_buttons(id, parked))
        .markup("<a href=\"/parked/")
        .text(id)
        .markup("\">Open item</a></p></section>")
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

    // Owner review 2026-11-19 (Needs review item 3): a mapping finding's advice
    // follows the problem read back from its stored reason, never its wording.
    #[test]
    fn mapping_advice_follows_the_problem_read_back_from_the_reason() {
        let advice = |kind: ParkedKind, reason: &str| {
            let finding = Finding::from_stored(reason);
            guidance(kind, finding.as_ref().map(|f| &f.problem))
        };
        let general = advice(ParkedKind::MappingFinding, "synthetic reason");
        for (reason, starts) in [
            (
                "Position { record: 3, field: 3 }: UnknownTest",
                "The machine sent a test code",
            ),
            (
                "Position { record: 4, field: 5 }: ValueKindMismatch { expected: Coded }",
                "The value does not fit",
            ),
            (
                "Position { record: 2, field: 0 }: MissingSample",
                "The message has no sample ID.",
            ),
            (
                "Position { record: 2, field: 4 }: MissingIdentity { setting: \"LAW order\" }",
                general,
            ),
            (
                "departure-conflict at Position { record: 3, field: 25 }: present non-final status contradicts documented final-only scope",
                general,
            ),
        ] {
            assert!(
                advice(ParkedKind::MappingFinding, reason).starts_with(starts),
                "{reason}"
            );
        }
        assert!(general.starts_with("Check this machine's test mapping"));
        assert!(
            advice(
                ParkedKind::InterpretationFailure,
                "Position { record: 3, field: 3 }: UnknownTest"
            )
            .starts_with("Check the machine's settings")
        );
    }
}
