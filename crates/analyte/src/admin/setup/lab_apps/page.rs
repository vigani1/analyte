//! Holds the Lab apps pages' view structs: display-ready values the templates show, built from the settings files, the key source and what the store says.
//!
//! Every word comes from `view::format`, the Overview's lab app rules or the
//! setup page's own sentences; the templates only loop, test and call macros.
//! A key is never shown, only where its value is found.
use crate::admin::machines::RemoveDialog;
use crate::admin::view::{
    Tone,
    format::{Line, Moment, Part},
    frame::Frame,
    parts::Notice,
    report::{Head, Status},
};
use analyte_i18n::message as tr_args;

/// GET /setup/lab-apps: every lab app the settings write, how it stands and what waits for it.
#[derive(askama::Template)]
#[template(path = "pages/setup/lab_apps/index.html")]
pub(super) struct List {
    pub(super) frame: Frame,
    /// Problems above the list: a name written differently in two places, or
    /// settings that could not be read.
    pub(super) problems: Vec<Notice>,
    pub(super) cards: Vec<Card>,
    /// The settings were read, so a lab app can be added.
    pub(super) add: bool,
    /// Results waiting for lab apps the settings no longer write, under the list.
    pub(super) notices: Vec<Notice>,
    pub(super) support: Vec<String>,
}

/// One lab app on the list.
pub(super) struct Card {
    pub(super) name: String,
    /// Its own page, where it is edited.
    pub(super) href: String,
    pub(super) remove: String,
    pub(super) status: Status,
    /// Drawn in red: Analyte cannot deliver to it now.
    pub(super) bad: bool,
    /// "Last delivered 10:43", "Nothing delivered yet" or "No delivery in the kept records".
    pub(super) delivered: Line,
    pub(super) address: String,
    pub(super) senders: Vec<Sender>,
    pub(super) keys: Vec<Key>,
    /// "4 items waiting · oldest 10:31" or "nothing waiting".
    pub(super) waiting: Line,
    /// What was observed about tries that keep failing.
    pub(super) failing: Option<Line>,
    /// How fresh its order list is, for a lab app a running machine uses.
    pub(super) orders: Option<Orders>,
    /// Where the settings still write it, when not in the lab apps file, with
    /// the exact place on hover.
    pub(super) written: Vec<(String, &'static str)>,
    /// What to do when a key is missing.
    pub(super) todo: Option<Todo>,
    /// What only the startup knows, such as keys set after Analyte started.
    pub(super) note: Option<String>,
}

/// A lab app's order list: its state in words and when it was last checked.
pub(super) struct Orders {
    pub(super) word: &'static str,
    pub(super) checked: Option<Moment>,
}

/// A machine whose settings send results to a lab app, linked to its page.
pub(super) struct Sender {
    pub(super) id: String,
    pub(super) href: String,
}

/// One of a lab app's two keys: where its value is found, never the value.
pub(in crate::admin::setup) struct Key {
    /// "API key" or "Signing key".
    pub(super) label: &'static str,
    /// The empty field's prompt: "Paste the API key".
    pub(super) placeholder: &'static str,
    /// The replacing field's label: "New API key".
    pub(super) replacement: &'static str,
    /// The form field that replaces it.
    pub(super) field: &'static str,
    /// The field's element id on the page.
    pub(super) input: &'static str,
    pub(super) tone: Tone,
    /// "Set on 19 Nov", "Set in the service environment (…)", "Missing" or "Cannot be checked".
    pub(in crate::admin::setup) word: String,
    /// Its credential name and, when stored, the exact time, on hover.
    pub(in crate::admin::setup) exact: String,
    /// How the form takes a value for it.
    pub(super) entry: Entry,
}

/// How a lab app's form takes a value for one key.
pub(super) enum Entry {
    /// A new lab app: the key is asked for.
    New,
    /// A value is found: Replace opens a field; `hint` says what replacing does.
    Replace { hint: &'static str },
    /// No value is found: the field says why it is needed.
    Needed(&'static str),
}

/// What to do, with the page that does it.
pub(super) struct Todo {
    pub(super) text: &'static str,
    pub(super) href: String,
    pub(super) label: &'static str,
}

/// GET /setup/lab-apps/new and /setup/lab-apps/{name}: the form for a new
/// lab app or for one that exists, and the Remove dialog drawn over the latter
/// (GET /setup/lab-apps/{name}/remove).
#[derive(askama::Template)]
#[template(path = "pages/setup/lab_apps/lab_app.html")]
pub(super) struct LabApp {
    pub(super) frame: Frame,
    pub(super) head: Head,
    pub(super) action: String,
    pub(super) csrf: String,
    pub(super) name: Name,
    pub(super) address: String,
    pub(super) freshness: String,
    pub(super) timeout: String,
    /// Send Needs review notices: yes (true) or no (ADR 0038).
    pub(super) notices: bool,
    /// What Analyte knows about those notices now: the lab app declined them,
    /// or the store could not be read. None when there is nothing to say.
    pub(super) notices_status: Option<String>,
    /// "The defaults suit most labs: 30 and 10 seconds."
    pub(super) defaults: String,
    pub(super) senders: Vec<Sender>,
    /// The machines whose own settings still write it; its address waits for them.
    pub(super) kept: Option<String>,
    /// A new lab app's keys are asked for; an existing one's show where they are.
    pub(super) keys: Vec<Key>,
    /// Remove this lab app; none on the form for a new one.
    pub(super) remove: Option<String>,
    pub(super) support: Vec<String>,
    pub(super) dialog: Option<RemoveDialog>,
}

/// A lab app's name on its form: typed for a new one, fixed once it exists,
/// because stored results wait under it.
pub(super) enum Name {
    New(String),
    Fixed(String),
}

pub(super) fn kept_in_settings(machines: &str) -> String {
    tr_args(
        "setup-still-kept-in-the-settings-of-2",
        [("join", machines.into())],
    )
}
