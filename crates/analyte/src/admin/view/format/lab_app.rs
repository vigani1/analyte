//! Says how a lab app is delivering: last delivery, waiting and failing results, its senders and its keys.
use super::{Line, amount, moment, moment_unix, time::not_a_date};
use analyte_store::FailingDestination;
use jiff::{Timestamp, tz::TimeZone};

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
        return Line::text(NOTHING_WAITING);
    }
    let items = amount(count, "item waiting", "items waiting");
    match oldest {
        Some(at) => Line::text(format!("{items} · oldest ")).time(moment(at, now)),
        None => Line::text(format!("{items} · oldest not recorded")),
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

/// When a stored key was set, as its status says: "Set on 19 Nov", with the
/// year when it is not this year's.
pub(in crate::admin) fn set_on(set_at: i64, now: Timestamp) -> String {
    set_on_in(set_at, now, &TimeZone::system())
}

fn set_on_in(set_at: i64, now: Timestamp, zone: &TimeZone) -> String {
    let Ok(at) = Timestamp::from_second(set_at) else {
        return format!("Set at {}", not_a_date(set_at));
    };
    let at = at.to_zoned(zone.clone());
    let pattern = if at.year() == now.to_zoned(zone.clone()).year() {
        "Set on %-d %b"
    } else {
        "Set on %-d %b %Y"
    };
    at.strftime(pattern).to_string()
}

/// A key whose value the service environment holds, which wins over a stored one.
pub(in crate::admin) fn set_in_environment(variable: &str) -> String {
    format!("Set in the service environment ({variable})")
}

/// Labels the lab app associated with the settings or data.
pub(in crate::admin) const LAB_APP: &str = "Lab app";

/// Says the lab app’s delivery queue is empty.
pub(in crate::admin) const NOTHING_WAITING: &str = "nothing waiting";

/// Labels the action to retry a delivery.
pub(in crate::admin) const SEND_AGAIN: &str = "Send again";

/// Labels a delivery or queue waiting to be sent.
pub(in crate::admin) const WAITING_TO_SEND: &str = "Waiting to be sent";

#[cfg(test)]
mod tests {
    use super::super::{KEY_NEEDED, KEYS_ADVICE, KEYS_APPLY, clock, machines_and_lab_apps};
    use super::*;

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
            "1 item waiting · oldest not recorded"
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

    // Owner decisions 2026-11-20 (admin restyle b3): a key is never shown, only
    // where it is found: the day it was stored (with the year when not this
    // year's), the service environment's variable, or why a field is needed.
    #[test]
    fn keys_read_as_where_they_are_found() -> Result<(), Box<dyn std::error::Error>> {
        let utc = &TimeZone::UTC;
        let now = Timestamp::from_second(1_795_046_077)?;
        assert_eq!(set_on_in(1_795_046_077, now, utc), "Set on 18 Nov");
        assert_eq!(set_on_in(1_763_510_400, now, utc), "Set on 19 Nov 2025");
        assert_eq!(
            set_on_in(i64::MAX, now, utc),
            format!("Set at Unix time {} (not a valid date)", i64::MAX)
        );
        assert_eq!(
            set_in_environment("ANALYTE_SECRET_MAIN_LAB_API"),
            "Set in the service environment (ANALYTE_SECRET_MAIN_LAB_API)"
        );
        assert_eq!(KEY_NEEDED, "Needed before Analyte can send results.");
        assert!(KEYS_ADVICE.ends_with(KEYS_APPLY));
        Ok(())
    }
}
