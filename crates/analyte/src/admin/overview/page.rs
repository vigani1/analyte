//! Holds the Overview's view structs: display-ready values the templates show, built from what the overview measured.
//!
//! Every word comes from `view::format` or the attention list; the templates
//! only loop, test and call macros. Nothing here invents a value: a figure or
//! line with no data behind it is left out.
use super::attention::{Overall, Thing};
use crate::admin::view::{
    Column, Tone,
    format::{self, Line, Moment, Part},
    frame::Frame,
    icon::Icon,
    parts::Notice,
};

/// The alerts table's columns.
pub(super) const ALERT_COLUMNS: [Column; 5] = [
    Column::text("Alert"),
    Column::text("Machine"),
    Column::text("Responder"),
    Column::text("Raised"),
    Column::number("Last value"),
];

/// GET / once something is set up or needs someone.
#[derive(askama::Template)]
#[template(path = "pages/overview/index.html")]
pub(super) struct Page {
    pub(super) frame: Frame,
    /// "Thursday 19 November · 10:45".
    pub(super) today: String,
    /// "5 machines · 2 lab apps".
    pub(super) counts: String,
    pub(super) verdict: Verdict,
    pub(super) machines: Vec<MachineNode>,
    pub(super) labs: Vec<LabNode>,
    pub(super) figures: Vec<Figure>,
    pub(super) orders: Vec<OrderRow>,
    /// Order sending is paused after a restore: what to do.
    pub(super) restore: Option<Notice>,
    /// Results waiting for lab apps no set-up machine uses.
    pub(super) kept: Vec<Notice>,
    pub(super) disk: Disk,
    pub(super) alerts: Vec<AlertRow>,
    /// The reason and fix of alerts whose cause varies.
    pub(super) alert_messages: Vec<String>,
    /// One status line per alert relay, for support.
    pub(super) relays: Vec<String>,
}

/// GET / on a first run: no machine is set up, nothing needs anyone and
/// nothing waits for a lab app, so the page only points to the setup wizard.
#[derive(askama::Template)]
#[template(path = "pages/overview/welcome.html")]
pub(super) struct Welcome {
    pub(super) frame: Frame,
    /// The setup wizard's steps, one sentence each.
    pub(super) steps: [&'static str; 4],
}

/// The setup wizard's four steps (`setup::wizard::Step`) as the welcome words them.
pub(super) const WIZARD_STEPS: [&str; 4] = [
    "Pick the machine’s model",
    "Choose how it’s connected and which lab app gets its results",
    "Run a listen-only test. Analyte only reads and sends nothing to the machine",
    "Save, then restart Analyte to use the new settings",
];

/// The answer to "is everything OK?": all good, or the things that need someone.
pub(super) struct Verdict {
    pub(super) tone: Tone,
    pub(super) title: String,
    pub(super) sub: &'static str,
    pub(super) todo: Vec<Todo>,
}

/// One numbered thing that needs someone, with the page that fixes it.
pub(super) struct Todo {
    /// "01", "02"…
    pub(super) number: String,
    pub(super) tone: Tone,
    pub(super) title: String,
    pub(super) detail: Option<String>,
    pub(super) href: &'static str,
    pub(super) label: &'static str,
}

/// The verdict's title for `count` things: "All good", or "3 things need you".
pub(super) fn title(count: usize) -> String {
    if count == 0 {
        return "All good".into();
    }
    format!(
        "{} you",
        format::amount(
            u64::try_from(count).unwrap_or(u64::MAX),
            "thing needs",
            "things need"
        )
    )
}

/// The line under the verdict's title for `count` things.
pub(super) const fn sub(count: usize) -> &'static str {
    if count == 0 {
        "Nothing needs you right now."
    } else {
        "Analyte keeps everything it has received while you fix these."
    }
}

impl Verdict {
    /// "All good" when nothing needs anyone; otherwise how many things do,
    /// red when any of them stops data now.
    pub(super) fn of(things: &[Thing]) -> Self {
        if things.is_empty() {
            return Self {
                tone: Tone::Good,
                title: title(0),
                sub: sub(0),
                todo: Vec::new(),
            };
        }
        Self {
            tone: Overall::of(things).tone(),
            title: title(things.len()),
            sub: sub(things.len()),
            todo: things
                .iter()
                .zip(1u32..)
                .map(|(thing, number)| Todo {
                    number: format!("{number:02}"),
                    tone: thing.tone(),
                    title: thing.text.clone(),
                    detail: thing.detail.clone(),
                    href: thing.fix.href(),
                    label: thing.fix.label(),
                })
                .collect(),
        }
    }
}

/// How a wire between a node and the bus is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wire {
    /// The link is open: data dots travel along it.
    Live,
    /// Waiting for the other side: a dotted line.
    Wait,
    /// Broken: a dashed line with a break mark.
    Cut,
}

impl Wire {
    /// Every kind, in the legend's order.
    pub(super) const ALL: [Self; 3] = [Self::Live, Self::Wait, Self::Cut];

    /// The wire's extra class in the design system.
    pub(super) const fn class(self) -> &'static str {
        match self {
            Self::Live => "",
            Self::Wait => "wire--wait",
            Self::Cut => "wire--cut",
        }
    }

    /// The legend's sample line for this kind.
    pub(super) const fn legend_class(self) -> &'static str {
        match self {
            Self::Live => "legend__line",
            Self::Wait => "legend__line legend__line--wait",
            Self::Cut => "legend__line legend__line--cut",
        }
    }

    /// What the legend calls it: an open link does not prove that data flows.
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Live => "Link open",
            Self::Wait => "Waiting",
            Self::Cut => "Broken",
        }
    }
}

/// One set-up machine on the signal path.
pub(super) struct MachineNode {
    pub(super) id: String,
    /// Its own page.
    pub(super) href: String,
    /// The model's name; none when its definition gives none.
    pub(super) model: Option<String>,
    pub(super) tone: Tone,
    /// The link state as the Machines page words it, or that Analyte stopped.
    pub(super) state: String,
    /// When data last came from it; none before it sent anything.
    pub(super) last_data: Option<Moment>,
    pub(super) kind: &'static str,
    pub(super) kind_icon: Icon,
    pub(super) wire: Wire,
    /// Staggers the travelling dots so neighbouring wires do not move in step.
    pub(super) dot: &'static str,
    /// What Analyte observed, shown while the link is not open.
    pub(super) line: Option<String>,
}

/// One lab app on the signal path.
pub(super) struct LabNode {
    pub(super) name: String,
    pub(super) tone: Tone,
    pub(super) status: &'static str,
    pub(super) wire: Wire,
    /// The mark on a cut line: why Analyte cannot deliver here.
    pub(super) mark: Icon,
    /// Which key is missing, last delivered with what waits, and who sends here.
    pub(super) lines: Vec<Line>,
    /// The exact reason its keys could not be checked, for support; empty otherwise.
    pub(super) support: Vec<String>,
    /// What to do when Analyte cannot deliver here.
    pub(super) todo: Option<&'static str>,
}

/// One figure below the signal path.
pub(super) struct Figure {
    pub(super) label: String,
    /// The figure; ignored when `when` holds it as a time.
    pub(super) value: String,
    pub(super) when: Option<Moment>,
    pub(super) aside: String,
    /// A time after the aside, such as when an order list was checked.
    pub(super) aside_when: Option<Moment>,
    /// A worded figure (such as "Up to date") with its tone's icon; a number when none.
    pub(super) tone: Option<Tone>,
}

/// One line of the Orders card.
pub(super) struct OrderRow {
    pub(super) tone: Tone,
    pub(super) icon: Icon,
    pub(super) text: String,
    pub(super) href: &'static str,
    pub(super) link: &'static str,
}

/// Disk space as the overview shows it, exact bytes on hover.
pub(super) struct Disk {
    pub(super) free: String,
    pub(super) free_exact: String,
    pub(super) reserve: String,
    pub(super) reserve_exact: String,
    /// Free space is above the reserve, so receiving goes on.
    pub(super) enough: bool,
}

impl Disk {
    /// `free` bytes on the data folder's volume and the `reserve` Analyte
    /// always keeps free, each in the largest readable unit with the exact
    /// bytes on hover.
    pub(super) fn of(free: u64, reserve: u64) -> Self {
        Self {
            free: format!("{} free", format::bytes(free)),
            free_exact: format!("{free} bytes free"),
            reserve: format::bytes(reserve),
            reserve_exact: format!("{reserve} bytes"),
            enough: free > reserve,
        }
    }

    pub(super) const fn tone(&self) -> Tone {
        if self.enough {
            Tone::Good
        } else {
            Tone::Problem
        }
    }

    pub(super) const fn status(&self) -> &'static str {
        if self.enough {
            "Enough space"
        } else {
            "Almost full: receiving refused"
        }
    }
}

/// One active alert in the alerts table.
pub(super) struct AlertRow {
    /// What it means, in plain words.
    pub(super) words: &'static str,
    /// The alert rule's signal name, on hover.
    pub(super) kind: &'static str,
    pub(super) machine: String,
    pub(super) responder: String,
    pub(super) raised: Option<Moment>,
    pub(super) value: String,
}
