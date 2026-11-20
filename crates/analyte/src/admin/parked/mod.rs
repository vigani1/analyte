//! Serves Needs review: the list of parked items with its Open and Resolved filters, each item's page, its dialogs and its audited original-message view.
//!
//! Pages show what the store holds: the category, the stored reason, the
//! machine and arrival time of a linked stored message, and the history Analyte
//! recorded. Sample IDs are not stored with an item and test codes and result
//! values never show; raw IDs only appear under "Details for support". Reads stop after
//! [`view::READ_BUDGET`] with the error page in the same look.
mod category;
mod dialog;
mod page;
mod problem;
mod raw;

pub(super) use dialog::{checked, confirm_reprocess, confirm_resolve, reprocess, resolve};
pub(super) use page::RawRequest;
pub(super) use raw::{RAW_VIEW, RawError, raw};

/// What an item is about in a few words, and its tone, as its card on Needs
/// review shows them: for an Activity line.
pub(super) fn headline(details: &Parked) -> (String, Tone) {
    let category = Category::of(details);
    (category.shown, category.tone)
}

use crate::admin::error::CommandError;
use crate::admin::view::list::{Cursor, PAGE, cursor_value, one_page};
use crate::admin::view::report::Chip;
use crate::admin::view::{self, Tab, Tone, format, frame::Frame};
use crate::admin::{Error, server::Context, sessions::Session};
use crate::deadline;
use crate::operator::recovery;
use analyte_model::{Identifier, ReceiptId};
use analyte_store::{Audit, OperatorError, Parked, ParkedCounts, ParkedFilter, Store};
use askama::Template;
use axum::{
    Extension,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use category::{Action, Category};
use jiff::Timestamp;
use page::{
    Button, Card, Dialog, Empty, Entry, Head, Item, List, Original, OriginalTarget, Pair, Resolved,
    Row, Todo, item_href,
};
use problem::Problem;
use serde::Deserialize;
use std::sync::Arc;

/// Which items the list shows; the sidebar's link opens the open ones.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Show {
    #[default]
    Open,
    Resolved,
}

impl Show {
    const fn filter(self) -> ParkedFilter {
        match self {
            Self::Open => ParkedFilter::Open,
            Self::Resolved => ParkedFilter::Resolved,
        }
    }

    /// The list's address for this filter.
    const fn href(self) -> &'static str {
        match self {
            Self::Open => "/parked?show=open",
            Self::Resolved => "/parked?show=resolved",
        }
    }
}

/// GET /parked's query: the filter and the cursor of the page.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListQuery {
    #[serde(default)]
    show: Show,
    #[serde(default, deserialize_with = "cursor_value")]
    after: u64,
}

/// What a linked record is, as far as the store says.
enum Linked {
    /// A stored message: the machine it came from and when it arrived.
    Message { machine: String, at: Timestamp },
    /// A stored message that data keeping removed.
    Removed,
    /// Anything else: a delivery, a transfer, an order or a machine.
    Other,
}

/// The receipt a target names, when it is written as one.
fn receipt_id(target: &Identifier) -> Option<ReceiptId> {
    target
        .as_str()
        .parse::<uuid::Uuid>()
        .ok()
        .and_then(|id| ReceiptId::try_from(id).ok())
}

/// Reads what `target` is without loading any original bytes.
fn linked(store: &Store, target: &Identifier) -> Result<Linked, OperatorError> {
    let Some(id) = receipt_id(target) else {
        return Ok(Linked::Other);
    };
    match store.receipt_metadata(id) {
        Ok(Some(metadata)) => Ok(Linked::Message {
            machine: metadata.machine.as_str().to_owned(),
            at: metadata.received_at,
        }),
        Ok(None) => Ok(Linked::Other),
        Err(OperatorError::Purged) => Ok(Linked::Removed),
        Err(error) => Err(error),
    }
}

/// A page rendered with `status`, or the store-failure page when it could not be written.
fn respond(
    state: &Context,
    session: Option<&Session>,
    id: Option<i64>,
    status: StatusCode,
    html: Result<String, askama::Error>,
) -> Response {
    match html {
        Ok(html) => (status, Html(html)).into_response(),
        Err(error) => {
            Problem::Store(format!("page could not be written: {error}")).page(state, session, id)
        }
    }
}

/// One stored parked item with the facts its list entry needs.
struct Listed {
    id: i64,
    parked: Parked,
    resolution: Option<String>,
    /// What the item's first target is.
    first: Option<Linked>,
}

/// What GET /parked reads in one store request.
struct ListRead {
    counts: ParkedCounts,
    items: Vec<Listed>,
    more: bool,
    /// The items resolved today, newest first, under the first page of open items.
    today: Vec<Listed>,
}

/// How many items resolved today the Open filter lists under the open ones.
const TODAY_ROWS: usize = 10;

pub(super) async fn list(
    State(state): State<Arc<Context>>,
    Query(query): Query<ListQuery>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    let show = query.show;
    let now = Timestamp::now();
    match tokio::time::timeout(view::READ_BUDGET, read_list(&state, query, now)).await {
        Ok(Ok(read)) => {
            let html = list_page(&state, session, show, read, now).render();
            respond(&state, session, None, StatusCode::OK, html)
        }
        Ok(Err(error)) => Problem::of(&error).page(&state, session, None),
        Err(_) => Problem::Time.page(&state, session, None),
    }
}

async fn read_list(state: &Context, query: ListQuery, now: Timestamp) -> Result<ListRead, Error> {
    let after = i64::try_from(query.after)?;
    let filter = query.show.filter();
    // Only the first page of open items lists those resolved today.
    let today = (query.show == Show::Open && after == 0)
        .then(|| format::today_span(now))
        .flatten();
    Ok(state
        .store
        .read::<_, CommandError>(deadline()?, move |s| {
            let counts = s.parked_counts()?;
            let (rows, more) = one_page::<_, CommandError>(
                s.parked_filtered_up_to(after, filter, PAGE + 1)?,
                PAGE,
                |last| Ok(!s.parked_filtered_up_to(last.id, filter, 1)?.is_empty()),
            )?;
            let listed = |rows: Vec<analyte_store::ParkedItem>| {
                rows.into_iter()
                    .map(|row| {
                        let parked = row.details().map_err(OperatorError::from)?;
                        let first = parked.targets.first().map(|t| linked(s, t)).transpose()?;
                        Ok(Listed {
                            id: row.id,
                            parked,
                            resolution: row.resolution,
                            first,
                        })
                    })
                    .collect::<Result<Vec<_>, CommandError>>()
            };
            let items = listed(rows)?;
            let today = match today {
                Some((from, until)) => listed(s.parked_resolved_between(from, until, TODAY_ROWS)?)?,
                None => Vec::new(),
            };
            Ok(ListRead {
                counts,
                items,
                more,
                today,
            })
        })
        .await?)
}

fn list_page(
    state: &Context,
    session: Option<&Session>,
    show: Show,
    read: ListRead,
    now: Timestamp,
) -> List {
    let ListRead {
        counts,
        items,
        more,
        today,
    } = read;
    let next = items.last().filter(|_| more).map(|last| match show {
        Show::Open => format!("/parked?after={}", last.id),
        Show::Resolved => format!("/parked?show=resolved&after={}", last.id),
    });
    let shown = match show {
        Show::Open => counts.open,
        Show::Resolved => counts.resolved,
    };
    let empty = items.is_empty().then(|| empty(show, shown, counts));
    let (mut cards, mut rows) = (Vec::new(), Vec::new());
    for item in items {
        let category = Category::of(&item.parked);
        match show {
            Show::Open => {
                let (machine, arrived) = match item.first {
                    Some(Linked::Message { machine, at }) => {
                        (Some(machine), Some(format::moment(at, now)))
                    }
                    Some(Linked::Removed | Linked::Other) | None => (None, None),
                };
                cards.push(Card {
                    id: item.id,
                    actions: actions(item.id, &item.parked, category.action, Size::Small),
                    category,
                    machine,
                    arrived,
                });
            }
            Show::Resolved => rows.push(row(item, category.shown, &settled_words, now)),
        }
    }
    let resolved = match show {
        Show::Resolved => (!rows.is_empty()).then_some(Resolved {
            heading: None,
            rows,
        }),
        // Under open items only: an empty Open filter says so on its own.
        Show::Open => (!cards.is_empty() && !today.is_empty()).then(|| Resolved {
            heading: Some("Resolved today"),
            rows: today
                .into_iter()
                .map(|item| {
                    let title = Category::of(&item.parked).shown;
                    row(
                        item,
                        title,
                        &|audit| format::settled_action(audit.action.as_str()).into(),
                        now,
                    )
                })
                .collect(),
        }),
    };
    List {
        frame: Frame::new(
            state,
            Tab::NeedsReview,
            "Needs review",
            Some(counts.open),
            session,
            now,
        ),
        chips: [
            Chip {
                href: Show::Open.href(),
                label: "Open",
                count: counts.open,
                current: show == Show::Open,
            },
            Chip {
                href: Show::Resolved.href(),
                label: "Resolved",
                count: counts.resolved,
                current: show == Show::Resolved,
            },
        ],
        cards,
        resolved,
        empty,
        next,
    }
}

/// A resolved item's row: its title, machine and how it was settled, with
/// `what` saying what the recorded action did.
fn row(item: Listed, title: String, what: &dyn Fn(&Audit) -> String, now: Timestamp) -> Row {
    Row {
        id: item.id,
        category: title,
        machine: match item.first {
            Some(Linked::Message { machine, .. }) => Some(machine),
            Some(Linked::Removed | Linked::Other) | None => None,
        },
        settled: settled_line(item.resolution.as_deref(), what, now),
        href: item_href(item.id),
    }
}

/// What a filter with nothing on this page says: no open item is good news;
/// no resolved item is only a fact; a page past the last item says so.
fn empty(show: Show, shown: u64, counts: ParkedCounts) -> Empty {
    let other = |show: Show, count: u64, words: &str| {
        (count > 0).then(|| (show.href().to_owned(), format!("{words} ({count})")))
    };
    match show {
        _ if shown > 0 => Empty {
            good: false,
            title: "No more items",
            text: "This page is past the last item.",
            link: Some((show.href().to_owned(), "Back to the first page".into())),
        },
        Show::Open => Empty {
            good: true,
            title: "Nothing needs review.",
            text: concat!(
                "When Analyte cannot handle a message or result on its own, it keeps it ",
                "here with what to do next.",
            ),
            link: other(Show::Resolved, counts.resolved, "See resolved items"),
        },
        Show::Resolved => Empty {
            good: false,
            title: "No resolved items yet.",
            text: "Items marked resolved or processed again move here and keep their history.",
            link: other(Show::Open, counts.open, "See open items"),
        },
    }
}

/// How big an item's buttons are: small on the list, full size on the item page.
#[derive(Clone, Copy)]
enum Size {
    Small,
    Full,
}

impl Size {
    const fn class(self, kind: Kind) -> &'static str {
        match (self, kind) {
            (Self::Small, Kind::Main) => "btn--primary btn--sm",
            (Self::Small, Kind::Second) => "btn--outline btn--sm",
            (Self::Small, Kind::Quiet) => "btn--quiet btn--sm",
            (Self::Full, Kind::Main) => "btn--primary",
            (Self::Full, Kind::Second) => "btn--outline",
            (Self::Full, Kind::Quiet) => "btn--quiet",
        }
    }
}

/// A button's weight beside the others.
#[derive(Clone, Copy)]
enum Kind {
    Main,
    Second,
    Quiet,
}

/// An open item's buttons: its main action first, then Mark resolved as the
/// second button when the main action is something else, for when the lab
/// decides nothing more is needed. Each opens its confirmation, which asks for
/// a reason. On the list, "Details" opens the item page.
fn actions(id: i64, parked: &Parked, action: Action, size: Size) -> Vec<Button> {
    let resolve = |kind| Button {
        href: format!("/parked/{id}/resolve"),
        label: "Mark resolved",
        class: size.class(kind),
    };
    let mut buttons = Vec::new();
    match action {
        Action::ProcessAgain => buttons.push(Button {
            href: format!("/parked/{id}/reprocess"),
            label: "Process again",
            class: size.class(Kind::Main),
        }),
        // A rejected delivery's target is its delivery ID; anything else gets no button.
        Action::SendAgain => buttons.extend(
            parked
                .targets
                .first()
                .and_then(|target| uuid::Uuid::parse_str(target.as_str()).ok())
                .map(|delivery| Button {
                    href: format!("/deliveries/{delivery}/resend"),
                    label: "Send again",
                    class: size.class(Kind::Main),
                }),
        ),
        Action::Resolve => buttons.push(resolve(Kind::Main)),
    }
    if action != Action::Resolve {
        buttons.push(resolve(Kind::Second));
    }
    if matches!(size, Size::Small) {
        buttons.push(Button {
            href: item_href(id),
            label: "Details",
            class: size.class(Kind::Quiet),
        });
    }
    buttons
}

/// The audit entry a stored resolution holds; none when it cannot be read as one.
fn resolution(stored: Option<&str>) -> Option<Audit> {
    stored.and_then(|text| serde_json::from_str(text).ok())
}

/// A resolved item's line on the list, from its stored resolution; `what`
/// says what the recorded action did.
fn settled_line(
    stored: Option<&str>,
    what: &dyn Fn(&Audit) -> String,
    now: Timestamp,
) -> format::Line {
    resolution(stored).map_or_else(
        || format::Line::text("Resolved; how it was resolved could not be read"),
        |audit| {
            format::settled_line(
                &what(&audit),
                &format::actor(audit.actor.as_str()),
                audit.time,
                audit.reason.as_str(),
                now,
            )
        },
    )
}

/// What an audited action did, in words.
fn settled_words(audit: &Audit) -> String {
    let targets: Vec<&str> = audit.targets.iter().map(Identifier::as_str).collect();
    format::settled(audit.action.as_str(), &targets)
}

/// What GET /parked/{id} and its dialogs read in one store request.
struct ItemRead {
    id: i64,
    parked: Parked,
    resolution: Option<String>,
    counts: ParkedCounts,
    /// Where this page of linked records starts.
    after: usize,
    /// What the first target is, for the head and history on every page.
    first: Option<Linked>,
    /// This page of linked records.
    linked: Vec<(Identifier, Linked)>,
    /// The item's reprocess audit entries, oldest first, and whether older ones exist.
    reprocessed: (Vec<Audit>, bool),
}

/// What item `id`'s page reads, `after` linked records in, in one store request.
struct ItemQuery {
    id: i64,
    after: usize,
    history: Identifier,
    action: Identifier,
}

impl ItemQuery {
    fn new(id: i64, after: u64) -> Result<Self, Error> {
        Ok(Self {
            id,
            after: usize::try_from(after)?,
            history: Identifier::new(&format!("parked:{id}"))?,
            action: Identifier::new(recovery::REPROCESS_ACTION)?,
        })
    }

    /// The item as its page shows it; none when there is no such item.
    fn run(&self, s: &Store) -> Result<Option<ItemRead>, CommandError> {
        let Some(row) = s.parked_item(self.id)? else {
            return Ok(None);
        };
        let parked = row.details().map_err(OperatorError::from)?;
        let first = parked.targets.first().map(|t| linked(s, t)).transpose()?;
        let page = parked
            .targets
            .iter()
            .skip(self.after)
            .take(PAGE)
            .map(|t| Ok((t.clone(), linked(s, t)?)))
            .collect::<Result<Vec<_>, OperatorError>>()?;
        Ok(Some(ItemRead {
            id: self.id,
            counts: s.parked_counts()?,
            reprocessed: s.audits_naming(&self.history, &self.action)?,
            after: self.after,
            first,
            linked: page,
            parked,
            resolution: row.resolution,
        }))
    }
}

/// Reads item `id`, `after` linked records in, as a read that stops with the
/// page; none when there is no such item.
async fn read_item(state: &Context, id: i64, after: u64) -> Result<Option<ItemRead>, Error> {
    let query = ItemQuery::new(id, after)?;
    Ok(state
        .store
        .read::<_, CommandError>(deadline()?, move |s| query.run(s))
        .await?)
}

pub(super) async fn item(
    State(state): State<Arc<Context>>,
    Path(id): Path<i64>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let session = session.as_ref().map(|Extension(session)| session.as_ref());
    let read = tokio::time::timeout(view::READ_BUDGET, read_item(&state, id, cursor.after)).await;
    match read {
        Ok(Ok(Some(read))) => {
            let html = item_page(&state, session, &read, None, Timestamp::now()).render();
            respond(&state, session, Some(id), StatusCode::OK, html)
        }
        Ok(Ok(None)) => Problem::NotFound.page(&state, session, Some(id)),
        Ok(Err(error)) => Problem::of(&error).page(&state, session, Some(id)),
        Err(_) => Problem::Time.page(&state, session, Some(id)),
    }
}

/// The item page, under `dialog` when one is open.
fn item_page(
    state: &Context,
    session: Option<&Session>,
    read: &ItemRead,
    dialog: Option<Dialog>,
    now: Timestamp,
) -> Item {
    let id = read.id;
    let Category {
        shown,
        action,
        guidance,
        why,
        support: reason_support,
        ..
    } = Category::of(&read.parked);
    let open = read.resolution.is_none();
    let settled = resolution(read.resolution.as_deref());
    let total = read.parked.targets.len();
    let end = read.after.saturating_add(read.linked.len());
    let mut head = Head::under(Some(id), true, shown);
    head.lede = match &read.first {
        Some(Linked::Message { machine, at }) => Some(format::arrived(machine, *at, now)),
        Some(Linked::Removed | Linked::Other) | None => None,
    };
    head.status = Some(if open {
        (Tone::Attention, "Open")
    } else {
        (Tone::Good, "Resolved")
    });
    let mut history_notes = Vec::new();
    if read.reprocessed.1 {
        history_notes.push("Older entries are kept but not shown here.");
    }
    if open {
        history_notes.push(concat!(
            "When someone resolves this item or processes it again, the time, the ",
            "action and the reason appear here.",
        ));
    }
    Item {
        frame: Frame::new(
            state,
            Tab::NeedsReview,
            "Item to review",
            Some(read.counts.open),
            session,
            now,
        ),
        head,
        why,
        todo: open.then(|| Todo {
            guidance,
            actions: actions(id, &read.parked, action, Size::Full),
        }),
        linked: linked_pairs(&read.linked, now),
        linked_note: (total > PAGE).then(|| {
            format!(
                "Linked records {} to {end} of {total}.",
                read.after.saturating_add(1)
            )
        }),
        next: (end < total).then(|| format!("/parked/{id}?after={end}")),
        original: original(id, read, session, now),
        history: history(read, settled.as_ref(), now),
        history_notes,
        support: support(read, reason_support, settled.as_ref()),
        dialog,
    }
}

/// One line per linked record on this page.
fn linked_pairs(linked: &[(Identifier, Linked)], now: Timestamp) -> Vec<Pair> {
    if linked.is_empty() {
        return vec![Pair {
            term: "Linked record",
            value: format::Line::text("None recorded"),
        }];
    }
    linked
        .iter()
        .map(|(_, linked)| match linked {
            Linked::Message { machine, at } => Pair {
                term: "Message",
                value: format::arrived(machine, *at, now),
            },
            Linked::Removed => Pair {
                term: "Message",
                value: format::Line::text("Removed by data keeping"),
            },
            Linked::Other => Pair {
                term: "Linked record",
                value: format::Line::text(
                    "Not a stored message; its ID is under Details for support",
                ),
            },
        })
        .collect()
}

/// The "Show the original message" forms: one per linked record on this page,
/// offering the stored message only while it is kept.
fn original(id: i64, read: &ItemRead, session: Option<&Session>, now: Timestamp) -> Original {
    let Some(session) = session.filter(|session| session.authenticated) else {
        return Original {
            signed_in: false,
            targets: Vec::new(),
        };
    };
    let targets = read
        .linked
        .iter()
        .enumerate()
        .map(|(index, (target, linked))| {
            let number = read.after.saturating_add(index).saturating_add(1);
            let (title, departures, buttons) = match linked {
                Linked::Message { machine, at } => (
                    format::arrived(machine, *at, now),
                    receipt_id(target).map(|receipt| {
                        format!("/receipts/{}/departures", uuid::Uuid::from(receipt))
                    }),
                    vec![
                        ("receipt", "Show the message as stored"),
                        ("frames", "Show the pieces as they arrived"),
                    ],
                ),
                Linked::Removed => (
                    format::Line::text("Message removed by data keeping"),
                    None,
                    vec![("frames", "Show the pieces as they arrived")],
                ),
                Linked::Other => (
                    format::Line::text(format!("Linked record {number}")),
                    None,
                    vec![("frames", "Show the pieces as they arrived")],
                ),
            };
            OriginalTarget {
                title,
                departures,
                form: RawRequest {
                    action: format!("/parked/{id}/raw"),
                    csrf: session.csrf.form_value().to_owned(),
                    target: target.as_str().to_owned(),
                    after: 0,
                    offset: 0,
                    field: format!("raw-reason-{number}"),
                    reason: String::new(),
                    buttons,
                },
            }
        })
        .collect();
    Original {
        signed_in: true,
        targets,
    }
}

/// The item's history: the message arriving, each time it was processed again
/// and how it was resolved, as Analyte recorded them.
fn history(read: &ItemRead, settled: Option<&Audit>, now: Timestamp) -> Vec<Entry> {
    let mut entries = Vec::new();
    if let Some(Linked::Message { machine, at }) = &read.first {
        entries.push(Entry {
            when: Some(format::moment(*at, now)),
            text: format!("Arrived from {machine}"),
            who: "Analyte stored the message".into(),
            good: false,
        });
    }
    let entry = |audit: &Audit, good: bool| Entry {
        when: Some(format::moment_unix(audit.time, now)),
        text: settled_words(audit),
        who: format::recorded(audit.actor.as_str(), audit.reason.as_str()),
        good,
    };
    let mut shown = false;
    for audit in &read.reprocessed.0 {
        let resolved = settled.is_some_and(|settled| same(settled, audit));
        shown |= resolved;
        entries.push(entry(audit, resolved));
    }
    match (settled, &read.resolution) {
        (Some(audit), _) if !shown => entries.push(entry(audit, true)),
        (None, Some(_)) => entries.push(Entry {
            when: None,
            text: "Resolved".into(),
            who: "How it was resolved could not be read; the stored record is under Details for support."
                .into(),
            good: true,
        }),
        _ => {}
    }
    entries
}

/// Whether two audit entries record the same action: a reprocess that settled
/// the item is stored as its resolution and in the audit trail.
fn same(a: &Audit, b: &Audit) -> bool {
    a.time == b.time
        && a.actor == b.actor
        && a.action == b.action
        && a.reason == b.reason
        && a.targets == b.targets
}

/// Exact IDs and stored values, for support: `reason` is the stored reason
/// and what goes with it.
fn support(read: &ItemRead, reason: Vec<String>, settled: Option<&Audit>) -> Vec<String> {
    let mut lines = vec![format!("Item {}", read.id)];
    lines.push(format!("Category as stored: {}", read.parked.kind.as_str()));
    lines.extend(reason);
    lines.extend(
        read.linked
            .iter()
            .map(|(target, _)| format!("Linked record: {}", target.as_str())),
    );
    for audit in read.reprocessed.0.iter().chain(settled) {
        lines.push(format!(
            "Recorded at Unix {}: {} by {}",
            audit.time,
            audit.action.as_str(),
            audit.actor.as_str()
        ));
    }
    if settled.is_none()
        && let Some(stored) = &read.resolution
    {
        lines.push(format!("Resolution as stored: {stored}"));
    }
    lines
}

/// The sidebar's open count after an action; it is a convenience, so a page
/// that cannot read it leaves it out, as the error pages do.
async fn open_count(state: &Context) -> Option<u64> {
    let until = deadline().ok()?;
    state
        .store
        .read::<_, OperatorError>(until, Store::parked_counts)
        .await
        .ok()
        .map(|counts| counts.open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_model::{Reason, TextError};

    fn parked(kind: &str, targets: &[&str]) -> Result<Parked, TextError> {
        Ok(Parked {
            kind: Identifier::new(kind)?,
            reason: Reason::new("synthetic")?,
            targets: targets
                .iter()
                .map(|t| Identifier::new(t))
                .collect::<Result<_, _>>()?,
        })
    }

    fn audit(action: &str, targets: &[&str], time: i64) -> Result<Audit, TextError> {
        Ok(Audit {
            actor: Identifier::new("admin-session-1")?,
            action: Identifier::new(action)?,
            targets: targets
                .iter()
                .map(|t| Identifier::new(t))
                .collect::<Result<_, _>>()?,
            time,
            reason: Reason::new("mapping repaired")?,
        })
    }

    // Owner request 2026-11-19 (Needs review item 3): Send again appears only
    // when the rejected delivery's target is a delivery ID; Mark resolved stays.
    #[test]
    fn send_again_needs_a_delivery_id() -> Result<(), TextError> {
        let labels = |item: &Parked| -> Vec<&'static str> {
            actions(7, item, Action::SendAgain, Size::Full)
                .iter()
                .map(|b| b.label)
                .collect()
        };
        let delivery = uuid::Uuid::now_v7().to_string();
        assert_eq!(
            labels(&parked("parked", &[&delivery])?),
            ["Send again", "Mark resolved"]
        );
        assert_eq!(labels(&parked("parked", &["order-9"])?), ["Mark resolved"]);
        Ok(())
    }

    // Owner decision 2026-11-19 (admin restyle b1): an item without a linked
    // record says so, and a resolution that cannot be read says so on the list
    // and in the history instead of showing raw JSON.
    #[test]
    fn missing_links_and_unreadable_resolutions_say_so() -> Result<(), TextError> {
        let now = Timestamp::UNIX_EPOCH;
        let none = linked_pairs(&[], now);
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].value.plain(), "None recorded");
        assert_eq!(
            settled_line(Some("resolved"), &settled_words, now).plain(),
            "Resolved; how it was resolved could not be read"
        );
        let read = ItemRead {
            id: 7,
            parked: parked("conflict", &[])?,
            resolution: Some("resolved".into()),
            counts: ParkedCounts::default(),
            after: 0,
            first: None,
            linked: Vec::new(),
            reprocessed: (Vec::new(), false),
        };
        let lines = history(&read, resolution(read.resolution.as_deref()).as_ref(), now);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].good && lines[0].when.is_none());
        assert!(
            support(&read, Vec::new(), None).contains(&"Resolution as stored: resolved".to_owned())
        );
        Ok(())
    }

    // Owner decision 2026-11-19 (admin restyle b1): a reprocess that settled
    // the item is stored both as its resolution and in the audit trail; the
    // history shows it once, as the settling line.
    #[test]
    fn a_settling_reprocess_is_one_history_line() -> Result<(), Box<dyn std::error::Error>> {
        let targets = ["r", "interpretation:r:2", "reprocess:succeeded", "parked:7"];
        let failed = audit(
            "receipt-reprocess",
            &["r", "interpretation:r:1", "reprocess:parked", "parked:7"],
            10,
        )?;
        let settling = audit("receipt-reprocess", &targets, 20)?;
        let stored = serde_json::to_string(&audit("receipt-reprocess", &targets, 20)?)?;
        let read = ItemRead {
            id: 7,
            parked: parked("mapping_finding", &["r"])?,
            resolution: Some(stored),
            counts: ParkedCounts::default(),
            after: 0,
            first: None,
            linked: Vec::new(),
            reprocessed: (vec![failed, settling], false),
        };
        let lines = history(
            &read,
            resolution(read.resolution.as_deref()).as_ref(),
            Timestamp::UNIX_EPOCH,
        );
        let shown: Vec<(&str, bool)> = lines.iter().map(|l| (l.text.as_str(), l.good)).collect();
        assert_eq!(
            shown,
            [
                (
                    "Processed again: reading 1 recorded; it still needs review",
                    false
                ),
                ("Processed again: reading 2 recorded", true)
            ]
        );
        Ok(())
    }
}
