//! Holds the Setup start page's and the add form's view structs: display-ready values the templates show, read from the settings files and the shipped models.
//!
//! Nothing here invents a value: a model the settings do not name reads "Not
//! chosen yet", and a machine that runs nothing yet has no settings version.
use crate::admin::view::parts::{Hover, Notice};
use crate::admin::view::{
    Column,
    format::Part,
    frame::Frame,
    icon::Icon,
    report::{Head, Status},
};
use analyte_i18n::text as tr;

/// The machine table's columns.
pub(super) fn machine_columns() -> [Column; 5] {
    [
        Column::text(crate::admin::view::format::machine()),
        Column::text(crate::admin::view::format::model()),
        Column::text(tr("admin-status")),
        Column::text(tr("admin-settings-version-2")),
        Column::text(""),
    ]
}

/// The supported models' columns.
pub(super) fn model_columns() -> [Column; 3] {
    [
        Column::text(crate::admin::view::format::model()),
        Column::text(tr("setup-how-it-was-checked")),
        Column::text(tr("admin-status")),
    ]
}

/// GET /setup: the lab apps, the machines the settings list and the shipped models.
#[derive(askama::Template)]
#[template(path = "pages/setup/index.html")]
pub(super) struct Index {
    pub(super) frame: Frame,
    pub(super) lab_apps: LabApps,
    /// Results waiting for lab apps no machine uses, or why they could not be counted.
    pub(super) notices: Vec<Notice>,
    /// The machines, or why the settings could not be read.
    pub(super) machines: Result<Vec<Machine>, String>,
    /// The shipped models, or why they could not be listed.
    pub(super) models: Result<Vec<Model>, String>,
}

/// What the start page says about lab apps.
pub(super) enum LabApps {
    /// None is set up yet, so setting up a machine has nowhere to send.
    None,
    /// At least one is set up.
    Some,
    /// The lab apps file could not be read; the text says why.
    Unreadable(String),
}

/// One machine on the start page.
pub(super) struct Machine {
    pub(super) id: String,
    /// Its setup step 1 and its Remove dialog; none for a machine removed but
    /// still running until the restart.
    pub(super) links: Option<(String, String)>,
    /// Its model's name with the definition path on hover; none when not chosen.
    pub(super) model: Option<Hover>,
    pub(super) status: Status,
    /// The running settings version, short with the full digest on hover;
    /// none while it runs nothing.
    pub(super) version: Option<Hover>,
}

/// One shipped model: its name, how it was checked and whether it can be chosen.
pub(super) struct Model {
    pub(super) name: Hover,
    pub(super) checked: Hover,
    /// Ready, Not ready yet or Cannot be read, with why under it.
    pub(super) status: Status,
    pub(super) icon: Icon,
}

/// GET /setup/add: the form for one more machine, with the next free name filled in.
#[derive(askama::Template)]
#[template(path = "pages/setup/add.html")]
pub(super) struct Add {
    pub(super) frame: Frame,
    pub(super) head: Head,
    pub(super) csrf: String,
    pub(super) name: String,
}

/// POST /setup/restart: Analyte is stopping; the page goes back to Setup once it is up again.
#[derive(askama::Template)]
#[template(path = "pages/setup/restart.html")]
pub(super) struct Restart {
    pub(super) frame: Frame,
}
