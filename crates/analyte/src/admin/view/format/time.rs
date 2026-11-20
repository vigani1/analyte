//! Says times, durations and countdowns in the PC's own time zone, and builds the lines that carry a time with its exact value.
use super::amount;
use jiff::{Timestamp, tz::TimeZone};

/// A length of time in its largest whole unit: "45 s", "3 min", "2 h", "1 day".
pub(in crate::admin) fn duration(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3_600 => format!("{} min", seconds / 60),
        3_600..86_400 => format!("{} h", seconds / 3_600),
        _ => amount(seconds / 86_400, "day", "days"),
    }
}

/// A time limit as people read it: "1.5 s", "1.75 s", "10 s", "5 min"; whole
/// minutes and hours read as [`duration`] does.
pub(in crate::admin) fn limit(limit: std::time::Duration) -> String {
    let millis = limit.subsec_millis();
    if millis == 0 {
        return duration(limit.as_secs());
    }
    let fraction = format!("{millis:03}");
    format!("{}.{} s", limit.as_secs(), fraction.trim_end_matches('0'))
}

/// The time zone these pages show times in: this PC's, by its name when it
/// has one.
pub(in crate::admin) fn time_zone() -> String {
    match TimeZone::system().iana_name() {
        Some(name) => format!("This PC’s time zone ({name})"),
        None => "This PC’s time zone".into(),
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

/// What the sidebar says about when the page was made: "Page loaded 10:45",
/// the minute it was loaded. While the live stream is connected, `live.js`
/// shows [`live_clock`] instead and returns to this when the stream drops.
pub(in crate::admin) fn page_loaded(now: Timestamp) -> String {
    page_loaded_in(now, &TimeZone::system())
}

fn page_loaded_in(now: Timestamp, zone: &TimeZone) -> String {
    format!("Page loaded {}", clock_in(now, now, zone))
}

/// The time a live page last heard from Analyte, to the second: "10:45:12".
pub(in crate::admin) fn live_clock(now: Timestamp) -> String {
    live_clock_in(now, &TimeZone::system())
}

fn live_clock_in(now: Timestamp, zone: &TimeZone) -> String {
    now.to_zoned(zone.clone()).strftime("%H:%M:%S").to_string()
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
pub(in crate::admin) fn exact_time(at: Timestamp) -> String {
    format!("{at} (Unix {})", at.as_second())
}

/// A stored Unix time outside the calendar, shown as it is.
pub(super) fn not_a_date(seconds: i64) -> String {
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

    /// This line, then `moment` with no separator.
    pub(in crate::admin) fn time(mut self, moment: Moment) -> Self {
        self.0.push(Part::Time(moment));
        self
    }

    /// This line, then `text` with no separator.
    pub(in crate::admin) fn text_part(mut self, text: &str) -> Self {
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

/// Time left on a countdown: "0:41", "1:00".
pub(in crate::admin) fn countdown(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let moment = moment(at, at);
        assert_eq!(moment.datetime, "2026-11-18T23:54:37Z");
        assert_eq!(moment.exact, "2026-11-18T23:54:37Z (Unix 1795046077)");
        let stored = moment_unix(i64::MAX, at);
        assert!(stored.shown.contains("Unix time 9223372036854775807"));
        assert!(stored.datetime.is_empty());
        Ok(())
    }

    // Owner request 2026-11-20 (admin restyle b5): Settings shows time limits
    // as people read them, to the millisecond that was set.
    #[test]
    fn limits_read_in_seconds_or_their_largest_whole_unit() {
        for (millis, expected) in [
            (1_500, "1.5 s"),
            (1_750, "1.75 s"),
            (250, "0.25 s"),
            (10_000, "10 s"),
            (300_000, "5 min"),
        ] {
            assert_eq!(limit(std::time::Duration::from_millis(millis)), expected);
        }
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
        // Owner decision 2026-11-20 (PR c2, Sidebar mockup): "Live · 10:45:12".
        assert_eq!(live_clock_in(now, utc), "23:54:37");
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
