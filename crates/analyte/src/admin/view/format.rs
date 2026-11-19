//! Turns technical values into words a lab manager reads at a glance; pages keep the exact value beside them.
//!
//! Sizes use binary units (1 GB = 1024³ bytes), the way Windows Explorer shows a
//! disk. Times show in the PC's own time zone; the exact UTC time and Unix
//! seconds stay in the hover text. Connection sentences say only what Analyte
//! observed (a port open or not, a listener with or without a peer), never what
//! it cannot see, such as a cable or a machine being switched on.
use crate::admin::markup::Markup;
use crate::admin::server::Connection;
use crate::machine::link::status::Light;
use crate::machine::{astm, autolyser, hl7, interpretation, law, poct, vidas};
use crate::operator::recovery;
use analyte_config::ValueKind;
use analyte_contract::ReplyProblem;
use analyte_mapping::{Finding, Position, Problem, ReasonPiece, Unidentified};
use analyte_model::OrderState;
use analyte_store::{
    CANCEL_CONFLICT_REASON, DeliveryState, FailingDestination, NO_RESULTS_REASON, OrderRefusal,
    ParkedKind, RevisionOutcome, reconciled_outcome,
};
use jiff::{Timestamp, tz::TimeZone};

/// A size such as "477 GB" or "1.5 MB"; under 1 KB it counts bytes.
pub(in crate::admin) fn bytes(count: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if count < 1024 {
        return amount(count, "byte", "bytes");
    }
    let value = u128::from(count);
    let mut unit: u128 = 1024;
    let mut name = UNITS[0];
    // Move up while the value, rounded, would read 1024 or more of this unit.
    for next in UNITS.iter().skip(1) {
        if rounded(value, unit) < 1024 {
            break;
        }
        unit = unit.saturating_mul(1024);
        name = next;
    }
    let tenths = rounded(value.saturating_mul(10), unit);
    if tenths < 100 && !tenths.is_multiple_of(10) {
        format!("{}.{} {name}", tenths / 10, tenths % 10)
    } else {
        format!("{} {name}", rounded(value, unit))
    }
}

/// `value / unit` rounded half up; `unit` is a power of 1024, never zero.
fn rounded(value: u128, unit: u128) -> u128 {
    value
        .saturating_add(unit / 2)
        .checked_div(unit)
        .unwrap_or_default()
}

/// A length of time in its largest whole unit: "45 s", "3 min", "2 h", "1 day".
pub(in crate::admin) fn duration(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3_600 => format!("{} min", seconds / 60),
        3_600..86_400 => format!("{} h", seconds / 3_600),
        _ => amount(seconds / 86_400, "day", "days"),
    }
}

/// How long ago `then` was, such as "3 min ago"; under a minute it says
/// "less than a minute ago", since seconds are not worth reading.
pub(in crate::admin) fn ago(then: Timestamp, now: Timestamp) -> String {
    match u64::try_from(now.as_second().saturating_sub(then.as_second())) {
        Ok(0..60) => "less than a minute ago".into(),
        Ok(seconds) => format!("{} ago", duration(seconds)),
        Err(_) => "unknown (clock moved back)".into(),
    }
}

/// A date and time in the PC's time zone, such as "2026-11-19 14:32".
pub(in crate::admin) fn time(at: Timestamp) -> String {
    time_in(at, &TimeZone::system())
}

fn time_in(at: Timestamp, zone: &TimeZone) -> String {
    at.to_zoned(zone.clone())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

/// What someone does while order sending is paused after a restore.
pub(in crate::admin) const RESTORE_ADVICE: &str =
    "Ask IT to reconcile orders with the command line; until then no order is sent to a machine.";

/// What a restyled page says when its reads took longer than
/// [`super::READ_BUDGET`].
pub(in crate::admin) const SLOW_READ: &str = "Analyte did not finish reading within 1.5 seconds.";

/// What "Last data received" says before a machine has sent anything.
pub(in crate::admin) const NOTHING_YET: &str = "Nothing yet";

/// A time of day in the PC's time zone: "10:41" today, "18 Nov 23:54"
/// earlier this year, "2025-11-18 23:54" before that.
pub(in crate::admin) fn clock(at: Timestamp, now: Timestamp) -> String {
    clock_in(at, now, &TimeZone::system())
}

fn clock_in(at: Timestamp, now: Timestamp, zone: &TimeZone) -> String {
    let at = at.to_zoned(zone.clone());
    let now = now.to_zoned(zone.clone());
    let pattern = if at.date() == now.date() {
        "%H:%M"
    } else if at.year() == now.year() {
        "%-d %b %H:%M"
    } else {
        "%Y-%m-%d %H:%M"
    };
    at.strftime(pattern).to_string()
}

/// What the sidebar says about when the page was made: "Page loaded 10:45".
/// Pages are not live yet, so this is the minute it was loaded, not a clock.
pub(in crate::admin) fn page_loaded(now: Timestamp) -> String {
    page_loaded_in(now, &TimeZone::system())
}

fn page_loaded_in(now: Timestamp, zone: &TimeZone) -> String {
    format!("Page loaded {}", clock_in(now, now, zone))
}

/// The date line above a page title: "Thursday 19 November · 10:45".
pub(in crate::admin) fn today(now: Timestamp) -> String {
    today_in(now, &TimeZone::system())
}

fn today_in(now: Timestamp, zone: &TimeZone) -> String {
    now.to_zoned(zone.clone())
        .strftime("%A %-d %B · %H:%M")
        .to_string()
}

/// Today, as [`clock`] reads it: the PC's local day that `now` falls in, as
/// Unix seconds `from..until`. None when the day cannot be worked out.
pub(in crate::admin) fn today_span(now: Timestamp) -> Option<(i64, i64)> {
    day_in(now, &TimeZone::system())
}

fn day_in(now: Timestamp, zone: &TimeZone) -> Option<(i64, i64)> {
    let start = now.to_zoned(zone.clone()).start_of_day().ok()?;
    let end = start.tomorrow().ok()?.start_of_day().ok()?;
    Some((start.timestamp().as_second(), end.timestamp().as_second()))
}

/// The exact value kept on hover for every shown time: UTC and Unix seconds.
fn exact_time(at: Timestamp) -> String {
    format!("{at} (Unix {})", at.as_second())
}

/// A stored Unix time outside the calendar, shown as it is.
fn not_a_date(seconds: i64) -> String {
    format!("Unix time {seconds} (not a valid date)")
}

/// A time ready for a `<time>` element: its machine-readable value, what
/// people read, and the exact value on hover.
pub(in crate::admin) struct Moment {
    /// RFC 3339 in UTC; empty when the stored value is not a date.
    pub(in crate::admin) datetime: String,
    pub(in crate::admin) shown: String,
    pub(in crate::admin) exact: String,
}

/// Words with times in them, as one line shows them: each time is a
/// [`Moment`], so it reads in mono with its exact value on hover.
pub(in crate::admin) struct Line(Vec<Part>);

/// One piece of a [`Line`].
pub(in crate::admin) enum Part {
    Text(String),
    Time(Moment),
}

impl Line {
    pub(in crate::admin) fn text(text: impl Into<String>) -> Self {
        Self(vec![Part::Text(text.into())])
    }

    fn time(mut self, moment: Moment) -> Self {
        self.0.push(Part::Time(moment));
        self
    }

    fn text_part(mut self, text: &str) -> Self {
        self.0.push(Part::Text(text.into()));
        self
    }

    /// This line, a " · " separator, then `next`.
    pub(in crate::admin) fn then(mut self, next: Self) -> Self {
        self.0.push(Part::Text(" · ".into()));
        self.0.extend(next.0);
        self
    }

    pub(in crate::admin) fn parts(&self) -> &[Part] {
        &self.0
    }

    /// The line as people read it, each time as shown: for text that cannot
    /// hold a `<time>`, such as a to-do line's detail.
    pub(in crate::admin) fn plain(&self) -> String {
        self.0
            .iter()
            .map(|part| match part {
                Part::Text(text) => text.as_str(),
                Part::Time(moment) => moment.shown.as_str(),
            })
            .collect()
    }
}

/// [`clock`] as a [`Moment`].
pub(in crate::admin) fn moment(at: Timestamp, now: Timestamp) -> Moment {
    Moment {
        datetime: at.to_string(),
        shown: clock(at, now),
        exact: exact_time(at),
    }
}

/// [`moment`] for Unix seconds from the store; a value outside the calendar
/// shows as it is.
pub(in crate::admin) fn moment_unix(seconds: i64, now: Timestamp) -> Moment {
    Timestamp::from_second(seconds).map_or_else(
        |_| Moment {
            datetime: String::new(),
            shown: not_a_date(seconds),
            exact: String::new(),
        },
        |at| moment(at, now),
    )
}

/// "5 machines · 2 lab apps", beside a page title.
pub(in crate::admin) fn machines_and_lab_apps(machines: usize, lab_apps: usize) -> String {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    format!(
        "{} · {}",
        amount(count(machines), "machine", "machines"),
        amount(count(lab_apps), "lab app", "lab apps")
    )
}

/// How a machine is attached, in one or two words.
pub(in crate::admin) const fn connection_kind(connection: &Connection) -> &'static str {
    match connection {
        Connection::Serial(_) => "Serial",
        Connection::UsbAdapter(_) => "USB adapter",
        Connection::Listening(_) | Connection::Dialing => "Network",
        Connection::Folder(_) => "Watched folder",
    }
}

/// What Analyte observes about a machine's link, with the time it last changed:
/// "Port COM3 not open since 10:41", "Listening on port 5000, nothing connected
/// since 10:41", "No connection to cobas-e411 since 10:41", "Watching results".
pub(in crate::admin) fn link_sentence(
    connection: &Connection,
    light: Light,
    since: &str,
    machine: &str,
) -> String {
    match (connection, light) {
        (Connection::Serial(port), Light::Live) => format!("Port {port} open since {since}"),
        (Connection::Serial(port), Light::Waiting | Light::Off) => {
            format!("Port {port} not open since {since}")
        }
        (Connection::UsbAdapter(serial), Light::Live) => {
            format!("Port of USB adapter {serial} open since {since}")
        }
        (Connection::UsbAdapter(serial), Light::Waiting | Light::Off) => {
            format!("Port of USB adapter {serial} not open since {since}")
        }
        (Connection::Listening(port), Light::Live) => {
            format!("Machine connected{} since {since}", on_port(*port))
        }
        (Connection::Listening(port), Light::Waiting) => {
            format!(
                "Listening{}, nothing connected since {since}",
                on_port(*port)
            )
        }
        (Connection::Listening(port), Light::Off) => {
            format!("Not listening{} since {since}", on_port(*port))
        }
        (Connection::Dialing, Light::Live) => format!("Connected to {machine} since {since}"),
        (Connection::Dialing, Light::Waiting | Light::Off) => {
            format!("No connection to {machine} since {since}")
        }
        (Connection::Folder(folder), Light::Live) => format!("Watching {folder}"),
        (Connection::Folder(folder), Light::Waiting | Light::Off) => {
            format!("Not watching {folder} since {since}")
        }
    }
}

/// " on port 5000", or nothing when the site file names no plain port.
fn on_port(port: Option<u16>) -> String {
    port.map_or_else(String::new, |port| format!(" on port {port}"))
}

/// What to check when a machine's link is not open: what Analyte cannot see.
/// A serial port that will not open is a problem on this PC (the adapter or
/// cable unplugged from it, a wrong port name, or another program holding the
/// port); the cable to the machine and its host settings matter once the port
/// is open and nothing arrives, which the listen test says.
pub(in crate::admin) fn link_advice(connection: &Connection) -> String {
    match connection {
        Connection::Serial(port) => format!(
            "Check that the USB adapter or cable is plugged into this PC, that {port} is the right port, and that no other program is using it."
        ),
        Connection::UsbAdapter(serial) => format!(
            "Check that the USB adapter with serial {serial} is plugged into this PC and that no other program is using its port."
        ),
        Connection::Listening(_) | Connection::Dialing => {
            "Check the network cable and the machine's host settings.".into()
        }
        Connection::Folder(_) => {
            "Check that the folder exists and that Analyte's service account may read it.".into()
        }
    }
}

/// When a lab app last accepted a delivery. `purged` says data keeping removed
/// delivered items, so an empty answer may hide older deliveries.
pub(in crate::admin) fn last_delivered(
    at: Option<Timestamp>,
    purged: bool,
    now: Timestamp,
) -> Line {
    match at {
        Some(at) => Line::text("Last delivered ").time(moment(at, now)),
        None if purged => Line::text("No delivery in the kept records"),
        None => Line::text("Nothing delivered yet"),
    }
}

/// What waits for a lab app, written to follow [`last_delivered`] after a
/// " · ": "4 items waiting · oldest 10:31", or "nothing waiting". Items are
/// results and order messages alike.
pub(in crate::admin) fn waiting(count: u64, oldest: Option<Timestamp>, now: Timestamp) -> Line {
    if count == 0 {
        return Line::text("nothing waiting");
    }
    let items = amount(count, "item waiting", "items waiting");
    match oldest {
        Some(at) => Line::text(format!("{items} · oldest ")).time(moment(at, now)),
        None => Line::text(format!("{items} · oldest not known")),
    }
}

/// What Analyte observed about a lab app whose tries keep failing (see
/// [`FailingDestination`]): "Last try 21:59 failed · 74 failed
/// tries since 21:21 · next try 22:00". The next try reads "due" once its time
/// has come, and "under way" while every waiting item is being tried. No cause
/// is named: the store keeps none.
pub(in crate::admin) fn failing(failing: &FailingDestination, now: Timestamp) -> Line {
    let tried = Line::text("Last try ")
        .time(moment_unix(failing.last, now))
        .text_part(" failed");
    let count = Line::text(format!(
        "{} since ",
        amount(failing.tries, "failed try", "failed tries")
    ))
    .time(moment_unix(failing.since, now));
    let next = match failing.next {
        Some(at) if at > now.as_second() => Line::text("next try ").time(moment_unix(at, now)),
        Some(_) => Line::text("next try due"),
        None => Line::text("a try is under way"),
    };
    tried.then(count).then(next)
}

/// What to check when a lab app's tries keep failing.
pub(in crate::admin) const FAILING_ADVICE: &str =
    "Check that the lab app is running and that this PC can reach it at its address.";

/// Which set-up machines send to a lab app: "From cobas-e411, iflash".
pub(in crate::admin) fn senders(machines: &[&str]) -> String {
    if machines.is_empty() {
        "No set-up machine sends here".into()
    } else {
        format!("From {}", machines.join(", "))
    }
}

/// Which of a lab app's keys the key source has no value for:
/// "Keys missing: API key and signing key." With none missing now, the keys
/// were entered after Analyte started.
pub(in crate::admin) fn missing_keys(missing: &[&str]) -> String {
    match missing {
        [] => "Its keys are set now; Analyte uses them after it restarts.".into(),
        [one] => format!("Key missing: {one}."),
        [first @ .., last] => format!("Keys missing: {} and {last}.", first.join(", ")),
    }
}

/// What to do for a lab app Analyte cannot deliver to.
pub(in crate::admin) const KEYS_ADVICE: &str = concat!(
    "add the missing keys on the Lab apps page or in the service environment. ",
    "Waiting items go out after Analyte restarts with the keys."
);

/// "Yes" or "No".
pub(in crate::admin) const fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

/// A count with the right word: "0 results", "1 result", "2 results".
pub(in crate::admin) fn amount(count: impl Into<u64>, one: &str, many: &str) -> String {
    let count = count.into();
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A time for people: the local date and time and how long ago, with the exact
/// UTC time and Unix seconds kept in the markup.
pub(in crate::admin) fn when(at: Timestamp, now: Timestamp) -> Markup {
    Markup::new()
        .markup("<time datetime=\"")
        .text(at)
        .markup("\" title=\"")
        .text(exact_time(at))
        .markup("\">")
        .text(time(at))
        .markup("</time> <span class=\"muted\">")
        .text(ago(at, now))
        .markup("</span>")
}

/// [`when`] for Unix seconds from the store; a value outside the calendar shows as is.
pub(in crate::admin) fn when_unix(seconds: i64, now: Timestamp) -> Markup {
    Timestamp::from_second(seconds).map_or_else(
        |_| Markup::new().text(not_a_date(seconds)),
        |at| when(at, now),
    )
}

/// A stored reason for people: each mapping position reads "record 3, field 3",
/// or "record 3" when field zero means the whole record, and the exact stored
/// text stays on hover. A reason without a position shows exactly as stored.
pub(in crate::admin) fn reason(stored: &str) -> Markup {
    reason_words(stored).map_or_else(
        || Markup::new().text(stored),
        |shown| super::exact(&shown, stored),
    )
}

/// [`reason`] as plain words; none when the stored reason holds no mapping
/// position, so it already reads as stored.
pub(in crate::admin) fn reason_words(stored: &str) -> Option<String> {
    let pieces = Position::split_stored(stored);
    if !pieces
        .iter()
        .any(|piece| matches!(piece, ReasonPiece::Position(_)))
    {
        return None;
    }
    let mut shown = String::new();
    for piece in pieces {
        match piece {
            ReasonPiece::Text(text) => shown.push_str(text),
            ReasonPiece::Position(Position { record, field: 0 }) => {
                shown.push_str(&format!("record {record}"));
            }
            ReasonPiece::Position(Position { record, field }) => {
                shown.push_str(&format!("record {record}, field {field}"));
            }
        }
    }
    Some(shown)
}

/// Who an audit entry names, in words: the admin pages' browser sessions read
/// "admin session" (the session's ID is for support only), Analyte's own work
/// reads "Analyte", and any other actor, such as one given to the command
/// line, reads as it was recorded. No person's name is ever made up.
pub(in crate::admin) fn actor(recorded: &str) -> String {
    if recorded.starts_with("admin-session-") {
        "admin session".into()
    } else if recorded == "analyte" {
        "Analyte".into()
    } else {
        recorded.to_owned()
    }
}

/// Who recorded an audit entry and why, as one line under it in a history:
/// "admin session · “settled with the lab app”".
pub(in crate::admin) fn recorded(actor: &str, reason: &str) -> String {
    format!("{} · “{reason}”", self::actor(actor))
}

/// What an audited action did to a Needs review item, from the action and
/// targets Analyte recorded with it: "Marked resolved", "Processed again:
/// reading 2 recorded" (and, when the new reading still needed review, "it
/// still needs review"), or "Resolved" for an action recorded elsewhere.
pub(in crate::admin) fn settled(action: &str, targets: &[&str]) -> String {
    match action {
        recovery::REPROCESS_ACTION => {
            let reading = targets
                .iter()
                .find_map(|target| target.strip_prefix("interpretation:"))
                .and_then(|rest| rest.rsplit(':').next())
                .map_or_else(String::new, |version| {
                    format!(": reading {version} recorded")
                });
            let still = if targets.contains(&"reprocess:parked") {
                "; it still needs review"
            } else {
                ""
            };
            format!("{}{reading}{still}", settled_action(action))
        }
        _ => settled_action(action).into(),
    }
}

/// [`settled`] in short, without what the new reading decided: "Marked
/// resolved", "Processed again", or "Resolved" for an action recorded elsewhere.
pub(in crate::admin) fn settled_action(action: &str) -> &'static str {
    match action {
        recovery::RESOLVE_ACTION => "Marked resolved",
        recovery::REPROCESS_ACTION => "Processed again",
        _ => "Resolved",
    }
}

/// One resolved item's line on the Needs review list: "Marked resolved by
/// admin session at 09:20: “Re-sent after the lab app restarted.”"
pub(in crate::admin) fn settled_line(
    what: &str,
    actor: &str,
    seconds: i64,
    reason: &str,
    now: Timestamp,
) -> Line {
    Line::text(format!("{what} by {actor} at "))
        .time(moment_unix(seconds, now))
        .text_part(&format!(": “{reason}”"))
}

/// Where a parked item's message came from and when it arrived:
/// "From cobas-e411 · arrived 10:45".
pub(in crate::admin) fn arrived(machine: &str, at: Timestamp, now: Timestamp) -> Line {
    Line::text(format!("From {machine} · arrived ")).time(moment(at, now))
}

/// What a new reading decided for each of its results, counted: "2 new
/// results, 1 correction". Results Analyte kept for review say so.
pub(in crate::admin) fn outcomes(outcomes: &[RevisionOutcome]) -> String {
    let count = |wanted: fn(&RevisionOutcome) -> bool| {
        u64::try_from(outcomes.iter().filter(|outcome| wanted(outcome)).count()).unwrap_or(u64::MAX)
    };
    let parts: Vec<String> = [
        (
            count(|o| matches!(o, RevisionOutcome::New(_))),
            "new result",
            "new results",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Correction(_))),
            "correction",
            "corrections",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Retransmission(_))),
            "result already received",
            "results already received",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Conflict)),
            "conflicting result kept for review",
            "conflicting results kept for review",
        ),
        (
            count(|o| matches!(o, RevisionOutcome::Unresolved)),
            "result without an identity kept for review",
            "results without an identity kept for review",
        ),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| amount(n, one, many))
    .collect();
    if parts.is_empty() {
        "no results in it".into()
    } else {
        parts.join(", ")
    }
}

/// A Needs review item's stored reason as people read it. Only a reason whose
/// shape Analyte wrote itself gets words: a typed spelling read back exactly
/// (such as a mapping finding) or a fixed sentence the code writes. Any other
/// text, such as a parser diagnostic or the lab app's own detail, has no
/// sentence and shows as stored. Test codes and values are never named; they
/// stay in the protected original message.
pub(in crate::admin) struct Explained {
    /// What the problem is, as the item's title; none when only its category is known.
    pub(in crate::admin) title: Option<&'static str>,
    /// One plain sentence; none when the reason shows as stored.
    pub(in crate::admin) sentence: Option<String>,
    /// Facts for support beside the stored reason, such as the lab app's HTTP status.
    pub(in crate::admin) support: Vec<String>,
}

impl Explained {
    fn said(title: &'static str, sentence: impl Into<String>) -> Self {
        Self {
            title: Some(title),
            sentence: Some(sentence.into()),
            support: Vec::new(),
        }
    }

    const fn as_stored() -> Self {
        Self {
            title: None,
            sentence: None,
            support: Vec::new(),
        }
    }

    fn with_support(mut self, line: String) -> Self {
        self.support.push(line);
        self
    }
}

/// Explains `stored`, the reason of an item of `kind`.
pub(in crate::admin) fn explain(kind: ParkedKind, stored: &str) -> Explained {
    match kind {
        ParkedKind::MappingFinding => mapping_finding(stored),
        ParkedKind::InterpretationFailure => fixed(stored, &READING_REASONS),
        ParkedKind::ResultReconciliation => reconciliation(stored),
        ParkedKind::IncompleteTransfer => fixed(stored, &TRANSFER_REASONS),
        ParkedKind::DeliveryRejected => rejected_delivery(stored),
        // Only HTTP 409 parks a delivery as a conflict (ADR 0008).
        ParkedKind::DeliveryConflict => {
            fixed(stored, &CONFLICT_REASONS).with_support(lab_app_answer("409"))
        }
        ParkedKind::OrderVersionRefused => refused_order(stored),
        ParkedKind::CancelConflict => fixed(stored, &CANCEL_REASONS),
        ParkedKind::PollIntent => fixed(stored, &POLL_REASONS),
    }
}

/// How the lab app answered, under "For support": "Lab app answer: HTTP 409".
fn lab_app_answer(status: &str) -> String {
    format!("Lab app answer: HTTP {status}")
}

/// A fixed reason the code writes, its title and its sentence.
type Fixed = (&'static str, &'static str, &'static str);

fn fixed(stored: &str, known: &[Fixed]) -> Explained {
    known
        .iter()
        .find(|(reason, _, _)| *reason == stored)
        .map_or_else(Explained::as_stored, |(_, title, sentence)| {
            Explained::said(title, *sentence)
        })
}

/// Where a mapping finding points: "record 6, field 3", or "record 6" when
/// field zero means the whole record.
fn place(position: Position) -> String {
    match position {
        Position { record, field: 0 } => format!("record {record}"),
        Position { record, field } => format!("record {record}, field {field}"),
    }
}

/// The title and sentence of a mapping problem found at `at`.
fn problem_words(problem: &Problem, at: Position) -> (&'static str, String) {
    let at = place(at);
    match problem {
        Problem::UnknownTest => (
            "Test not in this machine’s test mapping",
            format!("The test code in {at} is not in this machine’s test mapping."),
        ),
        Problem::MissingSample => (
            "Sample ID missing",
            format!("The message has no sample ID in {at}."),
        ),
        Problem::MissingIdentity { .. } => (
            "Result identity missing",
            format!(
                "The value this machine’s settings use to tell results apart is missing in {at}."
            ),
        ),
        Problem::ValueKindMismatch { expected } => (
            "Value does not fit the test mapping",
            match expected {
                ValueKind::Numeric => format!(
                    "The value in {at} is not a number, but this machine’s test mapping \
                     expects a number for this test."
                ),
                ValueKind::Coded => format!(
                    "The value in {at} is not a code, but this machine’s test mapping \
                     expects a code for this test."
                ),
                ValueKind::Either => format!(
                    "The value in {at} is neither a number nor a code, but this machine’s \
                     test mapping expects one of the two for this test."
                ),
            },
        ),
        Problem::ComponentParts => (
            "Result parts do not match the test mapping",
            format!(
                "The result in {at} has other parts than this machine’s test mapping lists \
                 for the test."
            ),
        ),
        Problem::UnsupportedValue => (
            "Value cannot be read",
            format!("The value in {at} is of a kind Analyte cannot read safely."),
        ),
        Problem::UnsupportedFormatting => (
            "Text with formatting codes",
            format!(
                "The text in {at} carries formatting codes; Analyte keeps them as sent and \
                 does not turn them into plain text."
            ),
        ),
        Problem::UnsupportedEscape => (
            "Escape code not agreed",
            format!(
                "The text in {at} uses an escape code that this machine’s settings do not \
                 say how to read."
            ),
        ),
        Problem::UnknownProcessing => (
            "Processing mode unknown",
            format!("The processing mode in {at} is missing or not one Analyte knows."),
        ),
        Problem::ConflictingClassification => (
            "Result classified more than one way",
            format!(
                "This machine’s quality control rules give the result in {at} more than one \
                 classification."
            ),
        ),
        Problem::NoObservations => ("No results in the message", NO_RESULTS.to_owned()),
        Problem::UnsupportedLawStatusOnly => (
            "Status-only update",
            format!(
                "The message in {at} only changes a result’s status, and Analyte cannot \
                 record such updates yet."
            ),
        ),
        Problem::InvalidTime => (
            "Time cannot be read",
            format!("The time in {at} could not be read; the text as sent is kept."),
        ),
        Problem::UnqualifiedHl7Report => (
            "Result report incomplete",
            format!(
                "The result report in {at} lacks a field, or a result status, that a report \
                 must have."
            ),
        ),
        Problem::DepartureConflict => departure_conflict(&at),
        Problem::DerivedStatusContract => (DERIVED_STATUS.0, DERIVED_STATUS.1.to_owned()),
    }
}

const NO_RESULTS: &str = "The message holds no results.";

fn departure_conflict(at: &str) -> (&'static str, String) {
    (
        "Result status not final",
        format!(
            "The result status in {at} is not final, but this machine is set up as sending \
             final results only."
        ),
    )
}

const DERIVED_STATUS: (&str, &str) = (
    "Final status cannot be sent",
    concat!(
        "The result’s final status comes from what this machine is documented to send, not ",
        "from the message, and the lab app contract in use cannot say so.",
    ),
);

fn mapping_finding(stored: &str) -> Explained {
    if let Some(finding) = Finding::from_stored(stored) {
        let (title, sentence) = problem_words(&finding.problem, finding.position);
        return Explained::said(title, sentence);
    }
    if let Some(at) = interpretation::departure_conflict_at(stored) {
        let (title, sentence) = departure_conflict(&place(at));
        return Explained::said(title, sentence);
    }
    if stored == interpretation::DERIVED_STATUS_REASON {
        return Explained::said(DERIVED_STATUS.0, DERIVED_STATUS.1);
    }
    Unidentified::from_stored(stored).map_or_else(Explained::as_stored, unidentified)
}

/// Why a result has no identity, in words.
fn unidentified(reason: Unidentified) -> Explained {
    const MISSING: &str = "Result identity missing";
    match reason {
        Unidentified::Missing { setting, position } => {
            let (title, sentence) = problem_words(&Problem::MissingIdentity { setting }, position);
            Explained::said(title, sentence)
        }
        Unidentified::UnknownTest => Explained::said(
            "Test not in this machine’s test mapping",
            "A test code in this message is not in this machine’s test mapping, so its result \
             has no identity.",
        ),
        Unidentified::AutolyserRun => Explained::said(
            MISSING,
            "This machine sends no run number that stays the same, and its patient code is not \
             a trusted sample ID, so Analyte cannot tell this result apart from others.",
        ),
        Unidentified::PoctRepeated => Explained::said(
            "Result identity repeated",
            "Two results in this message would have the same identity, and no sub-ID tells them \
             apart.",
        ),
        Unidentified::PoctEvidence => Explained::said(
            MISSING,
            "The device, time or sequence number that this machine’s settings use to tell \
             results apart is missing.",
        ),
        Unidentified::PoctSequence => Explained::said(
            "Result identity not trusted",
            "This machine’s sequence numbers can start over, so they cannot tell results apart \
             without a trusted run and revision.",
        ),
        Unidentified::VidasLiteral => Explained::said(
            MISSING,
            "This machine’s messages carry no trusted result or run identity, and do not say \
             whether a result is a patient or a quality control result.",
        ),
    }
}

/// The fixed reasons a message that could not be read is parked with.
const READING_REASONS: [Fixed; 15] = [
    (
        NO_RESULTS_REASON,
        "Processing again produced no results",
        "Processing the message again with the current settings produced no results.",
    ),
    (
        astm::CONFLICT_REASON,
        "Piece resent with other content",
        "The machine sent the same numbered piece of a message twice, with different content.",
    ),
    (
        astm::UNSOLICITED_REASON,
        "Piece arrived out of turn",
        "A piece of a message arrived when no transfer from the machine was open.",
    ),
    (
        hl7::UNSOLICITED_REASON,
        "Message not expected",
        "A message arrived that Analyte did not expect, or while too many messages were waiting.",
    ),
    (
        law::INTERRUPTED_REASON,
        "Processing interrupted",
        "Processing of this message stopped before it was confirmed as complete.",
    ),
    (
        law::policy::WORK_CONFLICT_REASON,
        "Returned work differs from the order sent",
        "The work the machine returned differs from the order Analyte sent it.",
    ),
    (
        law::policy::MAPPING_REFUSAL,
        "Results held back for review",
        concat!(
            "Part of this message needs review, and this machine’s results are recorded all ",
            "together or not at all, so none of them were recorded.",
        ),
    ),
    (
        law::policy::BATCH_CONFLICT_REASON,
        "Results conflict with earlier ones",
        concat!(
            "Results in this message conflict with results this machine sent earlier, so none ",
            "of them were recorded.",
        ),
    ),
    (
        poct::LIMIT_REASON,
        "Message too large",
        "The message was larger than the size limit for this machine.",
    ),
    (
        poct::CANCELLED_REASON,
        "Message cut off",
        "The connection was stopped while the message was arriving; the part that arrived is kept.",
    ),
    (
        poct::DEADLINE_REASON,
        "Message not finished in time",
        "The machine did not finish sending the message in time; the part that arrived is kept.",
    ),
    (
        poct::INCOMPLETE_REASON,
        "Message cut off",
        concat!(
            "The connection timed out or closed before the message was complete; the part that ",
            "arrived is kept.",
        ),
    ),
    (
        poct::DEFERRED_REASON,
        "Message arrived while busy",
        concat!(
            "The message arrived while too many messages were waiting to be stored, so Analyte ",
            "kept it here without reading it.",
        ),
    ),
    (
        vidas::SUFFIX_REASON,
        "Check characters do not match",
        "The check characters at the end of the message do not match its content.",
    ),
    (
        vidas::INCOMPLETE_REASON,
        "Transfer cut off or refused",
        "The transfer from the machine did not finish, or was refused; what arrived is kept.",
    ),
];

/// The fixed reason an edited result is parked with when nothing shows what changed.
const EDITED_REASONS: [Fixed; 1] = [(
    poct::EDIT_GUARD_REASON,
    "Edited result kept for review",
    concat!(
        "The machine sent an edited result whose content looks unchanged, so Analyte cannot ",
        "tell what was edited.",
    ),
)];

/// The fixed reasons an unfinished transfer is parked with.
const TRANSFER_REASONS: [Fixed; 2] = [
    (
        astm::CUT_OFF_REASON,
        "Transfer cut off",
        "The machine stopped sending before the message was complete; the pieces that arrived are kept.",
    ),
    (
        astm::STOPPED_REASON,
        "Transfer cut off by a stop",
        "Analyte stopped while the machine was sending; the pieces it had confirmed are kept.",
    ),
];

/// The fixed reason a conflicting delivery is parked with when the lab app gave no detail.
const CONFLICT_REASONS: [Fixed; 1] = [(
    analyte_sink::CONFLICT_REASON,
    "Lab app already has different content",
    "The lab app already holds different content under this delivery’s ID.",
)];

const CANCEL_REASONS: [Fixed; 1] = [(
    CANCEL_CONFLICT_REASON,
    "Machine’s cancel answers disagree",
    concat!(
        "The machine answered that it could not cancel a test that it had earlier reported ",
        "as cancelled on its own.",
    ),
)];

/// What an open request for a machine's next results is called, as its category and its title.
pub(in crate::admin) const POLL_TITLE: &str = "Request for results not settled";

const POLL_REASONS: [Fixed; 1] = [(
    autolyser::POLL_REASON,
    POLL_TITLE,
    concat!(
        "Analyte asked this machine for its next results, and the machine may count them as ",
        "sent before their answer was safely stored.",
    ),
)];

fn reconciliation(stored: &str) -> Explained {
    if stored == poct::EDIT_GUARD_REASON {
        return fixed(stored, &EDITED_REASONS);
    }
    let kept = |what: &str| {
        Explained::said(
            "Result kept for review",
            format!(
                "The result was recorded as {what}, but kept for review: it has no sample ID, \
                 or it holds a value Analyte cannot read safely."
            ),
        )
    };
    match reconciled_outcome(stored) {
        Some(RevisionOutcome::Conflict) => Explained::said(
            "Result differs from an earlier one",
            "A result came with other content under the same identity as an earlier result, \
             and nothing marks it as a correction.",
        ),
        Some(RevisionOutcome::Unresolved) => Explained::said(
            "Result identity missing",
            "The message does not give this result an identity, so Analyte cannot tell whether \
             it is new, repeated or a correction.",
        ),
        Some(RevisionOutcome::New(_)) => kept("new"),
        Some(RevisionOutcome::Correction(_)) => kept("a correction"),
        Some(RevisionOutcome::Retransmission(_)) => kept("already received"),
        None => Explained::as_stored(),
    }
}

fn rejected_delivery(stored: &str) -> Explained {
    const UNREADABLE: &str = "Lab app’s answer could not be read";
    if stored == analyte_sink::UNSUPPORTED_CONTRACT_REASON {
        return Explained::said(
            "Delivery stored for another contract",
            "The delivery was stored for a lab app contract version that this version of \
             Analyte cannot send.",
        );
    }
    match ReplyProblem::from_stored(stored) {
        Some(ReplyProblem::InvalidSuccess) => Explained::said(
            UNREADABLE,
            "The lab app answered that it took the delivery, but its answer does not follow \
             the lab app contract, so Analyte does not count it as delivered.",
        )
        .with_support(lab_app_answer("200")),
        Some(ReplyProblem::InvalidError) => Explained::said(
            UNREADABLE,
            "The lab app refused the delivery, but its answer does not follow the lab app \
             contract, so its reason is not known.",
        )
        .with_support(lab_app_answer("400 or 422")),
        Some(ReplyProblem::UnexpectedStatus(status)) => Explained::said(
            "Unexpected answer from the lab app",
            "The lab app gave an answer that the lab app contract does not define for a \
             delivery.",
        )
        .with_support(lab_app_answer(&status.to_string())),
        None => Explained::as_stored(),
    }
}

fn refused_order(stored: &str) -> Explained {
    match OrderRefusal::from_stored(stored) {
        Some(OrderRefusal::Older { offered, kept }) => Explained::said(
            "Older order version refused",
            format!(
                "The lab app sent version {offered} of an order after Analyte already had \
                 version {kept}; Analyte kept version {kept}."
            ),
        ),
        Some(OrderRefusal::ChangedContent { kept }) => Explained::said(
            "Changed order refused",
            format!(
                "The lab app sent other content under version {kept} of an order that Analyte \
                 already had; Analyte kept its copy."
            ),
        ),
        None => Explained::as_stored(),
    }
}

/// An order's state in words a lab manager uses; pages keep the stored state on hover.
pub(in crate::admin) const fn order_state(state: OrderState) -> &'static str {
    match state {
        OrderState::Requested => "Not sent yet",
        OrderState::Sending => "Sending",
        OrderState::Sent => "Sent, not yet accepted",
        OrderState::Accepted => "Accepted by the machine",
        OrderState::Done => "Done",
        OrderState::Rejected => "Rejected",
        OrderState::CancelRequested => "Cancel requested",
        OrderState::CancelConfirmed => "Cancelled",
        OrderState::Uncertain => "Outcome unknown: check the machine",
    }
}

/// A delivery's state in plain words; pages keep the stored state on hover.
pub(in crate::admin) const fn delivery_state(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::Pending => "Waiting to be sent",
        DeliveryState::InFlight => "Sending",
        DeliveryState::Done => "Delivered",
        DeliveryState::Parked => "Rejected by the lab app",
        DeliveryState::Paused => "Paused: lab app settings need repair",
        DeliveryState::Conflict => "Lab app reports a conflict",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner request 2026-11-19: sizes read like a disk, with the exact bytes kept elsewhere.
    #[test]
    fn sizes_use_the_largest_readable_unit() {
        for (count, expected) in [
            (0, "0 bytes"),
            (1, "1 byte"),
            (2, "2 bytes"),
            (1_023, "1023 bytes"),
            (1_024, "1 KB"),
            (1_536, "1.5 KB"),
            (1_048_575, "1 MB"),
            (1_073_741_824, "1 GB"),
            (10 * 1_073_741_824, "10 GB"),
            (511_966_605_312, "477 GB"),
            (u64::MAX, "16 EB"),
        ] {
            assert_eq!(bytes(count), expected, "{count}");
        }
    }

    #[test]
    fn durations_use_the_largest_whole_unit() {
        for (seconds, expected) in [
            (0, "0 s"),
            (1, "1 s"),
            (2, "2 s"),
            (59, "59 s"),
            (60, "1 min"),
            (180, "3 min"),
            (7_200, "2 h"),
            (86_400, "1 day"),
            (172_800, "2 days"),
            (u64::MAX, "213503982334601 days"),
        ] {
            assert_eq!(duration(seconds), expected, "{seconds}");
        }
    }

    // Owner request 2026-11-19 (admin restyle): under a minute reads "less than
    // a minute ago" instead of a count of seconds such as "0 s ago".
    #[test]
    fn ages_read_as_ago_and_never_go_negative() -> Result<(), jiff::Error> {
        let now = Timestamp::from_second(1_795_046_077)?;
        assert_eq!(ago(now, now), "less than a minute ago");
        assert_eq!(
            ago(Timestamp::from_second(1_795_046_076)?, now),
            "less than a minute ago"
        );
        assert_eq!(
            ago(Timestamp::from_second(1_795_046_018)?, now),
            "less than a minute ago"
        );
        assert_eq!(
            ago(Timestamp::from_second(1_795_046_017)?, now),
            "1 min ago"
        );
        assert_eq!(
            ago(Timestamp::from_second(1_795_045_897)?, now),
            "3 min ago"
        );
        assert_eq!(ago(Timestamp::UNIX_EPOCH, now), "20775 days ago");
        assert_eq!(
            ago(Timestamp::from_second(1_795_046_078)?, now),
            "unknown (clock moved back)"
        );
        Ok(())
    }

    #[test]
    fn times_show_local_date_and_keep_the_exact_value() -> Result<(), jiff::Error> {
        let at = Timestamp::from_second(1_795_046_077)?;
        assert_eq!(time_in(at, &TimeZone::UTC), "2026-11-18 23:54");
        assert_eq!(
            time_in(Timestamp::UNIX_EPOCH, &TimeZone::UTC),
            "1970-01-01 00:00"
        );
        let html = when(at, at);
        assert!(html.contains("datetime=\"2026-11-18T23:54:37Z\""));
        assert!(html.contains("title=\"2026-11-18T23:54:37Z (Unix 1795046077)\""));
        assert!(html.contains("less than a minute ago"));
        assert!(when_unix(i64::MAX, at).contains("Unix time 9223372036854775807"));
        Ok(())
    }

    // Owner request 2026-11-19 (admin restyle): a time reads as the clock today,
    // with day and month earlier this year and the full date before that.
    #[test]
    fn clock_times_grow_only_as_far_as_needed() -> Result<(), jiff::Error> {
        let now = Timestamp::from_second(1_795_046_077)?; // 2026-11-18 23:54:37 UTC
        let utc = &TimeZone::UTC;
        assert_eq!(clock_in(now, now, utc), "23:54");
        assert_eq!(
            clock_in(Timestamp::from_second(1_794_960_000)?, now, utc),
            "00:00"
        );
        assert_eq!(
            clock_in(Timestamp::from_second(1_794_959_999)?, now, utc),
            "17 Nov 23:59"
        );
        assert_eq!(
            clock_in(Timestamp::UNIX_EPOCH, now, utc),
            "1970-01-01 00:00"
        );
        assert_eq!(today_in(now, utc), "Wednesday 18 November · 23:54");
        // Owner request 2026-11-19 (screenshot review, PR #273): the sidebar
        // says when the page was loaded, to the minute, until pages are live.
        assert_eq!(page_loaded_in(now, utc), "Page loaded 23:54");
        let shown = moment_unix(100, now);
        assert_eq!(shown.datetime, "1970-01-01T00:01:40Z");
        assert_eq!(shown.exact, "1970-01-01T00:01:40Z (Unix 100)");
        let invalid = moment_unix(i64::MAX, now);
        assert!(invalid.datetime.is_empty());
        assert_eq!(
            invalid.shown,
            "Unix time 9223372036854775807 (not a valid date)"
        );
        Ok(())
    }

    // Owner request 2026-11-19 (admin restyle): each link type says only what
    // Analyte observed, with the time of the last change.
    #[test]
    fn link_sentences_say_only_what_was_observed() {
        let since = "10:41";
        for (connection, light, expected) in [
            (
                Connection::Serial("COM3".into()),
                Light::Off,
                "Port COM3 not open since 10:41",
            ),
            (
                Connection::Serial("COM3".into()),
                Light::Live,
                "Port COM3 open since 10:41",
            ),
            (
                Connection::UsbAdapter("A9X".into()),
                Light::Off,
                "Port of USB adapter A9X not open since 10:41",
            ),
            (
                Connection::Listening(Some(5000)),
                Light::Waiting,
                "Listening on port 5000, nothing connected since 10:41",
            ),
            (
                Connection::Listening(Some(5000)),
                Light::Live,
                "Machine connected on port 5000 since 10:41",
            ),
            (
                Connection::Listening(None),
                Light::Off,
                "Not listening since 10:41",
            ),
            (
                Connection::Dialing,
                Light::Off,
                "No connection to lumi since 10:41",
            ),
            (
                Connection::Dialing,
                Light::Live,
                "Connected to lumi since 10:41",
            ),
            (
                Connection::Folder("results".into()),
                Light::Live,
                "Watching results",
            ),
            (
                Connection::Folder("results".into()),
                Light::Off,
                "Not watching results since 10:41",
            ),
        ] {
            let sentence = link_sentence(&connection, light, since, "lumi");
            assert_eq!(sentence, expected);
            for unseen in [
                "cable not connected",
                "switched on",
                "onnected to the machine",
            ] {
                assert!(!sentence.contains(unseen), "{sentence}");
            }
        }
        // A port that will not open is a problem on this PC, not at the machine.
        assert_eq!(
            link_advice(&Connection::Serial("COM3".into())),
            "Check that the USB adapter or cable is plugged into this PC, that COM3 is the right port, and that no other program is using it."
        );
        assert_eq!(
            link_advice(&Connection::UsbAdapter("FT4XQ2B9".into())),
            "Check that the USB adapter with serial FT4XQ2B9 is plugged into this PC and that no other program is using its port."
        );
        assert_eq!(
            link_advice(&Connection::Dialing),
            "Check the network cable and the machine's host settings."
        );
        assert_eq!(
            connection_kind(&Connection::UsbAdapter("A9X".into())),
            "USB adapter"
        );
        assert_eq!(connection_kind(&Connection::Dialing), "Network");
    }

    // Owner request 2026-11-19 (admin restyle): lab app lines use real values
    // and say so when there is nothing to show.
    #[test]
    fn lab_app_lines_read_plainly() -> Result<(), jiff::Error> {
        let now = Timestamp::from_second(1_795_046_077)?;
        let earlier = Timestamp::from_second(1_795_045_000)?;
        assert_eq!(
            last_delivered(None, false, now).plain(),
            "Nothing delivered yet"
        );
        assert_eq!(
            last_delivered(None, true, now).plain(),
            "No delivery in the kept records"
        );
        assert!(
            last_delivered(Some(earlier), true, now)
                .plain()
                .starts_with("Last delivered ")
        );
        assert_eq!(waiting(0, None, now).plain(), "nothing waiting");
        assert_eq!(
            waiting(1, None, now).plain(),
            "1 item waiting · oldest not known"
        );
        assert!(
            waiting(4, Some(earlier), now)
                .plain()
                .starts_with("4 items waiting · oldest ")
        );
        assert_eq!(senders(&[]), "No set-up machine sends here");
        assert_eq!(
            senders(&["cobas-e411", "iflash"]),
            "From cobas-e411, iflash"
        );
        assert_eq!(
            missing_keys(&["API key", "signing key"]),
            "Keys missing: API key and signing key."
        );
        assert_eq!(missing_keys(&["signing key"]), "Key missing: signing key.");
        assert!(missing_keys(&[]).contains("after it restarts"));
        assert_eq!(machines_and_lab_apps(1, 2), "1 machine · 2 lab apps");
        Ok(())
    }

    // Owner request 2026-11-19 (overview: delivery failing): the node line says
    // when the last try failed, how many failed since when, and when the next
    // try is due, with real times only; no cause, as none is stored.
    #[test]
    fn failing_line_reads_tries_and_times() -> Result<(), Box<dyn std::error::Error>> {
        let now = Timestamp::from_second(1_795_046_077)?;
        let clock = |second| Timestamp::from_second(second).map(|at| clock(at, now));
        let failing = |since, last, tries, next| FailingDestination {
            destination: analyte_model::Identifier::new("main-lab").expect("a valid test name"),
            since,
            last,
            tries,
            next,
        };
        let (since, last, next) = (1_795_043_400, 1_795_046_000, 1_795_046_100);
        assert_eq!(
            super::failing(&failing(since, last, 74, Some(next)), now).plain(),
            format!(
                "Last try {} failed · 74 failed tries since {} · next try {}",
                clock(last)?,
                clock(since)?,
                clock(next)?
            )
        );
        assert_eq!(
            super::failing(&failing(last, last, 1, Some(last)), now).plain(),
            format!(
                "Last try {0} failed · 1 failed try since {0} · next try due",
                clock(last)?
            )
        );
        assert!(
            super::failing(&failing(since, last, 2, None), now)
                .plain()
                .ends_with(" · a try is under way")
        );
        Ok(())
    }

    // Owner review 2026-11-19 (structure review PR 2): positions read as record
    // and field, the stored reason stays on hover, and other reasons are unchanged.
    #[test]
    fn reasons_show_positions_plainly_and_keep_the_stored_text() {
        assert_eq!(
            &*reason("Position { record: 3, field: 3 }: UnknownTest"),
            concat!(
                "<span title=\"Position { record: 3, field: 3 }: UnknownTest\">",
                "record 3, field 3: UnknownTest</span>",
            )
        );
        assert_eq!(
            &*reason(
                "bad <a> at Position { record: 7, field: 0 }, see Position { record: 8, field: 2 }"
            ),
            concat!(
                "<span title=\"bad &lt;a&gt; at Position { record: 7, field: 0 }, see ",
                "Position { record: 8, field: 2 }\">bad &lt;a&gt; at record 7, see record 8, ",
                "field 2</span>",
            )
        );
        for stored in ["checksum mismatch in frame 2", "Position { record: x }", ""] {
            assert_eq!(&*reason(stored), &*Markup::new().text(stored));
        }
    }

    #[test]
    fn counts_agree_with_their_word() {
        assert_eq!(amount(0u64, "result", "results"), "0 results");
        assert_eq!(amount(1u64, "result", "results"), "1 result");
        assert_eq!(amount(2u64, "result", "results"), "2 results");
        assert_eq!(
            amount(u64::MAX, "order", "orders"),
            "18446744073709551615 orders"
        );
        assert_eq!(yes_no(true), "Yes");
        assert_eq!(yes_no(false), "No");
    }

    // Owner decision 2026-11-19 (admin restyle b1): history lines name the
    // admin pages' sessions "admin session" and never a made-up person; other
    // recorded actors read as recorded.
    #[test]
    fn actors_read_as_admin_session_analyte_or_as_recorded() {
        assert_eq!(
            actor("admin-session-0193a1b2-0000-7000-8000-000000000001"),
            "admin session"
        );
        assert_eq!(actor("analyte"), "Analyte");
        assert_eq!(actor("night-shift"), "night-shift");
        assert_eq!(
            recorded("admin-session-1", "settled"),
            "admin session · \u{201c}settled\u{201d}"
        );
    }

    // Owner decision 2026-11-19 (admin restyle b1): each history line says what
    // the recorded action did, from the targets Analyte stored with it.
    #[test]
    fn settled_actions_read_from_the_stored_action_and_targets() {
        assert_eq!(settled("parked-resolve", &["7"]), "Marked resolved");
        assert_eq!(
            settled(
                "receipt-reprocess",
                &["r", "interpretation:r:3", "reprocess:succeeded", "parked:7"]
            ),
            "Processed again: reading 3 recorded"
        );
        assert_eq!(
            settled(
                "receipt-reprocess",
                &["r", "interpretation:r:2", "reprocess:parked", "parked:7"]
            ),
            "Processed again: reading 2 recorded; it still needs review"
        );
        assert_eq!(settled("receipt-reprocess", &[]), "Processed again");
        assert_eq!(settled("autolyser_poll_custody_complete", &[]), "Resolved");
    }

    // Owner decision 2026-11-19 (admin restyle b1): "Reading N recorded" says
    // what the new reading decided, counted, and what was kept for review.
    #[test]
    fn reprocess_outcomes_are_counted_in_words() {
        use analyte_model::Revision;
        assert_eq!(outcomes(&[]), "no results in it");
        assert_eq!(
            outcomes(&[
                RevisionOutcome::New(Revision(0)),
                RevisionOutcome::New(Revision(0)),
                RevisionOutcome::Correction(Revision(1)),
                RevisionOutcome::Retransmission(Revision(0)),
                RevisionOutcome::Conflict,
                RevisionOutcome::Unresolved,
                RevisionOutcome::Unresolved,
            ]),
            concat!(
                "2 new results, 1 correction, 1 result already received, ",
                "1 conflicting result kept for review, ",
                "2 results without an identity kept for review"
            )
        );
    }

    // ADR 0018: the restyled pages show a stored reason in plain words with the
    // exact text on hover, by the same rule as the old pages.
    #[test]
    fn reason_words_follow_the_reason_rule() {
        assert_eq!(
            reason_words("Position { record: 3, field: 0 }: MissingSample").as_deref(),
            Some("record 3: MissingSample")
        );
        assert_eq!(reason_words("checksum mismatch"), None);
    }

    /// What `explain` gives for `stored` under `kind`: title, sentence and support.
    fn explained(
        kind: ParkedKind,
        stored: &str,
    ) -> (Option<&'static str>, Option<String>, Vec<String>) {
        let explained = explain(kind, stored);
        (explained.title, explained.sentence, explained.support)
    }

    /// Checks that `stored` reads as `title` and exactly `sentence`, with no support line.
    fn says(kind: ParkedKind, stored: &str, title: &str, sentence: &str) {
        assert_eq!(
            explained(kind, stored),
            (Some(title), Some(sentence.to_owned()), Vec::new()),
            "{stored}"
        );
    }

    /// Each mapping problem, its title and its sentence for record 6, field 3.
    const PROBLEM_WORDS: [(Problem, &str, &str); 18] = [
        (
            Problem::UnknownTest,
            "Test not in this machine’s test mapping",
            "The test code in record 6, field 3 is not in this machine’s test mapping.",
        ),
        (
            Problem::MissingSample,
            "Sample ID missing",
            "The message has no sample ID in record 6, field 3.",
        ),
        (
            Problem::ValueKindMismatch {
                expected: ValueKind::Numeric,
            },
            "Value does not fit the test mapping",
            concat!(
                "The value in record 6, field 3 is not a number, but this machine’s test ",
                "mapping expects a number for this test.",
            ),
        ),
        (
            Problem::ValueKindMismatch {
                expected: ValueKind::Coded,
            },
            "Value does not fit the test mapping",
            concat!(
                "The value in record 6, field 3 is not a code, but this machine’s test ",
                "mapping expects a code for this test.",
            ),
        ),
        (
            Problem::ValueKindMismatch {
                expected: ValueKind::Either,
            },
            "Value does not fit the test mapping",
            concat!(
                "The value in record 6, field 3 is neither a number nor a code, but this ",
                "machine’s test mapping expects one of the two for this test.",
            ),
        ),
        (
            Problem::ComponentParts,
            "Result parts do not match the test mapping",
            concat!(
                "The result in record 6, field 3 has other parts than this machine’s test ",
                "mapping lists for the test.",
            ),
        ),
        (
            Problem::UnsupportedValue,
            "Value cannot be read",
            "The value in record 6, field 3 is of a kind Analyte cannot read safely.",
        ),
        (
            Problem::UnsupportedFormatting,
            "Text with formatting codes",
            concat!(
                "The text in record 6, field 3 carries formatting codes; Analyte keeps them ",
                "as sent and does not turn them into plain text.",
            ),
        ),
        (
            Problem::UnsupportedEscape,
            "Escape code not agreed",
            concat!(
                "The text in record 6, field 3 uses an escape code that this machine’s ",
                "settings do not say how to read.",
            ),
        ),
        (
            Problem::UnknownProcessing,
            "Processing mode unknown",
            "The processing mode in record 6, field 3 is missing or not one Analyte knows.",
        ),
        (
            Problem::ConflictingClassification,
            "Result classified more than one way",
            concat!(
                "This machine’s quality control rules give the result in record 6, field 3 ",
                "more than one classification.",
            ),
        ),
        (
            Problem::NoObservations,
            "No results in the message",
            "The message holds no results.",
        ),
        (
            Problem::UnsupportedLawStatusOnly,
            "Status-only update",
            concat!(
                "The message in record 6, field 3 only changes a result’s status, and ",
                "Analyte cannot record such updates yet.",
            ),
        ),
        (
            Problem::InvalidTime,
            "Time cannot be read",
            "The time in record 6, field 3 could not be read; the text as sent is kept.",
        ),
        (
            Problem::UnqualifiedHl7Report,
            "Result report incomplete",
            concat!(
                "The result report in record 6, field 3 lacks a field, or a result status, ",
                "that a report must have.",
            ),
        ),
        (
            Problem::DepartureConflict,
            "Result status not final",
            concat!(
                "The result status in record 6, field 3 is not final, but this machine is ",
                "set up as sending final results only.",
            ),
        ),
        (
            Problem::DerivedStatusContract,
            "Final status cannot be sent",
            concat!(
                "The result’s final status comes from what this machine is documented to ",
                "send, not from the message, and the lab app contract in use cannot say so.",
            ),
        ),
        (
            Problem::MissingIdentity {
                setting: analyte_mapping::IdentitySetting::OrderId,
            },
            "Result identity missing",
            concat!(
                "The value this machine’s settings use to tell results apart is missing in ",
                "record 6, field 3.",
            ),
        ),
    ];

    // Owner review 2026-11-20 (PR 275): every mapping problem has one plain
    // sentence and its own title; the position reads as record and field (or
    // the record alone for field zero), and no test code or value is named.
    #[test]
    fn every_mapping_problem_reads_as_one_plain_sentence() {
        use analyte_mapping::IdentitySetting;
        let finding = ParkedKind::MappingFinding;
        for (problem, title, sentence) in PROBLEM_WORDS {
            let stored = format!("Position {{ record: 6, field: 3 }}: {problem}");
            says(finding, &stored, title, sentence);
        }
        // Field zero means the whole record.
        says(
            finding,
            "Position { record: 2, field: 0 }: MissingSample",
            "Sample ID missing",
            "The message has no sample ID in record 2.",
        );
        // Every identity setting reads back, and the setting name is for support only.
        for setting in IdentitySetting::ALL {
            let stored = format!(
                "Position {{ record: 4, field: 2 }}: {}",
                Problem::MissingIdentity { setting }
            );
            let (title, sentence, _) = explained(finding, &stored);
            assert_eq!(title, Some("Result identity missing"), "{stored}");
            assert!(
                !sentence.unwrap_or_default().contains(setting.as_str()),
                "{stored}"
            );
        }
    }

    /// Each reason a result has no identity, its title and its sentence.
    const UNIDENTIFIED_WORDS: [(Unidentified, &str, &str); 7] = [
        (
            Unidentified::Missing {
                setting: analyte_mapping::IdentitySetting::Run,
                position: Position {
                    record: 5,
                    field: 9,
                },
            },
            "Result identity missing",
            concat!(
                "The value this machine’s settings use to tell results apart is missing in ",
                "record 5, field 9.",
            ),
        ),
        (
            Unidentified::UnknownTest,
            "Test not in this machine’s test mapping",
            concat!(
                "A test code in this message is not in this machine’s test mapping, so its ",
                "result has no identity.",
            ),
        ),
        (
            Unidentified::AutolyserRun,
            "Result identity missing",
            concat!(
                "This machine sends no run number that stays the same, and its patient code ",
                "is not a trusted sample ID, so Analyte cannot tell this result apart from ",
                "others.",
            ),
        ),
        (
            Unidentified::PoctRepeated,
            "Result identity repeated",
            concat!(
                "Two results in this message would have the same identity, and no sub-ID ",
                "tells them apart.",
            ),
        ),
        (
            Unidentified::PoctEvidence,
            "Result identity missing",
            concat!(
                "The device, time or sequence number that this machine’s settings use to ",
                "tell results apart is missing.",
            ),
        ),
        (
            Unidentified::PoctSequence,
            "Result identity not trusted",
            concat!(
                "This machine’s sequence numbers can start over, so they cannot tell results ",
                "apart without a trusted run and revision.",
            ),
        ),
        (
            Unidentified::VidasLiteral,
            "Result identity missing",
            concat!(
                "This machine’s messages carry no trusted result or run identity, and do not ",
                "say whether a result is a patient or a quality control result.",
            ),
        ),
    ];

    // Owner review 2026-11-20 (PR 275): the other mapping findings the code
    // writes (a departure conflict, the derived-status sentence and each reason
    // a result has no identity) read plainly; anything else shows as stored.
    #[test]
    fn other_mapping_findings_read_plainly_and_free_text_shows_as_stored() {
        let finding = ParkedKind::MappingFinding;
        says(
            finding,
            concat!(
                "departure-conflict at Position { record: 3, field: 25 }: present non-final ",
                "status contradicts documented final-only scope",
            ),
            "Result status not final",
            concat!(
                "The result status in record 3, field 25 is not final, but this machine is set ",
                "up as sending final results only.",
            ),
        );
        says(
            finding,
            interpretation::DERIVED_STATUS_REASON,
            "Final status cannot be sent",
            concat!(
                "The result’s final status comes from what this machine is documented to send, ",
                "not from the message, and the lab app contract in use cannot say so.",
            ),
        );
        for (reason, title, sentence) in UNIDENTIFIED_WORDS {
            says(finding, &reason.to_string(), title, sentence);
        }
        for stored in [
            "Position { record: 3, field: 3 }: unknowntest",
            "missing identity something else at record 1 field 1",
            "a parser said something",
        ] {
            assert_eq!(
                explained(finding, stored),
                (None, None, Vec::new()),
                "{stored}"
            );
        }
        // A finding's spelling under another kind is not read as a finding.
        assert_eq!(
            explained(
                ParkedKind::InterpretationFailure,
                "Position { record: 3, field: 3 }: UnknownTest"
            ),
            (None, None, Vec::new())
        );
    }

    /// Each fixed reason the code writes, under its kind, with its title and sentence.
    const FIXED_WORDS: [(ParkedKind, &str, &str, &str); 19] = [
        (
            ParkedKind::InterpretationFailure,
            NO_RESULTS_REASON,
            "Processing again produced no results",
            "Processing the message again with the current settings produced no results.",
        ),
        (
            ParkedKind::InterpretationFailure,
            astm::CONFLICT_REASON,
            "Piece resent with other content",
            "The machine sent the same numbered piece of a message twice, with different content.",
        ),
        (
            ParkedKind::InterpretationFailure,
            astm::UNSOLICITED_REASON,
            "Piece arrived out of turn",
            "A piece of a message arrived when no transfer from the machine was open.",
        ),
        (
            ParkedKind::InterpretationFailure,
            hl7::UNSOLICITED_REASON,
            "Message not expected",
            "A message arrived that Analyte did not expect, or while too many messages were waiting.",
        ),
        (
            ParkedKind::InterpretationFailure,
            law::INTERRUPTED_REASON,
            "Processing interrupted",
            "Processing of this message stopped before it was confirmed as complete.",
        ),
        (
            ParkedKind::InterpretationFailure,
            law::policy::WORK_CONFLICT_REASON,
            "Returned work differs from the order sent",
            "The work the machine returned differs from the order Analyte sent it.",
        ),
        (
            ParkedKind::InterpretationFailure,
            law::policy::MAPPING_REFUSAL,
            "Results held back for review",
            concat!(
                "Part of this message needs review, and this machine’s results are recorded ",
                "all together or not at all, so none of them were recorded.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            law::policy::BATCH_CONFLICT_REASON,
            "Results conflict with earlier ones",
            concat!(
                "Results in this message conflict with results this machine sent earlier, so ",
                "none of them were recorded.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::LIMIT_REASON,
            "Message too large",
            "The message was larger than the size limit for this machine.",
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::CANCELLED_REASON,
            "Message cut off",
            "The connection was stopped while the message was arriving; the part that arrived is kept.",
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::DEADLINE_REASON,
            "Message not finished in time",
            "The machine did not finish sending the message in time; the part that arrived is kept.",
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::INCOMPLETE_REASON,
            "Message cut off",
            concat!(
                "The connection timed out or closed before the message was complete; the ",
                "part that arrived is kept.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            poct::DEFERRED_REASON,
            "Message arrived while busy",
            concat!(
                "The message arrived while too many messages were waiting to be stored, so ",
                "Analyte kept it here without reading it.",
            ),
        ),
        (
            ParkedKind::InterpretationFailure,
            vidas::SUFFIX_REASON,
            "Check characters do not match",
            "The check characters at the end of the message do not match its content.",
        ),
        (
            ParkedKind::InterpretationFailure,
            vidas::INCOMPLETE_REASON,
            "Transfer cut off or refused",
            "The transfer from the machine did not finish, or was refused; what arrived is kept.",
        ),
        (
            ParkedKind::IncompleteTransfer,
            astm::CUT_OFF_REASON,
            "Transfer cut off",
            concat!(
                "The machine stopped sending before the message was complete; the pieces ",
                "that arrived are kept.",
            ),
        ),
        (
            ParkedKind::IncompleteTransfer,
            astm::STOPPED_REASON,
            "Transfer cut off by a stop",
            "Analyte stopped while the machine was sending; the pieces it had confirmed are kept.",
        ),
        (
            ParkedKind::CancelConflict,
            CANCEL_CONFLICT_REASON,
            "Machine’s cancel answers disagree",
            concat!(
                "The machine answered that it could not cancel a test that it had earlier ",
                "reported as cancelled on its own.",
            ),
        ),
        (
            ParkedKind::PollIntent,
            autolyser::POLL_REASON,
            "Request for results not settled",
            concat!(
                "Analyte asked this machine for its next results, and the machine may count ",
                "them as sent before their answer was safely stored.",
            ),
        ),
    ];

    // Owner review 2026-11-20 (PR 275): each fixed sentence the code writes for
    // a message that could not be read, an unfinished transfer, a cancel
    // conflict or an unsettled request reads plainly; other text shows as stored.
    #[test]
    fn fixed_reasons_the_code_writes_read_plainly() {
        let reading = ParkedKind::InterpretationFailure;
        for (kind, stored, title, sentence) in FIXED_WORDS {
            says(kind, stored, title, sentence);
        }
        for kind in [
            reading,
            ParkedKind::IncompleteTransfer,
            ParkedKind::CancelConflict,
            ParkedKind::PollIntent,
        ] {
            assert_eq!(
                explained(kind, "POCT syntax: unexpected end"),
                (None, None, Vec::new())
            );
        }
        // A fixed sentence under another kind is not read as that kind's.
        assert_eq!(
            explained(ParkedKind::PollIntent, astm::CUT_OFF_REASON),
            (None, None, Vec::new())
        );
    }

    // Owner review 2026-11-20 (PR 275): a parked result's reconciliation
    // outcome and the edit guard read plainly; the revision number stays in the
    // stored reason under support.
    #[test]
    fn result_reconciliation_reasons_read_plainly() {
        let result = ParkedKind::ResultReconciliation;
        let kept = |what: &str| {
            format!(
                "The result was recorded as {what}, but kept for review: it has no sample ID, or \
                 it holds a value Analyte cannot read safely."
            )
        };
        for (stored, title, sentence) in [
            (
                "result reconciliation: Conflict".to_owned(),
                "Result differs from an earlier one",
                concat!(
                    "A result came with other content under the same identity as an earlier ",
                    "result, and nothing marks it as a correction.",
                )
                .to_owned(),
            ),
            (
                "result reconciliation: Unresolved".to_owned(),
                "Result identity missing",
                concat!(
                    "The message does not give this result an identity, so Analyte cannot tell ",
                    "whether it is new, repeated or a correction.",
                )
                .to_owned(),
            ),
            (
                "result reconciliation: New(Revision(0))".to_owned(),
                "Result kept for review",
                kept("new"),
            ),
            (
                "result reconciliation: Correction(Revision(2))".to_owned(),
                "Result kept for review",
                kept("a correction"),
            ),
            (
                "result reconciliation: Retransmission(Revision(1))".to_owned(),
                "Result kept for review",
                kept("already received"),
            ),
            (
                poct::EDIT_GUARD_REASON.to_owned(),
                "Edited result kept for review",
                concat!(
                    "The machine sent an edited result whose content looks unchanged, so Analyte ",
                    "cannot tell what was edited.",
                )
                .to_owned(),
            ),
        ] {
            says(result, &stored, title, &sentence);
        }
        for stored in [
            "result reconciliation: New(Revision(01))",
            "result reconciliation: conflict",
            "multiple final runs under the same order and test",
        ] {
            assert_eq!(
                explained(result, stored),
                (None, None, Vec::new()),
                "{stored}"
            );
        }
    }

    // Owner review 2026-11-20 (PR 275): a rejected delivery's reason reads
    // plainly when Analyte wrote it, with the HTTP status under support and
    // never in the sentence; the lab app's own detail shows as stored.
    #[test]
    fn delivery_and_order_reasons_keep_http_codes_for_support() {
        let rejected = ParkedKind::DeliveryRejected;
        let unreadable = "Lab app’s answer could not be read";
        for (stored, title, sentence, support) in [
            (
                "invalid success response",
                unreadable,
                concat!(
                    "The lab app answered that it took the delivery, but its answer does not ",
                    "follow the lab app contract, so Analyte does not count it as delivered.",
                ),
                "Lab app answer: HTTP 200",
            ),
            (
                "invalid error response",
                unreadable,
                concat!(
                    "The lab app refused the delivery, but its answer does not follow the lab app ",
                    "contract, so its reason is not known.",
                ),
                "Lab app answer: HTTP 400 or 422",
            ),
            (
                "unexpected HTTP status 418",
                "Unexpected answer from the lab app",
                "The lab app gave an answer that the lab app contract does not define for a delivery.",
                "Lab app answer: HTTP 418",
            ),
        ] {
            assert_eq!(
                explained(rejected, stored),
                (
                    Some(title),
                    Some(sentence.to_owned()),
                    vec![support.to_owned()]
                ),
                "{stored}"
            );
            assert!(!sentence.contains("HTTP"));
        }
        says(
            rejected,
            analyte_sink::UNSUPPORTED_CONTRACT_REASON,
            "Delivery stored for another contract",
            concat!(
                "The delivery was stored for a lab app contract version that this version of ",
                "Analyte cannot send.",
            ),
        );
        for stored in ["sample 12 is unknown", "unexpected HTTP status 0418"] {
            assert_eq!(
                explained(rejected, stored),
                (None, None, Vec::new()),
                "{stored}"
            );
        }
        // Only HTTP 409 parks a conflict: every one says so under support.
        let conflict = ParkedKind::DeliveryConflict;
        let http_409 = vec!["Lab app answer: HTTP 409".to_owned()];
        assert_eq!(
            explained(conflict, analyte_sink::CONFLICT_REASON),
            (
                Some("Lab app already has different content"),
                Some(
                    "The lab app already holds different content under this delivery’s ID."
                        .to_owned()
                ),
                http_409.clone()
            )
        );
        assert_eq!(
            explained(conflict, "result 7 already stored with other values"),
            (None, None, http_409)
        );
        let order = ParkedKind::OrderVersionRefused;
        says(
            order,
            "app sent order version 2 after cached version 5; kept 5",
            "Older order version refused",
            concat!(
                "The lab app sent version 2 of an order after Analyte already had version 5; ",
                "Analyte kept version 5.",
            ),
        );
        says(
            order,
            "app sent different content for order version 3; kept the cached one",
            "Changed order refused",
            concat!(
                "The lab app sent other content under version 3 of an order that Analyte already ",
                "had; Analyte kept its copy.",
            ),
        );
        assert_eq!(
            explained(
                order,
                "app sent order version 2 after cached version 5; kept 6"
            ),
            (None, None, Vec::new())
        );
    }

    // Owner review 2026-11-20 (PR 275): a settled item's action in short.
    #[test]
    fn settled_actions_read_in_short() {
        assert_eq!(settled_action("parked-resolve"), "Marked resolved");
        assert_eq!(settled_action("receipt-reprocess"), "Processed again");
        assert_eq!(
            settled_action("autolyser_poll_custody_complete"),
            "Resolved"
        );
    }

    // Owner review 2026-11-20 (PR 275): "today" is the PC's local day, the day
    // `clock` shows as a bare time; the span runs from its midnight to the next.
    #[test]
    fn today_runs_from_local_midnight_to_the_next() -> Result<(), Box<dyn std::error::Error>> {
        // A fixed UTC+1 zone: 2026-11-20 09:20 local is 08:20 UTC.
        let zone = TimeZone::fixed(jiff::tz::offset(1));
        let now: Timestamp = "2026-11-20T08:20:00Z".parse()?;
        let (from, until) = day_in(now, &zone).ok_or("no day")?;
        assert_eq!(
            Timestamp::from_second(from)?,
            "2026-11-19T23:00:00Z".parse()?
        );
        assert_eq!(
            Timestamp::from_second(until)?,
            "2026-11-20T23:00:00Z".parse()?
        );
        assert_eq!(clock_in(Timestamp::from_second(from)?, now, &zone), "00:00");
        assert_eq!(
            clock_in(Timestamp::from_second(from.saturating_sub(1))?, now, &zone),
            "19 Nov 23:59",
            "the second before is yesterday"
        );
        Ok(())
    }
}
