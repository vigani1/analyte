//! Holds the Machines pages' view structs: display-ready values the templates show, built from what Analyte observed or stored.
//!
//! Every word comes from `view::format`, the link light or the setup page;
//! the templates only loop, test and call macros. Nothing here invents a
//! value: a fact with nothing behind it is left out, and raw IDs only go
//! under "Details for support".
use crate::admin::view::{
    Column, Tone,
    format::{Moment, Part, StateLook},
    frame::Frame,
    icon::Icon,
    list::Empty,
    parts::{Hover, Notice},
    report::{Button, Fact, Head, Link, Status, Value},
};

/// The machine list's columns.
pub(super) const MACHINE_COLUMNS: [Column; 6] = [
    Column::text("Machine"),
    Column::text("Connection"),
    Column::text("Last data"),
    Column::text("Connected by"),
    Column::text("Since"),
    Column::text("Sends to"),
];

/// The recent messages' columns.
pub(super) const MESSAGE_COLUMNS: [Column; 5] = [
    Column::text("Time"),
    Column::text("Sample"),
    Column::text("Reading"),
    Column::text("Lab app"),
    Column::text("Details"),
];

/// The result statuses' columns on the departures page.
pub(super) const STATUS_COLUMNS: [Column; 2] = [
    Column::text("Status sent by the machine"),
    Column::text("Shown as"),
];

/// GET /machines: every machine Analyte runs or its settings list.
#[derive(askama::Template)]
#[template(path = "pages/machines/index.html")]
pub(super) struct List {
    pub(super) frame: Frame,
    /// "3 of 5 links open · 1 problem".
    pub(super) summary: String,
    /// Problems that change what the list can show, above it.
    pub(super) problems: Vec<Notice>,
    pub(super) rows: Vec<Row>,
    /// What the page says when it lists no machine.
    pub(super) empty: Option<Empty>,
    /// Said under the list: "Waiting" is normal after a restart, results kept
    /// for removed machines, order sending paused after a restore.
    pub(super) notices: Vec<Notice>,
    /// "Waiting" is normal after a restart; only while a machine waits.
    pub(super) waiting: bool,
    /// The next page of the list.
    pub(super) next: Option<String>,
    /// The exact reasons behind the problems, for support.
    pub(super) support: Vec<String>,
}

/// One machine on the list.
pub(super) struct Row {
    pub(super) id: String,
    pub(super) href: String,
    /// Its model's name, loaded at startup; none when not known.
    pub(super) model: Option<String>,
    pub(super) status: Status,
    /// When data last came from it; none before it sent anything or while it does not run.
    pub(super) last_data: Option<Moment>,
    pub(super) link: Option<Link>,
    /// When its open link opened; none while the link is not open.
    pub(super) since: Option<Moment>,
    pub(super) lab_app: Option<String>,
    /// Record only (ADR 0034): its counts since start; none for a normal machine.
    pub(super) recording: Option<String>,
}

/// GET /machines/{id}, and the same page under the Remove dialog.
#[derive(askama::Template)]
#[template(path = "pages/machines/machine.html")]
pub(super) struct Detail {
    pub(super) frame: Frame,
    pub(super) head: Head,
    pub(super) actions: Vec<Button>,
    pub(super) notices: Vec<Notice>,
    /// What to do for a machine that is not set up yet.
    pub(super) todo: Option<Todo>,
    pub(super) facts: Vec<Fact>,
    /// The departures its settings switch on; none when it does not run.
    pub(super) rules: Option<Vec<Rule>>,
    /// Their declarations, for support.
    pub(super) declarations: Vec<String>,
    pub(super) recent: Vec<Message>,
    /// Said under the recent messages, such as that older ones are kept.
    pub(super) recent_note: Option<String>,
    pub(super) support: Vec<String>,
    pub(super) dialog: Option<RemoveDialog>,
}

/// What to do, with the page that does it.
pub(super) struct Todo {
    pub(super) text: &'static str,
    pub(super) href: String,
    pub(super) label: &'static str,
}

/// One departure the machine's settings switch on: what it means in plain words,
/// its exact rule on hover, and its declaration for support.
#[derive(Clone)]
pub(in crate::admin) struct Rule {
    pub(in crate::admin) words: &'static str,
    pub(in crate::admin) meaning: &'static str,
    pub(in crate::admin) declaration: String,
}

/// One stored message from the machine, as its page lists it.
pub(super) struct Message {
    pub(super) when: Moment,
    /// The first sample ID and how many more, with all of them on hover.
    pub(super) sample: Option<(String, String)>,
    pub(super) reading: Status,
    /// The open Needs review item naming it.
    pub(super) review: Option<Button>,
    /// How its deliveries stand; none when nothing was queued for it.
    pub(super) delivery: Option<Delivery>,
    /// What the delivery cell says when nothing was queued: "Nothing yet"
    /// before the message is read, otherwise "—".
    pub(super) no_delivery: &'static str,
    /// How it was read, for a message that was.
    pub(super) href: Option<String>,
}

/// How a message's deliveries stand: the state that most needs someone, in
/// its tone and icon as every page shows it, its words with how many were
/// delivered, and the count per state on hover.
pub(super) struct Delivery {
    pub(super) look: StateLook,
    pub(super) words: String,
    pub(super) exact: String,
}

/// The Remove dialog drawn over a machine's page; a real form with the
/// session's form token, and Cancel back to the page.
pub(in crate::admin) struct RemoveDialog {
    pub(in crate::admin) action: String,
    pub(in crate::admin) csrf: String,
    pub(in crate::admin) cancel: String,
    pub(in crate::admin) title: String,
    /// What removing does.
    pub(in crate::admin) text: &'static str,
    /// What happens to its waiting results, with their exact count on hover.
    pub(in crate::admin) sending: Hover,
    /// "Still unfinished", or what the list says otherwise.
    pub(in crate::admin) list_title: &'static str,
    pub(in crate::admin) items: Vec<DialogItem>,
    /// Open work was found or could not be counted, so the form says "remove anyway".
    pub(in crate::admin) anyway: bool,
    /// The action's button; none when nothing can be done now, so only Cancel shows.
    pub(in crate::admin) button: Option<&'static str>,
}

/// One line of a dialog's list, with its icon in its tone.
pub(in crate::admin) struct DialogItem {
    pub(in crate::admin) tone: Tone,
    pub(in crate::admin) icon: Icon,
    pub(in crate::admin) text: String,
}

/// What removing a machine did, for its result page.
pub(in crate::admin) struct Removed {
    /// What happens to its lab app and waiting results.
    pub(in crate::admin) kept: String,
    /// Plain lines: the dated copy of the old settings, the settings file.
    pub(in crate::admin) lines: Vec<String>,
    /// What went wrong after the settings were changed: the settings file could
    /// not be moved, the audit could not be written.
    pub(in crate::admin) problems: Vec<String>,
    /// Paths of the dated copies, for support.
    pub(in crate::admin) support: Vec<String>,
    /// The session's form token for Restart now.
    pub(in crate::admin) csrf: String,
}

/// GET /receipts/{id}/departures: how one stored message was read.
#[derive(askama::Template)]
#[template(path = "pages/machines/departures.html")]
pub(super) struct Departures {
    pub(super) frame: Frame,
    pub(super) head: Head,
    pub(super) notices: Vec<Notice>,
    pub(super) facts: Vec<Fact>,
    /// Each departure's declaration as it was switched on for this reading;
    /// none when an older version did not record them.
    pub(super) enabled: Option<Vec<String>>,
    /// The departures used for this message; none when not recorded.
    pub(super) applied: Option<Vec<Applied>>,
    /// Each result part's status as sent and as shown; none when it could not be read.
    pub(super) statuses: Option<Vec<(String, String)>>,
    pub(super) links: Vec<Button>,
    pub(super) support: Vec<String>,
}

/// One departure used for a message: what was received, how it was read and where.
pub(super) struct Applied {
    pub(super) rule: String,
    /// What it means for the result, when it needs saying.
    pub(super) note: Option<Notice>,
    pub(super) facts: Vec<Fact>,
    /// Its declaration as one entry, for the support box.
    pub(super) declaration: Vec<String>,
}
