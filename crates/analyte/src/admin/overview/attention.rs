//! Decides whether everything is OK: one short line per thing that needs someone, each with the page that fixes it.
//!
//! It only reads what the overview already measured and the alerts the alert
//! rules raised; it adds no thresholds of its own. Lines hold names and counts,
//! never payloads or sample IDs (safety rule 10).
use crate::admin::markup::Markup;
use crate::admin::view::{Tab, Tone, format};
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
    /// Machines whose link is off, with the state the Machines page shows and
    /// what Analyte observed with what to check.
    pub(super) unlinked: Vec<(Identifier, &'static str, String)>,
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
    /// The link to the section; the overview template gives each section this `id`.
    pub(super) const fn href(self) -> &'static str {
        match self {
            Self::Sending => "/#sending",
            Self::Orders => "/#orders",
            Self::Disk => "/#disk",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Sending => "Sending",
            Self::Orders => "Orders",
            Self::Disk => "Disk",
        }
    }
}

/// Where someone goes to fix a thing. Lab app lines go to the Lab apps page,
/// where each lab app's address and keys are set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fix {
    Page(Tab),
    Section(Section),
}

impl Fix {
    pub(super) const fn href(self) -> &'static str {
        match self {
            Self::Page(tab) => tab.href(),
            Self::Section(section) => section.href(),
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Page(tab) => tab.label(),
            Self::Section(section) => section.label(),
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

impl Subject {
    /// How serious a line about this subject is: a problem stops data now;
    /// a warning needs a look but nothing is refused.
    const fn tone(&self) -> Tone {
        match self {
            Self::Disk | Self::Blocked(..) | Self::Stopped(_) | Self::Unlinked(_) => Tone::Problem,
            Self::Paused(_)
            | Self::Silent(_)
            | Self::Review
            | Self::Uncertain
            | Self::Cancelled
            | Self::RestorePaused
            | Self::Waiting
            | Self::OrderList => Tone::Attention,
        }
    }
}

/// One thing that needs someone: a short sentence, an optional detail with
/// what to check, and where to fix it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Thing {
    subject: Subject,
    pub(super) text: String,
    pub(super) detail: Option<String>,
    pub(super) fix: Fix,
}

impl Thing {
    fn new(subject: Subject, text: String, fix: Fix) -> Self {
        Self {
            subject,
            text,
            detail: None,
            fix,
        }
    }

    /// The line's tone, from its subject.
    pub(super) const fn tone(&self) -> Tone {
        self.subject.tone()
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
            Fix::Page(Tab::LabApps),
        ));
    }
    for machine in &facts.stopped {
        list.push(Thing::new(
            Subject::Stopped(machine.clone()),
            format!("Analyte stopped working with {}", machine.as_str()),
            Fix::Page(Tab::Machines),
        ));
    }
    for (machine, state, detail) in &facts.unlinked {
        if !facts.stopped.contains(machine) {
            list.push(Thing {
                detail: Some(detail.clone()),
                ..Thing::new(
                    Subject::Unlinked(machine.clone()),
                    format!("{}: {state}", machine.as_str()),
                    Fix::Page(Tab::Machines),
                )
            });
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
                |free| format!("Disk space is low: {} free", format::bytes(free)),
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
                        format::duration(seconds)
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
                        format::duration(seconds)
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
        Fix::Page(Tab::LabApps),
    )
}

fn review(count: u64) -> Thing {
    let text = if count == 0 {
        // An alert raised just before the last item was resolved.
        "Items may need review".into()
    } else {
        format!(
            "{} review",
            format::amount(count, "item needs", "items need")
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

/// All a first run shows: no machine is set up yet, so there is one thing to do.
pub(super) fn welcome() -> Markup {
    Markup::new().markup(concat!(
        "<div class=\"welcome\"><h2>Set up your first machine</h2><p>Analyte is running, ",
        "but no machine is set up yet. On the Setup page you choose the machine model, how ",
        "it is connected and the lab app that receives its results.</p>",
        "<p class=\"actions-row\"><a class=\"button\" href=\"/setup/first\">Set up a machine</a>",
        "</p></div>",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::overview::page::Verdict;
    use askama::Template as _;

    /// The overview's verdict on its own, as the page draws it.
    #[derive(askama::Template)]
    #[template(
        source = "{% import \"components/display.html\" as display %}{% call display::verdict(verdict) %}{% endcall %}",
        ext = "html"
    )]
    struct Shown {
        verdict: Verdict,
    }

    fn status(things: &[Thing]) -> String {
        Shown {
            verdict: Verdict::of(things),
        }
        .render()
        .expect("the verdict renders")
    }

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
        assert!(html.starts_with("<section class=\"verdict verdict--good\""));
        assert!(html.contains("role=\"status\""));
        assert!(html.contains("class=\"verdict__title\">All good</h2>"));
        assert!(!html.contains("things need"));
        assert!(!html.contains("verdict--bad"));
        assert!(!html.contains("class=\"todo\""));
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
                    unlinked: vec![(id("cobas-e411"), "Port not open", String::new())],
                    ..Facts::default()
                },
                "cobas-e411: Port not open",
                "/machines",
            ),
            (
                Facts {
                    stopped: vec![id("iflash")],
                    unlinked: vec![(id("iflash"), "Not listening", String::new())],
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
            unlinked: vec![(
                id("<m&1>"),
                "Not connected (retrying)",
                "No connection to <m&1> since 10:41. Check the network cable.".into(),
            )],
            uncertain: 1,
            ..Facts::default()
        };
        let html = status(&things(&facts));
        assert!(html.starts_with("<section class=\"verdict verdict--bad\""));
        assert!(html.contains("class=\"verdict__title\">3 things need you</h2>"));
        let items: Vec<&str> = html.split("<li>").skip(1).collect();
        assert_eq!(items.len(), 3, "{html}");
        let item = |title: &str| {
            items
                .iter()
                .find(|item| item.contains(&format!("<p class=\"todo__title\">{title}</p>")))
                .copied()
                .unwrap_or_default()
        };
        assert!(
            item("2 items need review")
                .contains("<a class=\"btn btn--outline\" href=\"/parked\">Needs review</a>")
        );
        let unlinked = item("&#60;m&#38;1&#62;: Not connected (retrying)");
        assert!(unlinked.contains("<a class=\"btn btn--outline\" href=\"/machines\">Machines</a>"));
        assert!(unlinked.contains(concat!(
            "<p class=\"todo__detail\">No connection to &#60;m&#38;1&#62; since 10:41. ",
            "Check the network cable.</p>"
        )));
        assert!(html.contains("href=\"/orders\">Machine orders</a>"));
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
            assert_eq!(thing.fix, Fix::Page(Tab::LabApps), "{}", thing.text);
        }
        let html = status(&list);
        assert_eq!(
            html.matches("href=\"/setup/lab-apps\">Lab apps</a>")
                .count(),
            4,
            "{html}"
        );
        assert!(!html.contains("href=\"/setup\">"));
    }

    // Owner request 2026-11-19 (admin restyle): each line carries a tone from
    // its subject: a problem when data is refused or cannot move, otherwise a
    // warning; the verdict is red when any line is a problem.
    #[test]
    fn each_subject_has_its_tone() {
        let problems = Facts {
            disk_full: true,
            stopped: vec![id("a")],
            unlinked: vec![(id("b"), "Port not open", String::new())],
            blocked: vec![(id("x"), Undeliverable::CredentialsMissing, 1)],
            ..Facts::default()
        };
        let list = things(&problems);
        assert_eq!(list.len(), 4);
        assert!(list.iter().all(|thing| thing.tone() == Tone::Problem));
        assert_eq!(Verdict::of(&list).tone, Tone::Problem);
        let warnings = Facts {
            review: 1,
            uncertain: 1,
            cancelled: 1,
            restore_paused: true,
            paused: vec![(id("demo"), 2)],
            alerts: vec![
                alert(AlertKey::MachineSilent(id("c")), None),
                alert(AlertKey::OldestUndelivered, Some(7200)),
                alert(AlertKey::OrderCacheStale, None),
            ],
            ..Facts::default()
        };
        let list = things(&warnings);
        assert_eq!(list.len(), 8);
        assert!(list.iter().all(|thing| thing.tone() == Tone::Attention));
        let verdict = Verdict::of(&list);
        assert_eq!(verdict.tone, Tone::Attention);
        assert_eq!(
            verdict.todo.first().map(|todo| todo.number.as_str()),
            Some("01")
        );
        assert!(status(&list).starts_with("<section class=\"verdict verdict--warn\""));
    }
}
