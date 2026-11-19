//! Decides whether everything is OK: one short line per thing that needs someone, each with the page that fixes it.
//!
//! It only reads what the overview already measured and the alerts the alert
//! rules raised; it adds no thresholds of its own. Lines hold names and counts,
//! never payloads or sample IDs (safety rule 10).
use crate::admin::markup::Markup;
use crate::admin::view::{Tab, plain};
use analyte_model::Identifier;
use analyte_store::{AlertKey, AlertState, Undeliverable};

/// What the overview measured that can need someone.
#[derive(Default)]
pub(super) struct Facts {
    /// Alerts raised by the configured alert rules and not yet cleared.
    pub(super) alerts: Vec<AlertState>,
    /// Items on Needs review that nobody has resolved.
    pub(super) review: u64,
    /// Machines whose service in Analyte has stopped.
    pub(super) stopped: Vec<Identifier>,
    /// Running machines whose link is off, with the state the Machines page shows.
    pub(super) unlinked: Vec<(Identifier, &'static str)>,
    /// Lab apps whose sending is paused, with how many items wait for each.
    pub(super) paused: Vec<(Identifier, u64)>,
    /// Lab apps Analyte cannot send to now, why, and how many items wait.
    pub(super) blocked: Vec<(Identifier, Undeliverable, u64)>,
    /// Orders whose outcome on the machine is not known.
    pub(super) uncertain: u64,
    /// Orders a machine cancelled on its own.
    pub(super) cancelled: u64,
    /// Free space is at or below the reserve, so receiving is refused.
    pub(super) disk_full: bool,
    /// Order sending is paused until IT reconciles orders after a restore.
    pub(super) restore_paused: bool,
}

/// A part of the overview page itself that a line can point to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Section {
    Sending,
    Orders,
    Disk,
}

impl Section {
    /// The section's `id` on the overview.
    pub(super) const fn id(self) -> &'static str {
        match self {
            Self::Sending => "sending",
            Self::Orders => "orders",
            Self::Disk => "disk",
        }
    }

    const fn href(self) -> &'static str {
        match self {
            Self::Sending => "/#sending",
            Self::Orders => "/#orders",
            Self::Disk => "/#disk",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Sending => "Sending",
            Self::Orders => "Orders",
            Self::Disk => "Disk",
        }
    }
}

/// Where someone goes to fix a thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fix {
    Page(Tab),
    Section(Section),
    /// The Lab app page under Setup, where each lab app's address and keys are set.
    LabApps,
}

impl Fix {
    const fn href(self) -> &'static str {
        match self {
            Self::Page(tab) => tab.href(),
            Self::Section(section) => section.href(),
            Self::LabApps => crate::admin::setup::LAB_APPS,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Page(tab) => tab.label(),
            Self::Section(section) => section.label(),
            Self::LabApps => "Lab apps",
        }
    }
}

/// What a line is about; an alert about something already listed adds no second line.
#[derive(Debug, PartialEq, Eq)]
enum Subject {
    Disk,
    Blocked(Identifier, Undeliverable),
    Paused(Identifier),
    Stopped(Identifier),
    Unlinked(Identifier),
    Silent(Identifier),
    Review,
    Uncertain,
    Cancelled,
    RestorePaused,
    Waiting,
    OrderList,
}

/// One thing that needs someone: a short sentence and where to fix it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Thing {
    subject: Subject,
    pub(super) text: String,
    pub(super) fix: Fix,
}

impl Thing {
    fn new(subject: Subject, text: String, fix: Fix) -> Self {
        Self { subject, text, fix }
    }
}

/// Every thing that needs someone now, most urgent first; empty means all is well.
/// Direct measurements come first; an alert adds a line only for what they do not
/// already say.
pub(super) fn things(facts: &Facts) -> Vec<Thing> {
    let mut list = Vec::new();
    if facts.disk_full {
        list.push(Thing::new(
            Subject::Disk,
            "Disk almost full: Analyte refuses new data from machines".into(),
            Fix::Section(Section::Disk),
        ));
    }
    for (destination, reason, waiting) in &facts.blocked {
        list.push(blocked(destination, *reason, Some(*waiting)));
    }
    for (destination, waiting) in &facts.paused {
        list.push(Thing::new(
            Subject::Paused(destination.clone()),
            format!(
                "Sending to lab app {} is paused: {waiting} waiting",
                destination.as_str()
            ),
            Fix::LabApps,
        ));
    }
    for machine in &facts.stopped {
        list.push(Thing::new(
            Subject::Stopped(machine.clone()),
            format!("Analyte stopped working with {}", machine.as_str()),
            Fix::Page(Tab::Machines),
        ));
    }
    for (machine, state) in &facts.unlinked {
        if !facts.stopped.contains(machine) {
            list.push(Thing::new(
                Subject::Unlinked(machine.clone()),
                format!("{}: {state}", machine.as_str()),
                Fix::Page(Tab::Machines),
            ));
        }
    }
    if facts.review > 0 {
        list.push(review(facts.review));
    }
    if facts.uncertain > 0 {
        list.push(uncertain(facts.uncertain));
    }
    if facts.cancelled > 0 {
        list.push(Thing::new(
            Subject::Cancelled,
            format!("Orders cancelled by a machine: {}", facts.cancelled),
            Fix::Page(Tab::CancelledByMachine),
        ));
    }
    if facts.restore_paused {
        list.push(restore_paused());
    }
    for alert in &facts.alerts {
        let thing = from_alert(alert);
        if !list.iter().any(|listed| listed.subject == thing.subject) {
            list.push(thing);
        }
    }
    list
}

/// The line for one active alert, in the same words as the direct measurement it matches.
fn from_alert(alert: &AlertState) -> Thing {
    let value = alert.value;
    match &alert.key {
        AlertKey::DiskReserve => Thing::new(
            Subject::Disk,
            value.map_or_else(
                || "Disk space is low".into(),
                |free| format!("Disk space is low: {} free", plain::bytes(free)),
            ),
            Fix::Section(Section::Disk),
        ),
        AlertKey::UndeliverableDestination(destination, reason) => {
            blocked(destination, *reason, value)
        }
        AlertKey::MachineSilent(machine) => Thing::new(
            Subject::Silent(machine.clone()),
            value.map_or_else(
                || format!("{} has gone quiet", machine.as_str()),
                |seconds| {
                    format!(
                        "{} has sent nothing for {}",
                        machine.as_str(),
                        plain::duration(seconds)
                    )
                },
            ),
            Fix::Page(Tab::Machines),
        ),
        AlertKey::ParkedItems => review(value.unwrap_or_default()),
        AlertKey::UncertainOrders => uncertain(value.unwrap_or_default()),
        AlertKey::RestorePaused => restore_paused(),
        AlertKey::OldestUndelivered => Thing::new(
            Subject::Waiting,
            value.map_or_else(
                || "A result has waited too long to be sent".into(),
                |seconds| {
                    format!(
                        "A result has waited {} to be sent",
                        plain::duration(seconds)
                    )
                },
            ),
            Fix::Section(Section::Sending),
        ),
        AlertKey::OrderCacheStale => Thing::new(
            Subject::OrderList,
            "The order list from the lab app is out of date".into(),
            Fix::Section(Section::Sending),
        ),
    }
}

fn blocked(destination: &Identifier, reason: Undeliverable, waiting: Option<u64>) -> Thing {
    let why = match reason {
        Undeliverable::CredentialsMissing => "keys missing",
        Undeliverable::NotConfigured => "not set up any more",
    };
    let waiting = waiting.map_or_else(String::new, |count| format!(", {count} waiting"));
    Thing::new(
        Subject::Blocked(destination.clone(), reason),
        format!(
            "Cannot send to lab app {}: {why}{waiting}",
            destination.as_str()
        ),
        Fix::LabApps,
    )
}

fn review(count: u64) -> Thing {
    let text = if count == 0 {
        // An alert raised just before the last item was resolved.
        "Items may need review".into()
    } else {
        format!(
            "{} review",
            plain::amount(count, "item needs", "items need")
        )
    };
    Thing::new(Subject::Review, text, Fix::Page(Tab::NeedsReview))
}

fn uncertain(count: u64) -> Thing {
    Thing::new(
        Subject::Uncertain,
        format!("Orders with unknown outcome: {count}"),
        Fix::Page(Tab::MachineOrders),
    )
}

fn restore_paused() -> Thing {
    Thing::new(
        Subject::RestorePaused,
        "Order sending is paused after a restore".into(),
        Fix::Section(Section::Orders),
    )
}

/// The status line at the top of the overview: "All good" in green, or how many
/// things need someone in red, each with a link to fix it.
pub(super) fn status(things: &[Thing]) -> Markup {
    if things.is_empty() {
        return Markup::new().markup(concat!(
            "<div class=\"status good\" role=\"status\"><p class=\"verdict\">All good</p>",
            "<p class=\"muted\">Nothing needs you right now.</p></div>",
        ));
    }
    let count = u64::try_from(things.len()).unwrap_or(u64::MAX);
    let mut html = Markup::new()
        .markup("<div class=\"status bad\" role=\"status\"><p class=\"verdict\">")
        .text(plain::amount(count, "thing needs", "things need"))
        .markup(" you</p><ul class=\"things\">");
    for thing in things {
        html.push_fragment(
            &Markup::new()
                .markup("<li><span>")
                .text(&thing.text)
                .markup("</span> <a href=\"")
                .markup(thing.fix.href())
                .markup("\">")
                .markup(thing.fix.label())
                .markup("</a></li>"),
        );
    }
    html.markup("</ul></div>")
}

/// All a first run shows: no machine is set up yet, so there is one thing to do.
pub(super) fn welcome() -> Markup {
    Markup::new().markup(concat!(
        "<div class=\"welcome\"><h2>Set up your first machine</h2><p>Analyte is running, ",
        "but no machine is set up yet. On the Setup page you choose the machine model, how ",
        "it is connected and the lab app that receives its results.</p>",
        "<p class=\"actions-row\"><a class=\"button\" href=\"/setup\">Set up a machine</a>",
        "</p></div>",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> Identifier {
        Identifier::new(text).expect("test identifiers are valid")
    }

    fn alert(key: AlertKey, value: Option<u64>) -> AlertState {
        AlertState {
            key,
            responder: "duty".into(),
            active: true,
            raised_at: Some(100),
            cleared_at: None,
            evaluated_at: 100,
            value,
        }
    }

    fn only(facts: &Facts) -> (String, &'static str) {
        let list = things(facts);
        assert_eq!(list.len(), 1, "{list:?}");
        let thing = list.first().expect("one thing");
        (thing.text.clone(), thing.fix.href())
    }

    // Owner request 2026-11-19 (overview item 1): nothing to do reads "All good" in green.
    #[test]
    fn nothing_wrong_reads_all_good() {
        let list = things(&Facts::default());
        assert!(list.is_empty());
        let html = status(&list);
        assert!(html.contains(
            "<div class=\"status good\" role=\"status\"><p class=\"verdict\">All good</p>"
        ));
        assert!(!html.contains("things need"));
        assert!(!html.contains("status bad"));
    }

    // Owner request 2026-11-19 (overview item 1): each kind of problem is one
    // short line with a link to the page that fixes it.
    #[test]
    fn each_measured_problem_is_one_line_with_its_fix() {
        let cases: Vec<(Facts, &str, &str)> = vec![
            (
                Facts {
                    review: 2,
                    ..Facts::default()
                },
                "2 items need review",
                "/parked",
            ),
            (
                Facts {
                    review: 1,
                    ..Facts::default()
                },
                "1 item needs review",
                "/parked",
            ),
            (
                Facts {
                    unlinked: vec![(id("cobas-e411"), "Port not open")],
                    ..Facts::default()
                },
                "cobas-e411: Port not open",
                "/machines",
            ),
            (
                Facts {
                    stopped: vec![id("iflash")],
                    unlinked: vec![(id("iflash"), "Not listening")],
                    ..Facts::default()
                },
                "Analyte stopped working with iflash",
                "/machines",
            ),
            (
                Facts {
                    paused: vec![(id("demo"), 4)],
                    ..Facts::default()
                },
                "Sending to lab app demo is paused: 4 waiting",
                "/setup/lab-apps",
            ),
            (
                Facts {
                    blocked: vec![(id("x"), Undeliverable::CredentialsMissing, 3)],
                    ..Facts::default()
                },
                "Cannot send to lab app x: keys missing, 3 waiting",
                "/setup/lab-apps",
            ),
            (
                Facts {
                    blocked: vec![(id("old"), Undeliverable::NotConfigured, 1)],
                    ..Facts::default()
                },
                "Cannot send to lab app old: not set up any more, 1 waiting",
                "/setup/lab-apps",
            ),
            (
                Facts {
                    uncertain: 1,
                    ..Facts::default()
                },
                "Orders with unknown outcome: 1",
                "/orders",
            ),
            (
                Facts {
                    cancelled: 2,
                    ..Facts::default()
                },
                "Orders cancelled by a machine: 2",
                "/analyzer-cancelled",
            ),
            (
                Facts {
                    disk_full: true,
                    ..Facts::default()
                },
                "Disk almost full: Analyte refuses new data from machines",
                "/#disk",
            ),
            (
                Facts {
                    restore_paused: true,
                    ..Facts::default()
                },
                "Order sending is paused after a restore",
                "/#orders",
            ),
        ];
        for (facts, text, href) in cases {
            assert_eq!(only(&facts), (text.to_owned(), href));
        }
    }

    // Owner request 2026-11-19 (overview item 1): active alerts become lines in
    // the same words, using the alert rules' own values.
    #[test]
    fn each_alert_is_one_line_with_its_fix() {
        let cases = [
            (
                alert(AlertKey::MachineSilent(id("cobas-e411")), Some(3 * 3600)),
                "cobas-e411 has sent nothing for 3 h",
                "/machines",
            ),
            (
                alert(AlertKey::MachineSilent(id("lumi")), None),
                "lumi has gone quiet",
                "/machines",
            ),
            (
                alert(AlertKey::OldestUndelivered, Some(7200)),
                "A result has waited 2 h to be sent",
                "/#sending",
            ),
            (
                alert(AlertKey::OrderCacheStale, Some(900)),
                "The order list from the lab app is out of date",
                "/#sending",
            ),
            (
                alert(AlertKey::DiskReserve, Some(1024 * 1024)),
                "Disk space is low: 1 MB free",
                "/#disk",
            ),
            (
                alert(AlertKey::ParkedItems, Some(3)),
                "3 items need review",
                "/parked",
            ),
            (
                alert(AlertKey::UncertainOrders, Some(1)),
                "Orders with unknown outcome: 1",
                "/orders",
            ),
            (
                alert(AlertKey::RestorePaused, Some(1)),
                "Order sending is paused after a restore",
                "/#orders",
            ),
            (
                alert(
                    AlertKey::UndeliverableDestination(
                        id("demo"),
                        Undeliverable::CredentialsMissing,
                    ),
                    Some(4),
                ),
                "Cannot send to lab app demo: keys missing, 4 waiting",
                "/setup/lab-apps",
            ),
        ];
        for (alert, text, href) in cases {
            let facts = Facts {
                alerts: vec![alert],
                ..Facts::default()
            };
            assert_eq!(only(&facts), (text.to_owned(), href));
        }
    }

    // An alert about something already measured adds no second line.
    #[test]
    fn an_alert_never_repeats_a_measured_line() {
        let facts = Facts {
            review: 2,
            disk_full: true,
            blocked: vec![(id("demo"), Undeliverable::CredentialsMissing, 4)],
            alerts: vec![
                alert(AlertKey::ParkedItems, Some(2)),
                alert(AlertKey::DiskReserve, Some(10)),
                alert(
                    AlertKey::UndeliverableDestination(
                        id("demo"),
                        Undeliverable::CredentialsMissing,
                    ),
                    Some(4),
                ),
                alert(
                    AlertKey::UndeliverableDestination(id("demo"), Undeliverable::NotConfigured),
                    Some(4),
                ),
            ],
            ..Facts::default()
        };
        let list = things(&facts);
        let texts: Vec<&str> = list.iter().map(|thing| thing.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Disk almost full: Analyte refuses new data from machines",
                "Cannot send to lab app demo: keys missing, 4 waiting",
                "2 items need review",
                "Cannot send to lab app demo: not set up any more, 4 waiting",
            ]
        );
    }

    // Owner request 2026-11-19 (overview item 1): the red line counts the things
    // and lists each with its link; names are escaped.
    #[test]
    fn problems_read_as_a_red_count_with_links() {
        let facts = Facts {
            review: 2,
            unlinked: vec![(id("<m&1>"), "Not connected (retrying)")],
            uncertain: 1,
            ..Facts::default()
        };
        let html = status(&things(&facts));
        assert!(html.starts_with(
            "<div class=\"status bad\" role=\"status\"><p class=\"verdict\">3 things need you</p>"
        ));
        assert!(html.contains(
            "<li><span>2 items need review</span> <a href=\"/parked\">Needs review</a></li>"
        ));
        assert!(html.contains("<li><span>&lt;m&amp;1&gt;: Not connected (retrying)</span> <a href=\"/machines\">Machines</a></li>"));
        assert!(html.contains("<a href=\"/orders\">Machine orders</a>"));
        let one = status(&things(&Facts {
            cancelled: 1,
            ..Facts::default()
        }));
        assert!(one.contains("1 thing needs you"));
    }

    // Owner request 2026-11-19 (after #260): every line about a lab app (keys
    // missing, sending paused, not set up any more) links to the Lab app page,
    // where its address and keys are set, under its own name.
    #[test]
    fn lab_app_lines_link_to_the_lab_app_page() {
        let facts = Facts {
            paused: vec![(id("demo"), 4)],
            blocked: vec![
                (id("x"), Undeliverable::CredentialsMissing, 3),
                (id("old"), Undeliverable::NotConfigured, 1),
            ],
            alerts: vec![alert(
                AlertKey::UndeliverableDestination(id("y"), Undeliverable::CredentialsMissing),
                Some(2),
            )],
            ..Facts::default()
        };
        let list = things(&facts);
        assert_eq!(list.len(), 4);
        for thing in &list {
            assert_eq!(thing.fix, Fix::LabApps, "{}", thing.text);
        }
        let html = status(&list);
        assert_eq!(
            html.matches("<a href=\"/setup/lab-apps\">Lab apps</a>")
                .count(),
            4,
            "{}",
            &*html
        );
        assert!(!html.contains("<a href=\"/setup\">"));
    }
}
