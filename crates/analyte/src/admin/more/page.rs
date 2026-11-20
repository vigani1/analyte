//! Holds the More pages' view structs: display-ready values the templates show, built from what the store and the loaded program hold.
//!
//! Test codes, result values and key values never reach these structs; sample
//! IDs do, exactly as stored. Full IDs stay on hover or under "Details for support".
use crate::admin::parked::RawRequest;
use crate::admin::view::{
    Column, Tone,
    format::{Line, Moment, Part},
    frame::Frame,
    icon::Icon,
    list::Empty,
    report::{Chip, Fact, Head, Value},
};

/// A status with its own icon, such as the question mark of an order whose
/// outcome is unknown; `note` is what was recorded, shown under it.
pub(super) struct Look {
    pub(super) tone: Tone,
    pub(super) icon: Icon,
    pub(super) word: &'static str,
    /// The stored state, on hover.
    pub(super) exact: String,
    pub(super) note: Option<String>,
}

/// What the Machine orders page says under its title.
pub(super) const ORDERS_LEDE: &str = concat!(
    "Orders from the lab app that Analyte sent to a machine, and what the machine did with ",
    "them. Analyte never sends an order again on its own.",
);

/// The machine orders' columns.
pub(super) const ORDER_COLUMNS: [Column; 7] = [
    Column::text("Last change"),
    Column::text("Order"),
    Column::text("Sample"),
    Column::text("Machine"),
    Column::number("Tests"),
    Column::text("Status"),
    Column::action("Action"),
];

/// GET /orders: the filters, then one row per order sent to a machine.
#[derive(askama::Template)]
#[template(path = "pages/orders/index.html")]
pub(super) struct Orders {
    pub(super) frame: Frame,
    pub(super) chips: Vec<Chip>,
    /// Order sending is paused after a restore.
    pub(super) paused: bool,
    pub(super) rows: Vec<OrderRow>,
    pub(super) empty: Option<Empty>,
    pub(super) next: Option<String>,
}

/// One order on the list.
pub(super) struct OrderRow {
    pub(super) when: Moment,
    /// The lab app's order number.
    pub(super) order: String,
    /// Its lab app, the version sent and Analyte's own number, on hover.
    pub(super) order_exact: String,
    /// From the lab app's copy of the order; none when no copy is kept.
    pub(super) sample: Option<String>,
    pub(super) machine: String,
    pub(super) model: Option<String>,
    /// How many tests were sent; none when an older version did not record them.
    pub(super) tests: Option<usize>,
    pub(super) status: Look,
    /// Its item's history, or Cancelled by machine.
    pub(super) link: Option<(String, &'static str)>,
}

/// GET /awos/{order}/{item}: one item of an order sent to a machine, what the
/// machine reported about it, the lab app's copy and the machine's messages.
#[derive(askama::Template)]
#[template(path = "pages/awos/item.html")]
pub(super) struct OrderItem {
    pub(super) frame: Frame,
    pub(super) head: Head,
    pub(super) order: Vec<Fact>,
    pub(super) item: Vec<Fact>,
    /// The order's other items, each linked to its own page.
    pub(super) others: Vec<(String, String)>,
    /// The lab app's copy: its facts, or a sentence saying why it is not shown.
    pub(super) copy: Vec<Fact>,
    pub(super) copy_note: Option<String>,
    pub(super) messages: Vec<Message>,
    /// "Show the original message" while notes on this page are hidden.
    pub(super) original: Option<Original>,
    /// The notes show: the view was recorded, and the page warns that they
    /// may contain patient data.
    pub(super) shown: bool,
    pub(super) next: Option<String>,
    pub(super) support: Vec<String>,
}

/// The order item page's "Show the original message": the audited form, or
/// none when the request came with the admin key alone, which has no
/// session to record the view with.
pub(super) struct Original {
    pub(super) form: Option<RawRequest>,
}

/// What the order item page says about hidden notes.
pub(super) const NOTES_HIDDEN: &str = concat!(
    "The machine's notes are hidden because free text may contain patient data. Each time ",
    "they are shown, Analyte first records who asked and why.",
);

/// What the order item page says under its title.
pub(super) const ITEM_LEDE: &str = concat!(
    "What the machine accepted and what it actually did are shown separately. Times keep the ",
    "machine's own spelling and time zone; when the machine leaves a time out, the earlier one ",
    "is kept. Messages marked as rejected were not applied.",
);

/// One message from the machine about the item.
pub(super) struct Message {
    /// "Message received" and when Analyte received it.
    pub(super) title: String,
    pub(super) facts: Vec<Fact>,
    pub(super) notes: Notes,
}

/// The free text a machine sent with one message: its notes, or its source
/// as stored when Analyte cannot read it. It may hold patient data, so it
/// shows only after the view was recorded.
pub(super) enum Notes {
    /// The message carried none.
    None,
    /// What is hidden, such as "2 notes from the machine, hidden".
    Hidden(String),
    /// The lines as sent, after the view was recorded.
    Shown(Vec<String>),
}

/// What the Cancelled by machine page says under its title.
pub(super) const CANCELLED_LEDE: &str = concat!(
    "Order items a machine cancelled on its own. Check each one with the lab and tell the ",
    "lab app if a sample needs running again.",
);

/// What to check about items a machine cancelled, said above them.
pub(super) const CANCELLED_NOTICE: &str = concat!(
    "The machine cancelled these tests on its own. That does not cancel them in the lab app, ",
    "does not cancel the order's other tests and does not let Analyte send anything else; the ",
    "lab app is not told automatically. If a cancel request from the lab app is pending, settle ",
    "it with the lab app's own answer: Analyte does not count the machine's cancel as that answer.",
);

/// GET /analyzer-cancelled: one card per order item a machine cancelled.
#[derive(askama::Template)]
#[template(path = "pages/analyzer-cancelled/index.html")]
pub(super) struct Cancelled {
    pub(super) frame: Frame,
    pub(super) cards: Vec<CancelledCard>,
    pub(super) next: Option<String>,
}

/// One order item a machine cancelled: "Sample 261119-018 · cobas-e411 · 2
/// tests", or the order number when no sample is known.
pub(super) struct CancelledCard {
    pub(super) sample: Option<String>,
    pub(super) order: String,
    pub(super) machine: String,
    pub(super) model: Option<String>,
    /// "2 tests"; none when an older version did not record them.
    pub(super) tests: Option<String>,
    /// The lab app, the version sent and Analyte's own number, on hover.
    pub(super) exact: String,
    /// When the machine's cancel was recorded and what the lab app's copy says.
    pub(super) lines: Vec<Line>,
    pub(super) href: String,
}

/// What the Deliveries page says under its title.
pub(super) const DELIVERIES_LEDE: &str = concat!(
    "Results and order messages Analyte sends to a lab app, and how each stands. Each keeps ",
    "the same delivery number on every retry, so the lab app never files it twice.",
);

/// The deliveries' columns.
pub(super) const DELIVERY_COLUMNS: [Column; 7] = [
    Column::text("Queued"),
    Column::text("Sample"),
    Column::text("From"),
    Column::text("To"),
    Column::number("Results"),
    Column::text("Outcome"),
    Column::action("Action"),
];

/// GET /deliveries, and the same list under the Send again dialog.
#[derive(askama::Template)]
#[template(path = "pages/deliveries/index.html")]
pub(super) struct Deliveries {
    pub(super) frame: Frame,
    pub(super) chips: Vec<Chip>,
    pub(super) rows: Vec<DeliveryRow>,
    pub(super) empty: Option<Empty>,
    pub(super) next: Option<String>,
    pub(super) dialog: Option<ResendDialog>,
}

/// One delivery on the list.
pub(super) struct DeliveryRow {
    /// When it was queued; none when an older version did not record it.
    pub(super) queued: Option<Moment>,
    /// The first sample ID and how many more, with all of them on hover.
    pub(super) sample: Option<(String, String)>,
    /// "Order message" for an order message, which carries no results.
    pub(super) kind: Option<&'static str>,
    pub(super) from: Option<String>,
    pub(super) to: String,
    pub(super) results: Option<usize>,
    pub(super) outcome: Look,
    /// Send again, when Analyte allows it for this delivery's state.
    pub(super) resend: Option<String>,
}

/// The Send again confirmation drawn over the list: a real form with the
/// session's form token, a required reason and the superseded box.
pub(super) struct ResendDialog {
    pub(super) action: String,
    pub(super) title: &'static str,
    pub(super) text: String,
    pub(super) placeholder: &'static str,
    pub(super) hint: &'static str,
    pub(super) button: &'static str,
    pub(super) csrf: String,
    pub(super) cancel: &'static str,
    /// What was typed, shown again after a refusal.
    pub(super) reason: String,
    pub(super) error: Option<&'static str>,
    /// The full delivery ID, for support.
    pub(super) support: Vec<String>,
    pub(super) superseded: bool,
}

/// What the Data keeping page says under its title.
pub(super) const KEEPING_LEDE: &str = concat!(
    "How long Analyte keeps received messages, and holds that keep chosen data until someone ",
    "releases them.",
);

/// What a hold does, said above the list of holds.
pub(super) const HOLDS_TEXT: &str = concat!(
    "A hold keeps data from clean-up until someone releases it; it never expires. Holding an ",
    "order also keeps its messages. When Analyte is not sure what belongs together, it keeps more.",
);

/// GET /retention, and the same page under the Release dialog.
#[derive(askama::Template)]
#[template(path = "pages/retention/index.html")]
pub(super) struct Keeping {
    pub(super) frame: Frame,
    /// Each machine and how long its messages are kept.
    pub(super) policies: Vec<(String, String)>,
    /// What the last clean-up did; none when none has run.
    pub(super) clean_up: Option<Vec<Fact>>,
    pub(super) oldest: Fact,
    pub(super) holds: Vec<HoldRow>,
    pub(super) next: Option<String>,
    /// The form to add a hold, with the session's form token; none with the admin key alone.
    pub(super) form: Option<String>,
    pub(super) dialog: Option<ReleaseDialog>,
}

/// One active hold.
pub(super) struct HoldRow {
    /// What it keeps, with an order number or message time in mono.
    pub(super) title: Line,
    /// Who placed it, when and why, from the audit history.
    pub(super) sub: Line,
    /// The stored target, on hover.
    pub(super) exact: String,
    /// Release, for a signed-in session.
    pub(super) release: Option<String>,
}

/// The Release confirmation drawn over Data keeping: the hold form's own
/// fields, hidden, with a required reason.
pub(super) struct ReleaseDialog {
    pub(super) action: &'static str,
    pub(super) title: &'static str,
    pub(super) text: String,
    pub(super) placeholder: &'static str,
    pub(super) hint: &'static str,
    pub(super) button: &'static str,
    pub(super) csrf: String,
    pub(super) cancel: &'static str,
    pub(super) reason: String,
    pub(super) error: Option<&'static str>,
    /// The hold form's other fields: scope, id, destination and operation.
    pub(super) hidden: Vec<(&'static str, String)>,
}

/// What the Settings page says under its title.
pub(super) const SETTINGS_LEDE: &str = concat!(
    "What Analyte runs with, from its program file and this PC. Read-only: machines and lab ",
    "apps change on Setup, everything else in the program file.",
);

/// GET /settings: what Analyte runs with, read-only and never a key's value.
#[derive(askama::Template)]
#[template(path = "pages/settings/index.html")]
pub(super) struct Settings {
    pub(super) frame: Frame,
    pub(super) engine: Vec<Fact>,
    pub(super) store: Vec<Fact>,
    pub(super) times: Vec<Fact>,
    pub(super) alerts: Vec<Fact>,
    /// Where each key is found: whose key, which one and its status.
    pub(super) keys: Vec<KeyRow>,
    /// Why some keys could not be listed.
    pub(super) problems: Vec<String>,
    pub(super) support: Vec<String>,
}

/// One key on the Settings page, never its value.
pub(super) struct KeyRow {
    /// The admin pages, a lab app or an alert target.
    pub(super) owner: String,
    pub(super) label: &'static str,
    pub(super) tone: Tone,
    pub(super) word: String,
    /// Its credential name or file, on hover.
    pub(super) exact: String,
}
