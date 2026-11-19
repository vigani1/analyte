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
use analyte_mapping::{Position, ReasonPiece};
use analyte_model::OrderState;
use analyte_store::DeliveryState;
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

/// The time a page was made, to the second: "10:45:12".
pub(in crate::admin) fn clock_seconds(at: Timestamp) -> String {
    at.to_zoned(TimeZone::system())
        .strftime("%H:%M:%S")
        .to_string()
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

    /// This line, a " · " separator, then `next`.
    pub(in crate::admin) fn then(mut self, next: Self) -> Self {
        self.0.push(Part::Text(" · ".into()));
        self.0.extend(next.0);
        self
    }

    pub(in crate::admin) fn parts(&self) -> &[Part] {
        &self.0
    }

    /// The line as people read it, each time as shown.
    #[cfg(test)]
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
pub(in crate::admin) const fn link_advice(connection: &Connection) -> &'static str {
    match connection {
        Connection::Serial(_) | Connection::UsbAdapter(_) => {
            "Check the cable and the machine's host settings."
        }
        Connection::Listening(_) | Connection::Dialing => {
            "Check the network cable and the machine's host settings."
        }
        Connection::Folder(_) => {
            "Check that the folder exists and that Analyte's service account may read it."
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
    "add the missing keys on the Lab apps page. Waiting items go out after ",
    "Analyte restarts with the keys."
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
    let pieces = Position::split_stored(stored);
    if !pieces
        .iter()
        .any(|piece| matches!(piece, ReasonPiece::Position(_)))
    {
        return Markup::new().text(stored);
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
    super::exact(&shown, stored)
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
        assert_eq!(
            link_advice(&Connection::Serial("COM3".into())),
            "Check the cable and the machine's host settings."
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
}
