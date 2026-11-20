//! Holds the Needs review pages' view structs: display-ready values the templates show, built from what the store holds.
//!
//! Every word comes from `view::format` or the item's category; the templates
//! only loop, test and call macros. Nothing here invents a value: a line with
//! no stored data behind it is left out, and raw IDs only go under "Details for support".
use super::category::Category;
use crate::admin::view::{
    Tone,
    format::{Line, Moment, Part},
    frame::Frame,
    icon::Icon,
    report::{Chip, Head},
};

/// GET /parked: the Open and Resolved filters, then the items the filter shows.
#[derive(askama::Template)]
#[template(path = "pages/parked/index.html")]
pub(super) struct List {
    pub(super) frame: Frame,
    pub(super) chips: [Chip; 2],
    /// The open items, when the Open filter is on.
    pub(super) cards: Vec<Card>,
    /// The resolved items: the Resolved filter's page, or those resolved today
    /// under the open items.
    pub(super) resolved: Option<Resolved>,
    /// What the page says when the filter shows nothing.
    pub(super) empty: Option<Empty>,
    /// The next page of the same filter.
    pub(super) next: Option<String>,
}

/// What the Needs review page says under its title.
pub(super) const LEDE: &str = concat!(
    "Messages and results Analyte kept but could not process on its own. Items stay ",
    "here until someone resolves them; their history is kept.",
);

/// One open item on the list.
pub(super) struct Card {
    pub(super) id: i64,
    pub(super) category: Category,
    /// The machine its message came from, when its target is a stored message.
    pub(super) machine: Option<String>,
    /// When that message arrived.
    pub(super) arrived: Option<Moment>,
    pub(super) actions: Vec<Button>,
}

/// Resolved items as rows, under `heading` when they sit below the open items.
pub(super) struct Resolved {
    pub(super) heading: Option<&'static str>,
    pub(super) rows: Vec<Row>,
}

/// One resolved item on the list: what it was and how it was settled.
pub(super) struct Row {
    pub(super) id: i64,
    pub(super) category: String,
    pub(super) machine: Option<String>,
    pub(super) settled: Line,
    pub(super) href: String,
}

/// What a filter with no items says: good news, with a tick, only for the
/// Open filter; `link` points to the other filter when it has items.
pub(super) struct Empty {
    pub(super) good: bool,
    pub(super) title: &'static str,
    pub(super) text: &'static str,
    pub(super) link: Option<(String, String)>,
}

/// Why an item is here: a plain sentence with no lead, or "Reason: " and the
/// stored reason.
pub(super) struct Why {
    pub(super) lead: &'static str,
    pub(super) text: Hover,
}

/// Words with their exact stored value on hover; `exact` is empty when they are the same.
pub(super) struct Hover {
    pub(super) shown: String,
    pub(super) exact: String,
}

/// A link drawn as a button: where it goes, its words and its kind.
pub(super) struct Button {
    pub(super) href: String,
    pub(super) label: &'static str,
    pub(super) class: &'static str,
}

/// GET /parked/{id}, and the same page under the Mark resolved and Process
/// again dialogs.
#[derive(askama::Template)]
#[template(path = "pages/parked/item.html")]
pub(super) struct Item {
    pub(super) frame: Frame,
    pub(super) head: Head,
    pub(super) why: Why,
    /// What to do and the actions; none once the item is resolved.
    pub(super) todo: Option<Todo>,
    /// One line per linked record on this page.
    pub(super) linked: Vec<Pair>,
    /// Which linked records this page shows, when there are more than one page.
    pub(super) linked_note: Option<String>,
    pub(super) next: Option<String>,
    pub(super) original: Original,
    pub(super) history: Vec<Entry>,
    /// Said under the history: that older entries are not shown, and while the
    /// item is open, what will appear there.
    pub(super) history_notes: Vec<&'static str>,
    /// Exact IDs and stored values, for support.
    pub(super) support: Vec<String>,
    pub(super) dialog: Option<Dialog>,
}

/// What to do about an open item, with its buttons.
pub(super) struct Todo {
    pub(super) guidance: &'static str,
    pub(super) actions: Vec<Button>,
}

/// One key and its value.
pub(super) struct Pair {
    pub(super) term: &'static str,
    pub(super) value: Line,
}

/// The "Show the original message" disclosure: a form per linked record, or
/// the way to sign in when the request came with the admin key alone.
pub(super) struct Original {
    pub(super) signed_in: bool,
    pub(super) targets: Vec<OriginalTarget>,
}

/// One linked record's way to its original bytes.
pub(super) struct OriginalTarget {
    pub(super) title: Line,
    /// "How this message was read", for a stored message.
    pub(super) departures: Option<String>,
    pub(super) form: RawRequest,
}

/// The audited form that shows original bytes, or a machine's notes on an
/// order item's page: one button per way they were kept.
pub(in crate::admin) struct RawRequest {
    pub(in crate::admin) action: String,
    pub(in crate::admin) csrf: String,
    pub(in crate::admin) target: String,
    pub(in crate::admin) after: u64,
    pub(in crate::admin) offset: u64,
    /// The reason field's id, unique on the page.
    pub(in crate::admin) field: String,
    pub(in crate::admin) reason: String,
    /// `source` value and words of each button.
    pub(in crate::admin) buttons: Vec<(&'static str, &'static str)>,
}

/// One line of an item's history.
pub(super) struct Entry {
    pub(super) when: Option<Moment>,
    pub(super) text: String,
    pub(super) who: String,
    /// The line that settled the item.
    pub(super) good: bool,
}

/// A confirmation drawn over the item page.
pub(super) struct Dialog {
    pub(super) action: String,
    pub(super) title: &'static str,
    pub(super) text: &'static str,
    pub(super) placeholder: &'static str,
    pub(super) hint: &'static str,
    pub(super) button: &'static str,
    pub(super) csrf: String,
    pub(super) cancel: String,
    /// What was typed, shown again after a refusal.
    pub(super) reason: String,
    pub(super) error: Option<&'static str>,
}

/// POST /parked/{id}/raw: the bytes someone asked to see, after the view was recorded.
#[derive(askama::Template)]
#[template(path = "pages/parked/raw.html")]
pub(super) struct Raw {
    pub(super) frame: Frame,
    pub(super) head: Head,
    /// "The message as stored" or "The pieces as they arrived from the machine".
    pub(super) what: String,
    /// One line per 16 bytes: hex, then the same bytes as text.
    pub(super) lines: Vec<String>,
    /// The form for the next part, when more bytes follow.
    pub(super) more: Option<RawRequest>,
    pub(super) support: Vec<String>,
}

/// The icon a resolved row shows; the templates name no icon themselves.
pub(super) const RESOLVED: (Icon, Tone) = (Icon::Ok, Tone::Good);
