//! Writes Activity's lines: the day dividers, the time of each line and the sentence for each stored event.
use super::{
    Moment, actor, amount, attempt_look, exact_time, lab_app_href, machine_href, order_look,
    samples, settled_action, test_count,
    time::{clock_in, not_a_date},
};
use crate::admin::view::{Tone, icon::Icon};
use crate::operator::recovery;
use analyte_model::{DeliveryId, OrderState};
use analyte_store::{Activity, AttemptOutcome, Carries, RecentOutcome, Tries};
use jiff::{Timestamp, tz::TimeZone};

/// One Activity line in words: who or what it is about (in bold), the rest
/// of the sentence, its tone and icon with the word a screen reader says for
/// them, and where to look at it. Only what Analyte stored is said; no value,
/// test code or typed reason is.
pub(in crate::admin) struct Said {
    pub(in crate::admin) head: String,
    pub(in crate::admin) rest: String,
    pub(in crate::admin) tone: Tone,
    pub(in crate::admin) icon: Icon,
    pub(in crate::admin) word: &'static str,
    pub(in crate::admin) link: Option<(String, &'static str)>,
}

/// The day a feed's lines fall in: "Today · Thursday 19 November",
/// "Wednesday 18 November", or with its year before this year.
pub(in crate::admin) fn feed_day(at: Timestamp, now: Timestamp) -> String {
    feed_day_in(at, now, &TimeZone::system())
}

fn feed_day_in(at: Timestamp, now: Timestamp, zone: &TimeZone) -> String {
    let at = at.to_zoned(zone.clone());
    let now = now.to_zoned(zone.clone());
    if at.date() == now.date() {
        format!("Today · {}", at.strftime("%A %-d %B"))
    } else if at.year() == now.year() {
        at.strftime("%A %-d %B").to_string()
    } else {
        at.strftime("%A %-d %B %Y").to_string()
    }
}

/// "event" or "events", after a count of Activity lines.
pub(in crate::admin) const fn events(count: u64) -> &'static str {
    if count == 1 { "event" } else { "events" }
}

/// A feed line's time of day to the second, as the Activity mockup shows it;
/// its day is the divider above it, and the exact value stays on hover.
pub(in crate::admin) fn feed_time(seconds: i64) -> Moment {
    feed_time_in(seconds, &TimeZone::system())
}

fn feed_time_in(seconds: i64, zone: &TimeZone) -> Moment {
    Timestamp::from_second(seconds).map_or_else(
        |_| Moment {
            datetime: String::new(),
            shown: not_a_date(seconds),
            exact: String::new(),
        },
        |at| Moment {
            datetime: at.to_string(),
            shown: at.to_zoned(zone.clone()).strftime("%H:%M:%S").to_string(),
            exact: exact_time(at),
        },
    )
}

/// The divider above lines whose time was not stored.
pub(in crate::admin) const FEED_UNDATED: &str = "Time not recorded";

/// A machine as an Activity line names it: its ID, and its model when known.
#[derive(Clone, Copy)]
pub(in crate::admin) struct Named<'a> {
    pub(in crate::admin) id: &'a str,
    pub(in crate::admin) model: Option<&'a str>,
}

impl Named<'_> {
    /// "cobas-e411 (cobas e 411)", or the ID alone.
    fn label(self) -> String {
        self.model.map_or_else(
            || self.id.to_owned(),
            |model| format!("{} ({model})", self.id),
        )
    }
}

/// The head of a line about `machine`, or `other` when it names none, and
/// what goes before the rest of the line: the model in brackets, not bold.
fn headed(machine: Option<Named<'_>>, other: &str) -> (String, String) {
    machine.map_or_else(
        || (other.to_owned(), String::new()),
        |machine| {
            (
                machine.id.to_owned(),
                machine
                    .model
                    .map_or_else(String::new, |model| format!("({model}) ")),
            )
        },
    )
}

/// What `what` says, stored at `at`, about `machine` when it names one: the
/// machine's ID in bold, then its model ("cobas-e411 (cobas e 411)").
/// `headline` gives a Needs review item's title and tone, as its card shows
/// them.
pub(in crate::admin) fn said(
    what: &Activity,
    at: Option<i64>,
    machine: Option<Named<'_>>,
    headline: impl FnOnce(&analyte_store::Parked) -> (String, Tone),
) -> Said {
    let from = |rest: String| match machine {
        Some(machine) => format!("{rest} – from {}", machine.label()),
        None => rest,
    };
    match what {
        Activity::Stored(reading) => stored(reading.as_ref(), machine),
        Activity::Parked { item, details } => {
            let (title, tone) = headline(details);
            Said {
                head: crate::admin::view::Tab::NeedsReview.label().into(),
                rest: from(format!("– item {item} added: {}", lower_first(&title))),
                tone,
                icon: tone.icon(),
                word: crate::admin::view::Tab::NeedsReview.label(),
                link: Some((format!("/parked/{item}"), "Open")),
            }
        }
        Activity::Resolved { item, action } => Said {
            head: crate::admin::view::Tab::NeedsReview.label().into(),
            rest: from(format!(
                "– item {item} {}",
                lower_first(settled_action(action))
            )),
            tone: Tone::Good,
            icon: Icon::Ok,
            word: "Resolved",
            link: Some((format!("/parked/{item}"), "Open")),
        },
        Activity::Attempt {
            delivery,
            lab_app,
            outcome,
            carries,
            tries,
        } => {
            let what = match carries {
                Carries::Results(reading) => carried(reading.as_ref()),
                Carries::OrderMessage { order } => format!("– order {order} message "),
            };
            let (words, link) = match (tries, at) {
                (Some(tries), Some(last)) => (
                    tried(lab_app, *outcome, *tries, last, &TimeZone::system()),
                    Some((every_try_href(*delivery), "Every try")),
                ),
                (Some(_) | None, _) => attempt(lab_app, *outcome),
            };
            let look = attempt_look(*outcome);
            let (head, aside) = headed(machine, lab_app);
            Said {
                head,
                rest: format!("{aside}{what}– {words}"),
                tone: look.tone,
                icon: look.icon,
                word: look.word,
                link,
            }
        }
        Activity::Order {
            order,
            sample,
            tests,
            state,
        } => {
            let mut rest = format!("– order {order} ");
            if let Some(sample) = sample {
                rest.push_str(&format!("– sample {sample} "));
            }
            if let Some(tests) = tests {
                rest.push_str(&format!("– {} ", test_count(*tests)));
            }
            ordered(&rest, *state, machine)
        }
        Activity::Changed {
            actor,
            action,
            target,
        } => {
            let (words, link) = change(action, target.as_deref());
            Said {
                head: self::actor(actor),
                rest: format!("– {words}"),
                tone: Tone::Neutral,
                icon: Icon::Info,
                word: "Change",
                link,
            }
        }
    }
}

/// A stored message's line: what its latest reading produced, if it was read.
fn stored(reading: Option<&RecentOutcome>, machine: Option<Named<'_>>) -> Said {
    let (tone, icon, word, outcome) = match reading {
        Some(RecentOutcome::Results { .. }) => (Tone::Good, Icon::Ok, "Stored", "message stored"),
        Some(RecentOutcome::Failed) => (
            Tone::Attention,
            Icon::Warning,
            "Warning",
            "message stored, could not be read",
        ),
        None => (
            Tone::Neutral,
            Icon::Waiting,
            "Not read yet",
            "message stored, not read yet",
        ),
    };
    let (head, aside) = headed(machine, "A machine");
    Said {
        head,
        rest: format!("{aside}{}– {outcome}", carried(reading)),
        tone,
        icon,
        word,
        link: None,
    }
}

/// A machine order's line: what it is (`order`), then its new state; one
/// whose outcome is unknown or that was rejected links to Machine orders.
fn ordered(order: &str, state: OrderState, machine: Option<Named<'_>>) -> Said {
    let look = order_look(state);
    let (head, aside) = headed(machine, "A machine");
    Said {
        head,
        rest: format!("{aside}{order}– {}", lower_first(look.word)),
        tone: look.tone,
        icon: look.icon,
        word: "Order",
        link: matches!(state, OrderState::Uncertain | OrderState::Rejected)
            .then(|| (super::super::Tab::MachineOrders.href().to_owned(), "Orders")),
    }
}

/// The sample and result count a message or delivery carries, each followed
/// by a space: "– sample 261119-031 – 8 results ".
fn carried(reading: Option<&RecentOutcome>) -> String {
    match reading {
        Some(RecentOutcome::Results { results, samples }) => {
            let sample = self::samples(samples)
                .map_or_else(String::new, |(shown, _)| format!("– sample {shown} "));
            let count = if *results == 0 {
                "no results".into()
            } else {
                amount(
                    u64::try_from(*results).unwrap_or(u64::MAX),
                    "result",
                    "results",
                )
            };
            format!("{sample}– {count} ")
        }
        Some(RecentOutcome::Failed) | None => String::new(),
    }
}

/// A link from an Activity line: where it goes and its words.
type FeedLink = Option<(String, &'static str)>;

/// How one delivery try to `lab_app` ended, as an Activity line says it,
/// and where to look at it; [`attempt_look`] gives its tone, icon and word.
fn attempt(lab_app: &str, outcome: AttemptOutcome) -> (String, FeedLink) {
    let deliveries = || {
        Some((
            "/deliveries?show=undelivered".to_owned(),
            crate::admin::view::Tab::Deliveries.label(),
        ))
    };
    match outcome {
        AttemptOutcome::Delivered => (format!("delivered to {lab_app}"), None),
        AttemptOutcome::RetryAt(_) => (format!("sending to {lab_app} failed, will retry"), None),
        AttemptOutcome::Interrupted => (
            format!("sending to {lab_app} was cut off, will retry"),
            None,
        ),
        AttemptOutcome::Rejected => (format!("rejected by {lab_app}"), deliveries()),
        AttemptOutcome::Paused => (
            format!("sending to {lab_app} paused: lab app settings need repair"),
            Some((lab_app_href(lab_app), crate::admin::view::format::LAB_APP)),
        ),
        AttemptOutcome::Conflict => (format!("{lab_app} reports a conflict"), deliveries()),
    }
}

/// Failed tries in a row to `lab_app` as one line says them, the latest
/// stored at `last`, in `zone`: "sending to main-lab failed · tried 35 times,
/// 02:32 – 12:01 · next try after 12:02". Times carry their day when it is
/// not the latest try's, whose day is the divider above the line. The next
/// try is said only while the delivery still waits for it.
fn tried(
    lab_app: &str,
    outcome: AttemptOutcome,
    tries: Tries,
    last: i64,
    zone: &TimeZone,
) -> String {
    let at = |seconds: i64| match (
        Timestamp::from_second(seconds),
        Timestamp::from_second(last),
    ) {
        (Ok(at), Ok(last)) => clock_in(at, last, zone),
        (Err(_), _) | (_, Err(_)) => not_a_date(seconds),
    };
    let failed = match outcome {
        AttemptOutcome::Interrupted => "was cut off",
        AttemptOutcome::RetryAt(_)
        | AttemptOutcome::Delivered
        | AttemptOutcome::Rejected
        | AttemptOutcome::Paused
        | AttemptOutcome::Conflict => "failed",
    };
    let mut words = format!(
        "sending to {lab_app} {failed} · tried {} times, {} – {}",
        tries.count,
        at(tries.first),
        at(last)
    );
    if let (true, AttemptOutcome::RetryAt(next)) = (tries.waiting, outcome) {
        words.push_str(&format!(" · next try after {}", at(next)));
    }
    words
}

/// Activity listing every try of `delivery`, one line each.
fn every_try_href(delivery: DeliveryId) -> String {
    format!(
        "{}?delivery={}",
        super::super::Tab::Activity.href(),
        uuid::Uuid::from(delivery)
    )
}

/// What an audited change did, from its action and the first thing it names,
/// and the page about that thing; an action this version does not know reads
/// as recorded.
fn change(action: &str, target: Option<&str>) -> (String, FeedLink) {
    let named = target.unwrap_or("(not recorded)");
    let machine = || target.map(|id| (machine_href(id), crate::admin::view::format::MACHINE));
    let lab_app = || target.map(|name| (lab_app_href(name), crate::admin::view::format::LAB_APP));
    let page = |tab: super::super::Tab| Some((tab.href().to_owned(), tab.label()));
    match action {
        "setup-add" => (format!("added machine {named}"), machine()),
        "setup-save" => (format!("saved the settings of {named}"), machine()),
        "setup-remove" => (format!("removed machine {named}"), None),
        "settings-checked-for-software-version" => (
            format!("confirmed the settings of {named} for its reported software"),
            machine(),
        ),
        "setup-restart" => ("asked Analyte to restart".into(), None),
        "lab-app-add" => (format!("added lab app {named}"), lab_app()),
        "lab-app-save" => (format!("saved lab app {named}"), lab_app()),
        "lab-app-remove" => (format!("removed lab app {named}"), None),
        "retention-hold-place" => (
            "placed a hold under Data keeping".into(),
            page(super::super::Tab::DataKeeping),
        ),
        "retention-hold-release" => (
            "released a hold under Data keeping".into(),
            page(super::super::Tab::DataKeeping),
        ),
        "retention-purge" => ("removed data past its keeping time".into(), None),
        "delivery-resend" => (
            "queued a delivery to send again".into(),
            page(super::super::Tab::Deliveries),
        ),
        recovery::REPROCESS_ACTION => ("processed a message again".into(), None),
        "recover-interrupted-delivery" => ("recovered an interrupted delivery".into(), None),
        "store-restore" => ("restored the store from a backup".into(), None),
        "orders-reconcile" => ("settled an order’s outcome".into(), None),
        "alert-retry" => ("retried an alert".into(), None),
        _ => (format!("recorded {action}"), None),
    }
}

/// `words` with a lowercase first letter, for the middle of a sentence,
/// unless it starts with a short form such as "LAW".
fn lower_first(words: &str) -> String {
    let mut chars = words.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if !second.is_uppercase() => {
            let rest = words.get(first.len_utf8()..).unwrap_or_default();
            format!("{}{rest}", first.to_lowercase())
        }
        _ => words.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Owner decision 2026-11-20 (PR c, Activity mockup): the feed's day
    // dividers say Today, then the weekday and date, with the year only before
    // this year; words mid-sentence start in lowercase unless they are a short form.
    #[test]
    fn feed_days_and_words_read_as_the_mockup() -> Result<(), jiff::Error> {
        let now = Timestamp::from_second(1_795_046_077)?; // 2026-11-18 23:54:37 UTC
        let utc = &TimeZone::UTC;
        assert_eq!(feed_day_in(now, now, utc), "Today · Wednesday 18 November");
        assert_eq!(
            feed_day_in(Timestamp::from_second(1_794_959_999)?, now, utc),
            "Tuesday 17 November"
        );
        assert_eq!(
            feed_day_in(Timestamp::UNIX_EPOCH, now, utc),
            "Thursday 1 January 1970"
        );
        assert_eq!(
            lower_first("Test or unit not mapped"),
            "test or unit not mapped"
        );
        assert_eq!(lower_first("LAW cancel refused"), "LAW cancel refused");
        assert_eq!(feed_time_in(1_795_046_077, utc).shown, "23:54:37");
        assert_eq!(
            feed_time_in(1_795_046_077, utc).exact,
            "2026-11-18T23:54:37Z (Unix 1795046077)"
        );
        assert_eq!(
            (events(1), events(0), events(194)),
            ("event", "events", "events")
        );
        Ok(())
    }

    // Owner decision 2026-11-21 (Activity groups retries): failed tries in a
    // row read as one line with how many, from when to when, and the next try
    // while the delivery waits; a time on another day than the latest try
    // carries its day.
    #[test]
    fn failed_tries_in_a_row_read_as_one_sentence() {
        let utc = &TimeZone::UTC;
        let last = 1_795_046_077; // 2026-11-18 23:54:37 UTC
        let tries = Tries {
            count: 35,
            first: last - 3_600 * 5,
            waiting: true,
        };
        assert_eq!(
            tried(
                "main-lab",
                AttemptOutcome::RetryAt(last + 60),
                tries,
                last,
                utc
            ),
            "sending to main-lab failed · tried 35 times, 18:54 – 23:54 · next try after 23:55"
        );
        assert_eq!(
            tried(
                "main-lab",
                AttemptOutcome::RetryAt(last + 600),
                Tries {
                    count: 2,
                    first: last - 86_400,
                    waiting: false,
                },
                last,
                utc
            ),
            "sending to main-lab failed · tried 2 times, 17 Nov 23:54 – 23:54",
            "a run that ended says no next try"
        );
        assert_eq!(
            tried("lab-2", AttemptOutcome::Interrupted, tries, last, utc),
            "sending to lab-2 was cut off · tried 35 times, 18:54 – 23:54"
        );
    }
}
