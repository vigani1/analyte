//! Decides whether everything is OK: one short line per thing that needs someone, each with the page that fixes it.
//!
//! It only reads what the overview already measured and the alerts the alert
//! rules raised; it adds no thresholds of its own. Lines hold names and counts,
//! never payloads or sample IDs (safety rule 10). Each line also has a kind
//! from a closed list ([`Reason`]), which the public `GET /status` names
//! without any name or count (ADR 0023).
use crate::admin::view::{Tab, Tone, format};
use crate::machine::software_version::UNCHECKED_WARNING;
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
    /// Listening machines that refused a device not on their `allowed_peers`
    /// lately, with the newest refusal in words (ADR 0026).
    pub(super) refused: Vec<(Identifier, String)>,
    /// Listening machines without `allowed_peers`: any device that reaches
    /// their port is accepted as them (ADR 0026).
    pub(super) open: Vec<Identifier>,
    /// Machines that reported unexpected or unknown software.
    pub(super) software: Vec<(Identifier, String)>,
    /// Machines configured with a software version outside manual coverage.
    pub(super) software_unchecked: Vec<Identifier>,
    /// A software version warning failed to save during this run.
    pub(super) software_warning_write_failed: bool,
    /// Lab apps whose sending is paused, with how many items wait for each.
    pub(super) paused: Vec<(Identifier, u64)>,
    /// Lab apps Analyte cannot send to now, why, how many items wait and, for
    /// a kept lab app without keys, who sent them and that they are kept.
    pub(super) blocked: Vec<(Identifier, Undeliverable, u64, Option<String>)>,
    /// Lab apps whose tries keep failing (`analyte_store::FailingDestination`),
    /// how many items wait, and what their node says was observed.
    pub(super) failing: Vec<(Identifier, u64, String)>,
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
/// where each lab app's address and keys are set; failing tries go to the
/// Deliveries page, which lists each delivery and its state.
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

    /// The command that shows or fixes the same thing from the command line,
    /// for `analyte overview` and `analyte alerts list`; none where only the
    /// page does it. Recovery commands still ask for `--actor` and `--reason`.
    pub(super) const fn command(self) -> Option<&'static str> {
        match self {
            Self::Page(Tab::NeedsReview) => Some("analyte review list"),
            Self::Page(Tab::Deliveries) | Self::Section(Section::Sending) => {
                Some("analyte --actor <you> deliveries list --reason \"<why>\"")
            }
            Self::Page(Tab::MachineOrders) => Some("analyte orders list"),
            Self::Page(Tab::CancelledByMachine) => Some("analyte orders cancelled"),
            Self::Page(Tab::DataKeeping) | Self::Section(Section::Disk) => {
                Some("analyte data-keeping")
            }
            Self::Section(Section::Orders) => {
                Some("analyte --actor <you> orders resume-after-restore --reason \"<why>\"")
            }
            Self::Page(Tab::Activity) => Some("analyte activity"),
            Self::Page(Tab::Machines | Tab::Setup) => Some("analyte machines list"),
            Self::Page(Tab::LabApps) => Some("analyte lab-apps list"),
            Self::Page(Tab::Overview | Tab::Settings) => None,
        }
    }
}

/// What a line is about; an alert about something already listed adds no second line.
#[derive(Debug, PartialEq, Eq)]
enum Subject {
    /// Free space at or below the reserve: receiving is refused.
    Disk,
    /// The disk-reserve alert rule: space is running low.
    DiskLow,
    Blocked(Identifier, Undeliverable),
    Failing(Identifier),
    Paused(Identifier),
    Stopped(Identifier),
    Unlinked(Identifier),
    Silent(Identifier),
    Software(Identifier),
    SoftwareUnchecked(Identifier),
    SoftwareWarningWriteFailed,
    Refused(Identifier),
    Open(Identifier),
    Review,
    Uncertain,
    Cancelled,
    RestorePaused,
    Waiting,
    OrderList,
}

impl Subject {
    /// How serious a line about this subject is: a problem stops data now;
    /// a warning needs a look but nothing is refused. Failing tries are a
    /// problem from the first one, like keys missing: results are not reaching
    /// the lab app now. No age threshold is added, as the overview adds none of
    /// its own; the site's optional `oldest_undelivered` alert adds its line.
    const fn tone(&self) -> Tone {
        match self {
            Self::Disk
            | Self::DiskLow
            | Self::Blocked(..)
            | Self::Failing(_)
            | Self::Stopped(_)
            | Self::Unlinked(_) => Tone::Problem,
            Self::Paused(_)
            | Self::Silent(_)
            | Self::Software(_)
            | Self::SoftwareUnchecked(_)
            | Self::SoftwareWarningWriteFailed
            | Self::Refused(_)
            | Self::Open(_)
            | Self::Review
            | Self::Uncertain
            | Self::Cancelled
            | Self::RestorePaused
            | Self::Waiting
            | Self::OrderList => Tone::Attention,
        }
    }

    /// The kind `GET /status` names for a line about this subject.
    const fn reason(&self) -> Reason {
        match self {
            Self::Disk => Reason::DiskFull,
            Self::DiskLow => Reason::DiskLow,
            Self::Blocked(..) => Reason::LabAppBlocked,
            Self::Failing(_) => Reason::SendingFailing,
            Self::Stopped(_) => Reason::MachineStopped,
            Self::Unlinked(_) => Reason::MachineLinkOff,
            Self::Paused(_) => Reason::SendingPaused,
            Self::Silent(_) => Reason::MachineSilent,
            Self::Software(_) => Reason::MachineSoftwareVersionChanged,
            Self::SoftwareUnchecked(_) => Reason::MachineSoftwareVersionUnchecked,
            Self::SoftwareWarningWriteFailed => Reason::SoftwareWarningWriteFailed,
            Self::Refused(_) => Reason::MachineRefusedConnection,
            Self::Open(_) => Reason::MachineOpenToAnyDevice,
            Self::Waiting => Reason::ResultWaiting,
            Self::Review => Reason::NeedsReview,
            Self::Uncertain => Reason::OrdersUncertain,
            Self::Cancelled => Reason::OrdersCancelled,
            Self::RestorePaused => Reason::OrdersPausedAfterRestore,
            Self::OrderList => Reason::OrderListStale,
        }
    }

    /// The machine or lab app the line is about, when it is about one.
    const fn about(&self) -> Option<&Identifier> {
        match self {
            Self::Blocked(name, _)
            | Self::Failing(name)
            | Self::Paused(name)
            | Self::Stopped(name)
            | Self::Unlinked(name)
            | Self::Silent(name)
            | Self::Software(name)
            | Self::SoftwareUnchecked(name)
            | Self::Refused(name)
            | Self::Open(name) => Some(name),
            Self::Disk
            | Self::DiskLow
            | Self::Review
            | Self::Uncertain
            | Self::Cancelled
            | Self::RestorePaused
            | Self::Waiting
            | Self::OrderList
            | Self::SoftwareWarningWriteFailed => None,
        }
    }

    /// Whether a line about this subject already says what an alert about
    /// `other` would: the same subject, or a full disk for "space is low".
    fn covers(&self, other: &Self) -> bool {
        self == other || matches!((self, other), (Self::Disk, Self::DiskLow))
    }
}

/// What kind of thing needs someone, as the public `GET /status` names it:
/// one word per kind from this closed list, never a name, count or value
/// (safety rule 10, ADR 0023). Declared most serious first, every kind that
/// stops data now before every kind that only needs a look; the answer lists
/// the kinds in this order, each once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::admin) enum Reason {
    /// Free space is at or below the reserve: new data is refused.
    DiskFull,
    /// The disk-reserve alert: space is running low.
    DiskLow,
    /// A lab app cannot be sent to: keys missing or not set up any more.
    LabAppBlocked,
    /// Tries to send to a lab app keep failing.
    SendingFailing,
    /// Analyte stopped working with a machine.
    MachineStopped,
    /// A machine's link is off.
    MachineLinkOff,
    /// Sending to a lab app is paused.
    SendingPaused,
    /// A machine has sent nothing for longer than its alert rule allows.
    MachineSilent,
    /// A listening machine refused a device not on its `allowed_peers` lately
    /// (ADR 0026): the machine's address changed, or another device tried.
    MachineRefusedConnection,
    /// A result has waited longer than its alert rule allows to be sent.
    ResultWaiting,
    /// Items on Needs review.
    NeedsReview,
    /// Orders whose outcome on the machine is not known.
    OrdersUncertain,
    /// Orders a machine cancelled on its own.
    OrdersCancelled,
    /// Order sending is paused until IT reconciles orders after a restore.
    OrdersPausedAfterRestore,
    /// The order list from the lab app is out of date.
    OrderListStale,
    /// A listening machine accepts connections from any device (ADR 0026).
    MachineOpenToAnyDevice,
    /// The machine reports unexpected or unknown software.
    MachineSoftwareVersionChanged,
    /// The selected software version has no manual coverage.
    MachineSoftwareVersionUnchecked,
    /// An unexpected or unknown software warning could not be saved.
    SoftwareWarningWriteFailed,
}

impl Reason {
    /// The word `GET /status` answers with.
    pub(in crate::admin) const fn word(self) -> &'static str {
        match self {
            Self::DiskFull => "disk_full",
            Self::DiskLow => "disk_low",
            Self::LabAppBlocked => "lab_app_blocked",
            Self::SendingFailing => "sending_failing",
            Self::MachineStopped => "machine_stopped",
            Self::MachineLinkOff => "machine_link_off",
            Self::SendingPaused => "sending_paused",
            Self::MachineSilent => "machine_silent",
            Self::MachineRefusedConnection => "machine_refused_connection",
            Self::ResultWaiting => "result_waiting",
            Self::NeedsReview => "needs_review",
            Self::OrdersUncertain => "orders_uncertain",
            Self::OrdersCancelled => "orders_cancelled",
            Self::OrdersPausedAfterRestore => "orders_paused_after_restore",
            Self::OrderListStale => "order_list_stale",
            Self::MachineOpenToAnyDevice => "machine_open_to_any_device",
            Self::MachineSoftwareVersionChanged => "machine_software_version_changed",
            Self::MachineSoftwareVersionUnchecked => "machine_software_version_unchecked",
            Self::SoftwareWarningWriteFailed => "software_warning_write_failed",
        }
    }
}

/// One thing that needs someone: a short sentence, an optional detail with
/// what to check, and where to fix it.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::admin) struct Thing {
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

    /// The kind `GET /status` names for this line.
    pub(super) const fn reason(&self) -> Reason {
        self.subject.reason()
    }

    /// The machine or lab app this line is about, when it is about one.
    pub(super) const fn about(&self) -> Option<&Identifier> {
        self.subject.about()
    }
}

/// What `GET /status` says: the overall state and the kinds of thing that
/// need someone, both from the same list of things the verdict shows, so the
/// tray, a notification and the Overview never disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct Summary {
    /// The verdict's state.
    pub(in crate::admin) overall: Overall,
    /// Each kind on the list once, most serious first; empty when all is well.
    pub(in crate::admin) reasons: Vec<Reason>,
}

impl Summary {
    /// The summary of `things`.
    pub(super) fn of(things: &[Thing]) -> Self {
        let reasons: std::collections::BTreeSet<Reason> =
            things.iter().map(|thing| thing.subject.reason()).collect();
        Self {
            overall: Overall::of(things),
            reasons: reasons.into_iter().collect(),
        }
    }
}

/// The overall state the Overview's verdict shows, in one word: the one place
/// that turns the list of things into a state, for the verdict and for the
/// public `GET /status` the desktop app reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::admin) enum Overall {
    /// Nothing needs anyone ("All good").
    Good,
    /// Something needs a look, but nothing stops data now.
    Attention,
    /// At least one thing stops data now (a [`Tone::Problem`] line).
    Problem,
}

impl Overall {
    /// The state of `things`: a problem if any line is one, otherwise attention
    /// if there is any line at all.
    pub(super) fn of(things: &[Thing]) -> Self {
        if things.is_empty() {
            Self::Good
        } else if things.iter().any(|thing| thing.tone() == Tone::Problem) {
            Self::Problem
        } else {
            Self::Attention
        }
    }

    /// The verdict's tone for this state.
    pub(super) const fn tone(self) -> Tone {
        match self {
            Self::Good => Tone::Good,
            Self::Attention => Tone::Attention,
            Self::Problem => Tone::Problem,
        }
    }

    /// The word `GET /status` answers with.
    pub(in crate::admin) const fn word(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Attention => "attention",
            Self::Problem => "problem",
        }
    }
}

/// Every thing that needs someone now, in Overview display order.
/// Measured lines come first, then uncovered alerts in the order they were read.
/// Software alerts are listed in the alerts card; the verdict uses the machine's
/// current software warning from the measured facts.
pub(super) fn things(facts: &Facts) -> Vec<Thing> {
    let mut list = measured_things(facts);
    for alert in &facts.alerts {
        if matches!(alert.key, AlertKey::MachineSoftwareVersionChanged(_)) {
            continue;
        }
        let thing = from_alert(alert);
        if !list
            .iter()
            .any(|listed| listed.subject.covers(&thing.subject))
        {
            list.push(thing);
        }
    }
    list
}

/// The lines based on current observations, in their fixed display order.
fn measured_things(facts: &Facts) -> Vec<Thing> {
    let mut list = Vec::new();
    if facts.disk_full {
        list.push(Thing::new(
            Subject::Disk,
            "Disk almost full: Analyte refuses new data from machines".into(),
            Fix::Section(Section::Disk),
        ));
    }
    for (destination, reason, waiting, kept) in &facts.blocked {
        list.push(Thing {
            detail: kept.clone(),
            ..blocked(destination, *reason, Some(*waiting))
        });
    }
    for (destination, waiting, observed) in &facts.failing {
        list.push(Thing {
            detail: Some(format!("{observed}. {}", format::FAILING_ADVICE)),
            ..Thing::new(
                Subject::Failing(destination.clone()),
                format!(
                    "Sending to lab app {} is failing: {waiting} waiting",
                    destination.as_str()
                ),
                Fix::Page(Tab::Deliveries),
            )
        });
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
    list.extend(
        facts
            .refused
            .iter()
            .map(|(machine, words)| refused(machine, words)),
    );
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
    list.extend(facts.open.iter().map(open));
    list.extend(software_attention(facts));
    list
}

fn software_attention(facts: &Facts) -> impl Iterator<Item = Thing> + '_ {
    facts
        .software
        .iter()
        .map(|(machine, text)| {
            Thing::new(
                Subject::Software(machine.clone()),
                text.clone(),
                Fix::Page(Tab::Machines),
            )
        })
        .chain(facts.software_unchecked.iter().map(|machine| {
            Thing::new(
                Subject::SoftwareUnchecked(machine.clone()),
                format!("{UNCHECKED_WARNING} for {}", machine.as_str()),
                Fix::Page(Tab::Machines),
            )
        }))
        .chain(facts.software_warning_write_failed.then(|| {
            Thing::new(
                Subject::SoftwareWarningWriteFailed,
                "Analyte could not save a machine software version warning".into(),
                Fix::Page(Tab::Machines),
            )
        }))
}

/// The line for one active alert, in the same words as the direct measurement it matches.
pub(in crate::admin) fn from_alert(alert: &AlertState) -> Thing {
    let value = alert.value;
    match &alert.key {
        AlertKey::DiskReserve => Thing::new(
            Subject::DiskLow,
            value.map_or_else(
                || "Disk space is low".into(),
                |free| format!("Disk space is low: {} free", format::bytes(free)),
            ),
            Fix::Section(Section::Disk),
        ),
        AlertKey::UndeliverableDestination(destination, reason) => {
            blocked(destination, *reason, value)
        }
        AlertKey::MachineSoftwareVersionChanged(machine) => Thing::new(
            Subject::Software(machine.clone()),
            format!(
                "Check the software version reported by {}.",
                machine.as_str()
            ),
            Fix::Page(Tab::Machines),
        ),
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

/// The line for a machine that refused a device lately, `words` saying which and when (ADR 0026).
fn refused(machine: &Identifier, words: &str) -> Thing {
    Thing {
        detail: Some(format::REFUSED_ADVICE.into()),
        ..Thing::new(
            Subject::Refused(machine.clone()),
            format!("{}: {words}", machine.as_str()),
            Fix::Page(Tab::Machines),
        )
    }
}

/// The line for a listening machine that accepts any device (ADR 0026).
fn open(machine: &Identifier) -> Thing {
    Thing {
        detail: Some(format::OPEN_TO_ANY_ADVICE.into()),
        ..Thing::new(
            Subject::Open(machine.clone()),
            format!("{}: {}", machine.as_str(), format::OPEN_TO_ANY),
            Fix::Page(Tab::Machines),
        )
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
        format::RESTORE_PAUSED.into(),
        Fix::Section(Section::Orders),
    )
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

    // ADR 0019: the tray's word is the verdict's own state: good with nothing
    // listed, attention for a look, problem once any line stops data now.
    #[test]
    fn overall_is_the_verdict_state_in_one_word() {
        let good = things(&Facts::default());
        assert_eq!(Overall::of(&good), Overall::Good);
        assert_eq!(Verdict::of(&good).tone, Tone::Good);
        let review = things(&Facts {
            review: 2,
            ..Facts::default()
        });
        assert_eq!(Overall::of(&review), Overall::Attention);
        assert_eq!(Verdict::of(&review).tone, Tone::Attention);
        let full = things(&Facts {
            review: 2,
            disk_full: true,
            ..Facts::default()
        });
        assert_eq!(Overall::of(&full), Overall::Problem);
        assert_eq!(Verdict::of(&full).tone, Tone::Problem);
        let words = [Overall::Good, Overall::Attention, Overall::Problem].map(Overall::word);
        assert_eq!(words, ["good", "attention", "problem"]);
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
                    blocked: vec![(id("x"), Undeliverable::CredentialsMissing, 3, None)],
                    ..Facts::default()
                },
                "Cannot send to lab app x: keys missing, 3 waiting",
                "/setup/lab-apps",
            ),
            (
                Facts {
                    blocked: vec![(id("old"), Undeliverable::NotConfigured, 1, None)],
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
            blocked: vec![(id("demo"), Undeliverable::CredentialsMissing, 4, None)],
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

    // Overview follows the measured issues first, then the remaining alerts.
    #[test]
    fn measured_lines_keep_their_order_and_take_precedence_over_alerts() {
        let machine = id("analyzer");
        let facts = Facts {
            paused: vec![(id("demo"), 2)],
            stopped: vec![machine.clone()],
            unlinked: vec![(machine.clone(), "Port not open", "Check the cable.".into())],
            software: vec![(
                machine.clone(),
                "Check the version in machine settings.".into(),
            )],
            alerts: vec![
                alert(AlertKey::MachineSoftwareVersionChanged(machine), None),
                alert(
                    AlertKey::MachineSoftwareVersionChanged(id("old-analyzer")),
                    None,
                ),
                alert(AlertKey::OrderCacheStale, None),
                alert(AlertKey::DiskReserve, Some(10)),
            ],
            ..Facts::default()
        };
        let list = things(&facts);
        assert_eq!(
            list.iter().map(Thing::reason).collect::<Vec<_>>(),
            [
                Reason::SendingPaused,
                Reason::MachineStopped,
                Reason::MachineSoftwareVersionChanged,
                Reason::OrderListStale,
                Reason::DiskLow,
            ]
        );
        assert!(list.iter().any(|item| {
            item.reason() == Reason::MachineSoftwareVersionChanged
                && item.text == "Check the version in machine settings."
                && item.fix.href() == "/machines"
        }));
        assert_eq!(Overall::of(&list), Overall::Problem);
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

    // Owner request 2026-11-19 (screenshot review, PR #273): the red notice
    // under the signal path for a kept lab app without keys is gone, so its
    // to-do line keeps what only the notice said: who sent the waiting items,
    // that they are kept and never deleted, and how many need someone.
    #[test]
    fn keys_missing_line_keeps_who_sent_what_waits() {
        let kept = "Removed machine m-3: 4 results or order messages are kept and never deleted.";
        let facts = Facts {
            blocked: vec![(
                id("demo"),
                Undeliverable::CredentialsMissing,
                4,
                Some(kept.into()),
            )],
            ..Facts::default()
        };
        let list = things(&facts);
        let lines: Vec<_> = list
            .iter()
            .map(|line| (line.text.as_str(), line.detail.as_deref(), line.tone()))
            .collect();
        assert_eq!(
            lines,
            [(
                "Cannot send to lab app demo: keys missing, 4 waiting",
                Some(kept),
                Tone::Problem
            )]
        );
    }

    // Owner request 2026-11-19 (after #260): every line about a lab app (keys
    // missing, sending paused, not set up any more) links to the Lab apps page,
    // where its address and keys are set, under its own name.
    #[test]
    fn lab_app_lines_link_to_the_lab_app_page() {
        let facts = Facts {
            paused: vec![(id("demo"), 4)],
            blocked: vec![
                (id("x"), Undeliverable::CredentialsMissing, 3, None),
                (id("old"), Undeliverable::NotConfigured, 1, None),
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

    // Owner request 2026-11-19 (overview: delivery failing): failing tries are a
    // red line that says what was observed, what to check, and links to the
    // Deliveries page.
    #[test]
    fn failing_delivery_is_a_problem_line_with_what_to_check() {
        let facts = Facts {
            failing: vec![(
                id("main-lab"),
                2,
                "Last try 21:59 failed · 74 failed tries since 21:21 · next try 22:00".into(),
            )],
            ..Facts::default()
        };
        assert_eq!(
            only(&facts),
            (
                "Sending to lab app main-lab is failing: 2 waiting".to_owned(),
                "/deliveries"
            )
        );
        let list = things(&facts);
        let thing = list.first().expect("one line");
        assert_eq!(thing.tone(), Tone::Problem);
        assert_eq!(
            thing.detail.as_deref(),
            Some(concat!(
                "Last try 21:59 failed · 74 failed tries since 21:21 · next try 22:00. ",
                "Check that the lab app is running and that this PC can reach it at its address."
            ))
        );
        let html = status(&list);
        assert!(html.starts_with("<section class=\"verdict verdict--bad\""));
        assert!(html.contains("class=\"verdict__title\">1 thing needs you</h2>"));
        assert!(
            html.contains("href=\"/deliveries\">Deliveries</a>"),
            "{html}"
        );
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
            blocked: vec![(id("x"), Undeliverable::CredentialsMissing, 1, None)],
            failing: vec![(id("y"), 1, "Last try 21:59 failed".into())],
            ..Facts::default()
        };
        let list = things(&problems);
        assert_eq!(list.len(), 5);
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

    // ADR 0023: every kind of line has one word from a closed list, and the
    // list puts every kind that stops data now before every kind that only
    // needs a look, so the first word is the most serious.
    #[test]
    fn each_subject_has_one_reason_word_most_serious_first() {
        let subjects = [
            (Subject::Disk, "disk_full"),
            (Subject::DiskLow, "disk_low"),
            (
                Subject::Blocked(id("x"), Undeliverable::NotConfigured),
                "lab_app_blocked",
            ),
            (Subject::Failing(id("x")), "sending_failing"),
            (Subject::Stopped(id("m")), "machine_stopped"),
            (Subject::Unlinked(id("m")), "machine_link_off"),
            (Subject::Paused(id("x")), "sending_paused"),
            (Subject::Silent(id("m")), "machine_silent"),
            (Subject::Refused(id("m")), "machine_refused_connection"),
            (Subject::Waiting, "result_waiting"),
            (Subject::Review, "needs_review"),
            (Subject::Uncertain, "orders_uncertain"),
            (Subject::Cancelled, "orders_cancelled"),
            (Subject::RestorePaused, "orders_paused_after_restore"),
            (Subject::OrderList, "order_list_stale"),
            (Subject::Open(id("m")), "machine_open_to_any_device"),
            (
                Subject::Software(id("m")),
                "machine_software_version_changed",
            ),
            (
                Subject::SoftwareUnchecked(id("m")),
                "machine_software_version_unchecked",
            ),
            (
                Subject::SoftwareWarningWriteFailed,
                "software_warning_write_failed",
            ),
        ];
        let mut reasons: Vec<Reason> = subjects
            .iter()
            .map(|(subject, word)| {
                assert_eq!(subject.reason().word(), *word, "{subject:?}");
                subject.reason()
            })
            .collect();
        let declared = reasons.clone();
        reasons.sort();
        assert_eq!(reasons, declared, "the list above is in declared order");
        let tones: Vec<Tone> = subjects.iter().map(|(subject, _)| subject.tone()).collect();
        let first_attention = tones
            .iter()
            .position(|tone| *tone == Tone::Attention)
            .expect("some kinds only need a look");
        assert!(
            tones
                .iter()
                .take(first_attention)
                .all(|tone| *tone == Tone::Problem)
        );
        assert!(
            tones
                .iter()
                .skip(first_attention)
                .all(|tone| *tone == Tone::Attention)
        );
        let shared: Vec<&str> = include_str!("../../../../testing/fixtures/status-reasons.txt")
            .lines()
            .collect();
        assert_eq!(
            subjects.iter().map(|(_, word)| *word).collect::<Vec<_>>(),
            shared,
            "the app reads the same words, in the same order"
        );
        for word in subjects.map(|(_, word)| word) {
            assert!(
                word.bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
                "{word}"
            );
        }
    }

    // ADR 0023: the status kinds come from the same lines as the verdict,
    // each kind once and most serious first, whatever order the lines are in.
    #[test]
    fn the_summary_lists_each_kind_once_most_serious_first() {
        let facts = Facts {
            review: 2,
            unlinked: vec![
                (id("a"), "Port not open", String::new()),
                (id("b"), "Port not open", String::new()),
            ],
            paused: vec![(id("demo"), 2)],
            alerts: vec![alert(AlertKey::DiskReserve, Some(10))],
            ..Facts::default()
        };
        let summary = Summary::of(&things(&facts));
        assert_eq!(summary.overall, Overall::Problem);
        let words: Vec<&str> = summary.reasons.iter().map(|reason| reason.word()).collect();
        assert_eq!(
            words,
            [
                "disk_low",
                "machine_link_off",
                "sending_paused",
                "needs_review"
            ]
        );
        let good = Summary::of(&things(&Facts::default()));
        assert_eq!(good.overall, Overall::Good);
        assert!(good.reasons.is_empty());
    }

    // ADR 0023: a full disk says "full" even when the disk alert is also
    // raised; the alert alone says "low", and adds no second disk line.
    #[test]
    fn a_full_disk_is_full_and_the_alert_alone_is_low() {
        let full = Facts {
            disk_full: true,
            alerts: vec![alert(AlertKey::DiskReserve, Some(10))],
            ..Facts::default()
        };
        let list = things(&full);
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(Summary::of(&list).reasons, [Reason::DiskFull]);
        let low = Facts {
            alerts: vec![alert(AlertKey::DiskReserve, Some(10))],
            ..Facts::default()
        };
        assert_eq!(Summary::of(&things(&low)).reasons, [Reason::DiskLow]);
    }
}
