//! Writes Activity's lines: the day dividers, the time of each line and the sentence for each stored event.
use super::{
    Moment, actor, attempt_look, exact_time, lab_app_href, machine_href, order_in_line, order_look,
    samples, settled_words, test_count,
    time::{clock_in, not_a_date},
};
use crate::admin::view::{Tone, icon::Icon};
use crate::operator::recovery;
use analyte_i18n::{count as tr_count, message as tr_args, text as tr};
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
    let weekday = analyte_i18n::weekday(at.weekday().to_monday_one_offset());
    let day = at.day().to_string();
    let month = analyte_i18n::month(at.month());
    if at.date() == now.date() {
        tr_args(
            "admin-feed-day-today",
            [
                ("weekday", weekday.into()),
                ("day", day.into()),
                ("month", month.into()),
            ],
        )
    } else if at.year() == now.year() {
        tr_args(
            "admin-feed-day",
            [
                ("weekday", weekday.into()),
                ("day", day.into()),
                ("month", month.into()),
            ],
        )
    } else {
        tr_args(
            "admin-feed-day-year",
            [
                ("weekday", weekday.into()),
                ("day", day.into()),
                ("month", month.into()),
                ("year", at.year().to_string().into()),
            ],
        )
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
pub(in crate::admin) fn feed_undated() -> &'static str {
    tr("admin-feed-undated")
}

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
/// `headline` gives a Needs review item's title, written for the middle of
/// the line, and its tone, as its card shows them.
pub(in crate::admin) fn said(
    what: &Activity,
    at: Option<i64>,
    machine: Option<Named<'_>>,
    headline: impl FnOnce(&analyte_store::Parked) -> (String, Tone),
) -> Said {
    let from = |rest: String| match machine {
        Some(machine) => tr_args(
            "admin-activity-from",
            [("rest", rest.into()), ("machine", machine.label().into())],
        ),
        None => rest,
    };
    match what {
        Activity::Stored(reading) => stored(reading.as_ref(), machine),
        Activity::Parked { item, details } => {
            let (title, tone) = headline(details);
            parked_line(*item, &title, tone, from)
        }
        Activity::Resolved { item, action } => Said {
            head: crate::admin::view::Tab::NeedsReview.label().into(),
            rest: from(tr_args(
                "admin-item-3",
                [
                    ("item", (item).to_string().into()),
                    ("action", settled_words(action).in_line.into()),
                ],
            )),
            tone: Tone::Good,
            icon: Icon::Ok,
            word: tr("admin-resolved"),
            link: Some((format!("/parked/{item}"), tr("admin-open"))),
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
                Carries::OrderMessage { order } => tr_args(
                    "admin-order-message-2",
                    [("order", (order).to_string().into())],
                ),
            };
            let (words, link) = match (tries, at) {
                (Some(tries), Some(last)) => (
                    tried(lab_app, *outcome, *tries, last, &TimeZone::system()),
                    Some((every_try_href(*delivery), tr("admin-every-try"))),
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
            let mut rest = tr_args("admin-order-3", [("order", (order).to_string().into())]);
            if let Some(sample) = sample {
                rest.push_str(&tr_args(
                    "admin-sample-2",
                    [("sample", (sample).to_string().into())],
                ));
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
                word: tr("admin-change"),
                link,
            }
        }
    }
}

fn parked_line(item: i64, title: &str, tone: Tone, from: impl FnOnce(String) -> String) -> Said {
    let review = crate::admin::view::Tab::NeedsReview.label();
    Said {
        head: review.into(),
        rest: from(tr_args(
            "admin-item-added",
            [
                ("item", item.to_string().into()),
                ("title", title.to_owned().into()),
            ],
        )),
        tone,
        icon: tone.icon(),
        word: review,
        link: Some((format!("/parked/{item}"), tr("admin-open"))),
    }
}

/// A stored message's line: what its latest reading produced, if it was read.
fn stored(reading: Option<&RecentOutcome>, machine: Option<Named<'_>>) -> Said {
    let (tone, icon, word, outcome) = match reading {
        Some(RecentOutcome::Results { .. }) => (
            Tone::Good,
            Icon::Ok,
            tr("admin-stored"),
            tr("admin-message-stored"),
        ),
        Some(RecentOutcome::Failed) => (
            Tone::Attention,
            Icon::Warning,
            tr("admin-warning"),
            tr("admin-message-stored-could-not-be-read"),
        ),
        None => (
            Tone::Neutral,
            Icon::Waiting,
            tr("admin-not-read-yet"),
            tr("admin-message-stored-not-read-yet"),
        ),
    };
    let (head, aside) = headed(machine, tr("admin-activity-a-machine"));
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
    let (head, aside) = headed(machine, tr("admin-activity-a-machine"));
    Said {
        head,
        rest: format!("{aside}{order}– {}", order_in_line(state)),
        tone: look.tone,
        icon: look.icon,
        word: tr("admin-order"),
        link: matches!(state, OrderState::Uncertain | OrderState::Rejected).then(|| {
            (
                super::super::Tab::MachineOrders.href().to_owned(),
                tr("admin-orders"),
            )
        }),
    }
}

/// The sample and result count a message or delivery carries, each followed
/// by a space: "– sample 261119-031 – 8 results ".
fn carried(reading: Option<&RecentOutcome>) -> String {
    match reading {
        Some(RecentOutcome::Results { results, samples }) => {
            let sample = self::samples(samples).map_or_else(String::new, |(shown, _)| {
                tr_args("admin-sample-3", [("shown", (shown).to_string().into())])
            });
            let count = if *results == 0 {
                tr("admin-no-results-2").into()
            } else {
                tr_count(
                    "admin-count-results",
                    u64::try_from(*results).unwrap_or(u64::MAX),
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
        AttemptOutcome::Delivered => (
            tr_args(
                "admin-delivered-to",
                [("lab-app", (lab_app).to_string().into())],
            ),
            None,
        ),
        AttemptOutcome::RetryAt(_) => (
            tr_args(
                "admin-sending-to-failed-will-retry",
                [("lab-app", (lab_app).to_string().into())],
            ),
            None,
        ),
        AttemptOutcome::Interrupted => (
            tr_args(
                "admin-sending-to-was-cut-off-will-retry",
                [("lab-app", (lab_app).to_string().into())],
            ),
            None,
        ),
        AttemptOutcome::Rejected => (
            tr_args(
                "admin-rejected-by",
                [("lab-app", (lab_app).to_string().into())],
            ),
            deliveries(),
        ),
        AttemptOutcome::Paused => (
            tr_args(
                "admin-sending-to-paused-lab-app-settings-need-repair",
                [("lab-app", (lab_app).to_string().into())],
            ),
            Some((lab_app_href(lab_app), crate::admin::view::format::lab_app())),
        ),
        AttemptOutcome::Conflict => (
            tr_args(
                "admin-reports-a-conflict",
                [("lab-app", (lab_app).to_string().into())],
            ),
            deliveries(),
        ),
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
    // A fixed key: the catalog chooses each language's words for it.
    let outcome_key = match outcome {
        AttemptOutcome::Interrupted => "cut-off",
        AttemptOutcome::RetryAt(_)
        | AttemptOutcome::Delivered
        | AttemptOutcome::Rejected
        | AttemptOutcome::Paused
        | AttemptOutcome::Conflict => "failed",
    };
    let mut words = tr_args(
        "admin-sending-to-tried-times",
        [
            ("lab-app", (lab_app).to_string().into()),
            ("outcome", outcome_key.into()),
            ("count", (tries.count).to_string().into()),
            ("first", (at(tries.first)).to_string().into()),
            ("last", (at(last)).to_string().into()),
        ],
    );
    if let (true, AttemptOutcome::RetryAt(next)) = (tries.waiting, outcome) {
        words.push_str(&tr_args(
            "admin-next-try-after",
            [("next", (at(next)).to_string().into())],
        ));
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
    let named = target.unwrap_or(tr("admin-not-recorded-2"));
    let machine = || target.map(|id| (machine_href(id), crate::admin::view::format::machine()));
    let lab_app = || target.map(|name| (lab_app_href(name), crate::admin::view::format::lab_app()));
    let page = |tab: super::super::Tab| Some((tab.href().to_owned(), tab.label()));
    match action {
        "setup-add" => (
            tr_args(
                "admin-added-machine",
                [("named", (named).to_string().into())],
            ),
            machine(),
        ),
        "setup-save" => (
            tr_args(
                "admin-saved-the-settings-of",
                [("named", (named).to_string().into())],
            ),
            machine(),
        ),
        "setup-remove" => (
            tr_args(
                "admin-removed-machine-2",
                [("named", (named).to_string().into())],
            ),
            None,
        ),
        "settings-checked-for-software-version" => (
            tr_args(
                "admin-confirmed-the-settings-of-for-its-reported-software",
                [("named", (named).to_string().into())],
            ),
            machine(),
        ),
        "setup-restart" => (tr("admin-asked-analyte-to-restart").into(), None),
        "lab-app-add" => (
            tr_args(
                "admin-added-lab-app",
                [("named", (named).to_string().into())],
            ),
            lab_app(),
        ),
        "lab-app-save" => (
            tr_args(
                "admin-saved-lab-app",
                [("named", (named).to_string().into())],
            ),
            lab_app(),
        ),
        "lab-app-remove" => (
            tr_args(
                "admin-removed-lab-app",
                [("named", (named).to_string().into())],
            ),
            None,
        ),
        "retention-hold-place" => (
            tr("admin-placed-a-hold-under-data-keeping").into(),
            page(super::super::Tab::DataKeeping),
        ),
        "retention-hold-release" => (
            tr("admin-released-a-hold-under-data-keeping").into(),
            page(super::super::Tab::DataKeeping),
        ),
        "retention-purge" => (tr("admin-removed-data-past-its-keeping-time").into(), None),
        "delivery-resend" => (
            tr("admin-queued-a-delivery-to-send-again").into(),
            page(super::super::Tab::Deliveries),
        ),
        recovery::REPROCESS_ACTION => (tr("admin-processed-a-message-again").into(), None),
        "recover-interrupted-delivery" => {
            (tr("admin-recovered-an-interrupted-delivery").into(), None)
        }
        "store-restore" => (tr("admin-restored-the-store-from-a-backup").into(), None),
        "orders-reconcile" => (tr("admin-settled-an-order-s-outcome").into(), None),
        "alert-retry" => (tr("admin-retried-an-alert").into(), None),
        _ => (
            tr_args(
                "admin-recorded-3",
                [("action", (action).to_string().into())],
            ),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_i18n::Language;

    // ADR 0039: a cut-off or failed send is a fixed key the catalog words;
    // the English "failed" once reached every language.
    #[test]
    fn a_failed_send_is_worded_by_each_catalog() {
        let line = |outcome: &str| {
            tr_args(
                "admin-sending-to-tried-times",
                [
                    ("lab-app", "main-lab".into()),
                    ("outcome", outcome.into()),
                    ("count", "3".into()),
                    ("first", "09:00".into()),
                    ("last", "09:05".into()),
                ],
            )
        };
        assert_eq!(
            line("failed"),
            "sending to main-lab failed · tried 3 times, 09:00 – 09:05"
        );
        assert_eq!(
            line("cut-off"),
            "sending to main-lab was cut off · tried 3 times, 09:00 – 09:05"
        );
        for language in Language::installed().filter(|language| language.tag() != "en") {
            let (failed, cut) = language.sync_scope(|| (line("failed"), line("cut-off")));
            assert_ne!(failed, cut, "{}", language.tag());
            assert!(!failed.contains("failed"), "{}: {failed}", language.tag());
            assert!(!cut.contains("cut-off"), "{}: {cut}", language.tag());
        }
    }

    // ADR 0039 (owner request 2026-11-27): a label inside an Activity line is
    // its own message in each language. English lowers it as it always did;
    // German keeps its nouns capitalised.
    #[test]
    fn labels_inside_activity_lines_keep_each_language_s_capitals()
    -> Result<(), analyte_model::TextError> {
        let item = analyte_store::Parked {
            kind: analyte_model::Identifier::new("mapping_finding")?,
            reason: analyte_model::Reason::new("Position { record: 3, field: 3 }: MissingTest")?,
            targets: Vec::new(),
        };
        let lines = || {
            let (title, tone) = crate::admin::parked::headline(&item);
            (
                parked_line(5, &title, tone, |rest| rest).rest,
                ordered("", OrderState::CancelRequested, None).rest,
                settled_words(analyte_store::RERUN_MEASUREMENT_ACTION).in_line,
            )
        };
        let (added, order, settled) = lines();
        assert_eq!(added, "– item 5 added: test code missing");
        assert!(order.ends_with("– cancel requested"), "{order}");
        assert_eq!(settled, "rerun queued");
        let german = Language::negotiate(["de"]).expect("German is installed");
        let (added, order, settled) = german.sync_scope(lines);
        assert_eq!(added, "– Eintrag 5 hinzugefügt: Testcode fehlt");
        assert!(order.ends_with("– Stornierung angefordert"), "{order}");
        assert_eq!(settled, "Wiederholungsmessung eingereiht");
        Ok(())
    }

    // Owner decision 2026-11-20 (PR c, Activity mockup): the feed's day
    // dividers say Today, then the weekday and date, with the year only before
    // this year.
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
