//! Holds Activity's view structs: the filters, today's counts and the lines grouped by day, as the template shows them.
use crate::admin::view::{Tone, format::Moment, frame::Frame, icon::Icon, list::Empty};
use analyte_i18n::text as tr;

/// What the Activity page says under its title.
pub(super) fn activity_lede() -> &'static str {
    tr("admin-activity-lede")
}

/// GET /activity: the machine and type filters, today's count, the lines
/// grouped by day, newest first, and the link to earlier lines.
#[derive(askama::Template)]
#[template(path = "pages/activity/index.html")]
pub(super) struct Feed {
    pub(super) frame: Frame,
    pub(super) machines: Vec<MachineOption>,
    /// Whether the page lists every try of one delivery instead of the
    /// feed; it then has no filters.
    pub(super) one_delivery: bool,
    /// The type shown, which the machine filter's Show keeps.
    pub(super) show: &'static str,
    pub(super) chips: Vec<TypeChip>,
    /// How many lines today holds for the machine chosen, and "event" or
    /// "events"; none when the PC's day could not be worked out.
    pub(super) today: Option<(u64, &'static str)>,
    pub(super) days: Vec<Day>,
    pub(super) empty: Option<Empty>,
    /// Show earlier activity.
    pub(super) earlier: Option<String>,
}

/// One machine in the machine filter: its ID, with its model beside it.
pub(super) struct MachineOption {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) selected: bool,
}

/// One type filter: a button that shows the lines of its type, with today's
/// count; the one shown is drawn pressed.
pub(super) struct TypeChip {
    pub(super) value: &'static str,
    pub(super) label: &'static str,
    pub(super) count: Option<u64>,
    pub(super) pressed: bool,
}

/// The lines of one day, under its divider.
pub(super) struct Day {
    pub(super) label: String,
    pub(super) lines: Vec<FeedLine>,
}

/// One line: when, its icon in its tone with the word a screen reader says,
/// who or what it is about, the rest of the sentence and a link.
pub(super) struct FeedLine {
    /// Its position, which the script uses to mark lines that are new; for
    /// failed tries in a row, one that stays the same while they grow.
    pub(super) key: String,
    /// None when no time was stored.
    pub(super) at: Option<Moment>,
    pub(super) tone: Tone,
    pub(super) icon: Icon,
    pub(super) word: &'static str,
    pub(super) head: String,
    pub(super) rest: String,
    pub(super) link: Option<(String, &'static str)>,
}
