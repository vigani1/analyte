//! Turns technical values into words a lab manager reads at a glance; pages keep the exact value beside them.
//!
//! Sizes use binary units (1 GB = 1024³ bytes), the way Windows Explorer shows a
//! disk. Times show in the PC's own time zone; the exact UTC time and Unix
//! seconds stay in the hover text.
use crate::admin::markup::Markup;
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

/// How long ago `then` was, such as "3 min ago".
pub(in crate::admin) fn ago(then: Timestamp, now: Timestamp) -> String {
    match u64::try_from(now.as_second().saturating_sub(then.as_second())) {
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
        .text(format!("{at} (Unix {})", at.as_second()))
        .markup("\">")
        .text(time(at))
        .markup("</time> <span class=\"muted\">")
        .text(ago(at, now))
        .markup("</span>")
}

/// [`when`] for Unix seconds from the store; a value outside the calendar shows as is.
pub(in crate::admin) fn when_unix(seconds: i64, now: Timestamp) -> Markup {
    Timestamp::from_second(seconds).map_or_else(
        |_| Markup::new().text(format!("Unix time {seconds} (not a valid date)")),
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

    #[test]
    fn ages_read_as_ago_and_never_go_negative() -> Result<(), jiff::Error> {
        let now = Timestamp::from_second(1_795_046_077)?;
        assert_eq!(ago(now, now), "0 s ago");
        assert_eq!(ago(Timestamp::from_second(1_795_046_076)?, now), "1 s ago");
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
        assert!(html.contains("0 s ago"));
        assert!(when_unix(i64::MAX, at).contains("Unix time 9223372036854775807"));
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
