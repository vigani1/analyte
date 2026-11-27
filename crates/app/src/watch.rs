//! Decides what the tray shows from each answer of the engine's `/status`, when an answer earns a notification and what it says, and what the title bar keeps of the last one.
//!
//! The engine counts as off only after [`MISSES_FOR_OFF`] questions in a row
//! went unanswered, so a restart from the setup page does not flash "off".
//! A notification is sent when the state gets worse (good, then attention,
//! then problem, then off), and when a kind of thing the app has not told of
//! appears while the state is not good (ADR 0023); never for the first state
//! the app sees, never when things get better. A kind is told once until the
//! state is good again, so a kind that comes and goes does not repeat. The
//! words name the most serious kind and what to do; they say nothing about
//! patients or results, and no name or count, as `/status` has none.
use crate::engine::status::{Health, Reason, Reasons, Reply};
use analyte_i18n::{message as tr_args, text as tr};
use jiff::{SignedDuration, Timestamp, Zoned, tz::TimeZone};

/// How many unanswered questions in a row make the engine count as off once
/// it was seen running; before the first answer one is enough.
pub const MISSES_FOR_OFF: u32 = 2;

/// How long the title bar keeps the last notification's line once what it
/// told of is gone.
pub const RECAP_KEPT: SignedDuration = SignedDuration::from_hours(24);

/// What the app shows about the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// The app has not asked yet.
    Checking,
    /// The engine answered with this state.
    Running(Health),
    /// The engine did not answer.
    Off,
}

/// The tray icon for a state: three states of a running engine, plus off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tray {
    /// A green round badge with a tick.
    Good,
    /// An amber triangle with "!".
    Attention,
    /// A red octagon with "×".
    Problem,
    /// A faded mark with a grey pause badge.
    Off,
}

impl Tray {
    /// The icon's name in `crates/app/icons/tray-<name>-<size>.png`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Attention => "attention",
            Self::Problem => "problem",
            Self::Off => "off",
        }
    }
}

impl Shown {
    /// The tray icon. An engine that cannot read its state shows the problem
    /// icon: nobody can tell it is fine.
    #[must_use]
    pub const fn tray(self) -> Tray {
        match self {
            Self::Running(Health::Good) => Tray::Good,
            Self::Running(Health::Attention) => Tray::Attention,
            Self::Running(Health::Problem | Health::Unknown) => Tray::Problem,
            Self::Checking | Self::Off => Tray::Off,
        }
    }

    /// The tray's state line and tooltip, in plain words.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::Checking => tr("desktop-checking-the-engine"),
            Self::Running(health) => health.words(),
            Self::Off => tr("desktop-the-engine-isn-t-answering"),
        }
    }

    /// How bad the state is, for "did it get worse"; none before the first answer.
    const fn rank(self) -> Option<u8> {
        match self {
            Self::Checking => None,
            Self::Running(Health::Good) => Some(0),
            Self::Running(Health::Attention) => Some(1),
            Self::Running(Health::Problem | Health::Unknown) => Some(2),
            Self::Off => Some(3),
        }
    }
}

/// A change of what the app shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    /// What was shown before.
    pub from: Shown,
    /// What is shown now.
    pub to: Shown,
}

/// What one notification told of, to tell later whether it is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum About {
    /// One kind of thing that needs someone.
    Reason(Reason),
    /// A state the engine gave without a kind the app knows, or `unknown`.
    State(Health),
    /// The engine stopped answering.
    Off,
}

impl About {
    /// Whether what this told of is gone while `shown` with `reasons`: none
    /// when the app cannot tell (the engine is off or cannot read its state).
    fn gone(self, shown: Shown, reasons: Reasons) -> Option<bool> {
        match (self, shown) {
            (Self::Off, Shown::Off) => Some(false),
            (Self::Off, Shown::Running(_)) => Some(true),
            (Self::State(Health::Unknown), Shown::Running(health)) => {
                Some(health != Health::Unknown)
            }
            (Self::Reason(reason), Shown::Running(health)) if health != Health::Unknown => {
                Some(!reasons.contains(reason))
            }
            (Self::State(was), Shown::Running(health)) if health != Health::Unknown => {
                Some(Shown::Running(health).rank() < Shown::Running(was).rank())
            }
            // Off, not asked yet, or running without reading its state.
            (Self::Reason(_) | Self::State(_) | Self::Off, Shown::Checking)
            | (Self::Reason(_) | Self::State(_), Shown::Off | Shown::Running(_)) => None,
        }
    }
}

/// A notification's words: what happened, and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// What happened, as a short sentence.
    pub title: &'static str,
    /// What to do, and how many other things need a look.
    pub body: String,
    about: About,
}

impl Notice {
    /// The notice for `shown` with `reasons`, headed by the most serious of
    /// the kinds `new` to the app, or of all listed when none is new.
    fn of(shown: Shown, reasons: Reasons, new: Reasons) -> Self {
        match shown {
            Shown::Off => Self {
                title: shown.words(),
                body: tr("desktop-open-analyte-to-see-what-the-app-saw").into(),
                about: About::Off,
            },
            Shown::Running(Health::Unknown) => Self {
                title: shown.words(),
                body: tr("desktop-it-is-running-but-could-not-say-how-things").into(),
                about: About::State(Health::Unknown),
            },
            Shown::Checking
            | Shown::Running(Health::Good | Health::Attention | Health::Problem) => {
                match new.first().or_else(|| reasons.first()) {
                    Some(reason) => Self {
                        title: reason.title(),
                        body: tr_args(
                            "desktop-notification-body",
                            [
                                ("advice", reason.advice().into()),
                                (
                                    "others",
                                    reasons.without(Reasons::NONE.with(reason)).len().into(),
                                ),
                            ],
                        ),
                        about: About::Reason(reason),
                    },
                    None => Self {
                        title: shown.words(),
                        body: tr("desktop-open-analyte-to-see-what").into(),
                        about: About::State(match shown {
                            Shown::Running(health) => health,
                            Shown::Checking | Shown::Off => Health::Unknown,
                        }),
                    },
                }
            }
        }
    }
}

/// The last notification, kept for the title bar: what it said, when, and
/// since when what it told of is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recap {
    /// The notification's title.
    pub title: &'static str,
    /// When it was sent.
    pub sent: Timestamp,
    /// When the app first saw what it told of gone, while it stays gone.
    pub fixed: Option<Timestamp>,
    about: About,
}

impl Recap {
    /// The title bar's line at `now` on `zone`'s clock: the notification's
    /// time and title, and since when it is fixed; none once it has been
    /// fixed for longer than [`RECAP_KEPT`].
    #[must_use]
    pub fn line(&self, now: Timestamp, zone: &TimeZone) -> Option<String> {
        let sent = tr_args(
            "desktop-notified",
            [
                ("zone", (moment(self.sent, now, zone)).to_string().into()),
                ("title", (self.title).to_string().into()),
            ],
        );
        match self.fixed {
            None => Some(sent),
            Some(fixed) if now.duration_since(fixed) > RECAP_KEPT => None,
            Some(fixed) => Some(tr_args(
                "desktop-fixed-since",
                [
                    ("sent", (sent).to_string().into()),
                    ("zone", (moment(fixed, now, zone)).to_string().into()),
                ],
            )),
        }
    }
}

/// A time on this computer's clock: `14:32` today, `19 Nov 14:32` on
/// another day.
fn moment(at: Timestamp, now: Timestamp, zone: &TimeZone) -> String {
    let then = Zoned::new(at, zone.clone());
    if then.date() == Zoned::new(now, zone.clone()).date() {
        then.strftime("%H:%M").to_string()
    } else {
        then.strftime("%-d %b %H:%M").to_string()
    }
}

/// What one answer brought: the change of what is shown, and the
/// notification it earns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The change of what is shown, if any.
    pub change: Option<Change>,
    /// The notification to send, if any.
    pub notice: Option<Notice>,
}

/// What the app has seen of the engine so far.
#[derive(Clone, Debug)]
pub struct Watch {
    shown: Shown,
    /// The kinds the last answer listed; none while the engine is off.
    reasons: Reasons,
    /// The kinds already told of, or listed in the first answer, since the
    /// state was last good; none before the first answer.
    told: Option<Reasons>,
    /// Unanswered questions since the last answer.
    misses: u32,
    /// When the engine last answered, and with which state.
    last_answer: Option<(Timestamp, Health)>,
    /// The last question's reply.
    last_reply: Option<Reply>,
    /// The last notification.
    recap: Option<Recap>,
}

impl Default for Watch {
    fn default() -> Self {
        Self::new()
    }
}

impl Watch {
    /// Nothing seen yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            shown: Shown::Checking,
            reasons: Reasons::NONE,
            told: None,
            misses: 0,
            last_answer: None,
            last_reply: None,
            recap: None,
        }
    }

    /// What the app shows now.
    #[must_use]
    pub const fn shown(&self) -> Shown {
        self.shown
    }

    /// When the engine last answered, and the state it gave then.
    #[must_use]
    pub const fn last_answer(&self) -> Option<(Timestamp, Health)> {
        self.last_answer
    }

    /// The last question's reply, for the Engine off screen.
    #[must_use]
    pub const fn last_reply(&self) -> Option<&Reply> {
        self.last_reply.as_ref()
    }

    /// The last notification, for the title bar.
    #[must_use]
    pub const fn recap(&self) -> Option<Recap> {
        self.recap
    }

    /// Takes one reply received at `now`; gives the change of what is shown
    /// and the notification it earns, if any.
    pub fn observe(&mut self, reply: Reply, now: Timestamp) -> Seen {
        let (next, reasons) = match &reply {
            Reply::Answered(health, reasons) => {
                self.misses = 0;
                self.last_answer = Some((now, *health));
                (Shown::Running(*health), *reasons)
            }
            Reply::Unrecognised(_) | Reply::Silent(_) => {
                self.misses = self.misses.saturating_add(1);
                match self.shown {
                    Shown::Running(_) if self.misses < MISSES_FOR_OFF => (self.shown, self.reasons),
                    Shown::Checking | Shown::Running(_) | Shown::Off => (Shown::Off, Reasons::NONE),
                }
            }
        };
        self.last_reply = Some(reply);
        let from = std::mem::replace(&mut self.shown, next);
        self.reasons = reasons;
        let new = self.learn();
        let worse = matches!((from.rank(), next.rank()), (Some(was), Some(is)) if is > was);
        let notice = (worse || !new.is_empty()).then(|| Notice::of(next, reasons, new));
        self.follow_recap(notice.as_ref(), now);
        Seen {
            change: (from != next).then_some(Change { from, to: next }),
            notice,
        }
    }

    /// Adds the kinds now listed to those told of; gives those that are news.
    /// A good state forgets them all; the first answer is no news.
    fn learn(&mut self) -> Reasons {
        match self.shown {
            Shown::Running(Health::Good) => {
                self.told = Some(Reasons::NONE);
                Reasons::NONE
            }
            Shown::Running(Health::Attention | Health::Problem) => {
                let new = self
                    .told
                    .map_or(Reasons::NONE, |told| self.reasons.without(told));
                self.told = Some(self.told.unwrap_or_default().and(self.reasons));
                new
            }
            Shown::Running(Health::Unknown) | Shown::Off | Shown::Checking => Reasons::NONE,
        }
    }

    /// Keeps `notice` as the last one, or notes whether what the last one
    /// told of is gone now.
    fn follow_recap(&mut self, notice: Option<&Notice>, now: Timestamp) {
        if let Some(notice) = notice {
            self.recap = Some(Recap {
                title: notice.title,
                sent: now,
                fixed: None,
                about: notice.about,
            });
        } else if let Some(recap) = &mut self.recap {
            match recap.about.gone(self.shown, self.reasons) {
                Some(true) => {
                    recap.fixed.get_or_insert(now);
                }
                Some(false) => recap.fixed = None,
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::status::Silence;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).expect("a valid test time")
    }

    fn silent() -> Reply {
        Reply::Silent(Silence::Refused)
    }

    fn answered(health: Health) -> Reply {
        Reply::Answered(health, Reasons::NONE)
    }

    fn with(health: Health, reasons: &[Reason]) -> Reply {
        Reply::Answered(health, Reasons::of(reasons))
    }

    /// Feeds replies one second apart; gives what each brought.
    fn run(watch: &mut Watch, replies: Vec<Reply>) -> Vec<Seen> {
        replies
            .into_iter()
            .zip(1..)
            .map(|(reply, second)| watch.observe(reply, at(second)))
            .collect()
    }

    fn titles(seen: &[Seen]) -> Vec<Option<&'static str>> {
        seen.iter()
            .map(|seen| seen.notice.as_ref().map(|notice| notice.title))
            .collect()
    }

    // ADR 0019 (tray states): the Overview's word becomes the tray icon; an
    // engine that cannot read its state is never shown as good.
    #[test]
    fn status_words_map_to_tray_icons() {
        assert_eq!(Shown::Running(Health::Good).tray(), Tray::Good);
        assert_eq!(Shown::Running(Health::Attention).tray(), Tray::Attention);
        assert_eq!(Shown::Running(Health::Problem).tray(), Tray::Problem);
        assert_eq!(Shown::Running(Health::Unknown).tray(), Tray::Problem);
        assert_eq!(Shown::Off.tray(), Tray::Off);
        assert_eq!(Shown::Checking.tray(), Tray::Off);
    }

    // ADR 0019 (notifications): only a worse state notifies, once per change:
    // good, attention, problem, off; never the first state, never a better
    // one. Without a kind the app knows, the title is the state.
    #[test]
    fn only_a_worse_state_notifies() {
        let mut watch = Watch::new();
        let seen = run(
            &mut watch,
            vec![
                answered(Health::Attention),
                answered(Health::Attention),
                answered(Health::Problem),
                answered(Health::Unknown),
                answered(Health::Good),
                answered(Health::Attention),
            ],
        );
        assert_eq!(
            titles(&seen),
            [
                None, // the first state seen
                None, // no change
                Some("A problem needs you"),
                None, // as bad as before
                None, // better
                Some("Something needs you"),
            ]
        );
        assert_eq!(seen.iter().filter(|seen| seen.change.is_some()).count(), 5);
        let last = seen.last().and_then(|seen| seen.notice.as_ref());
        assert_eq!(
            last.map(|notice| notice.body.as_str()),
            Some("Open Analyte to see what.")
        );
    }

    // ADR 0023: a worse state names the most serious kind that is new, says
    // what to do, and how many other things need a look.
    #[test]
    fn the_notice_names_the_most_serious_new_kind_and_what_to_do() {
        let mut watch = Watch::new();
        let seen = run(
            &mut watch,
            vec![
                with(Health::Attention, &[Reason::NeedsReview]),
                with(
                    Health::Problem,
                    &[
                        Reason::NeedsReview,
                        Reason::SendingFailing,
                        Reason::MachineLinkOff,
                    ],
                ),
            ],
        );
        assert_eq!(seen.first().and_then(|seen| seen.notice.clone()), None);
        let notice = seen
            .get(1)
            .and_then(|seen| seen.notice.clone())
            .expect("a worse state notifies");
        assert_eq!(notice.title, "Sending results to a lab app keeps failing");
        assert_eq!(
            notice.body,
            "Open Analyte to see which lab app and what to check. 2 more things need a look too."
        );
        let mut watch = Watch::new();
        let seen = run(
            &mut watch,
            vec![
                answered(Health::Good),
                with(Health::Problem, &[Reason::MachineLinkOff]),
            ],
        );
        let notice = seen
            .get(1)
            .and_then(|seen| seen.notice.clone())
            .expect("a worse state notifies");
        assert_eq!(notice.title, "A machine’s connection is off");
        assert_eq!(
            notice.body,
            "Open Analyte to see which machine and what to check."
        );
    }

    // ADR 0023: a new kind while the state is not good notifies once, even
    // when the state stays as bad; a kind that comes and goes is not told
    // again until the state is good; the first answer's kinds are no news.
    #[test]
    fn a_new_kind_notifies_once_until_all_is_good() {
        let link = Reason::MachineLinkOff;
        let disk = Reason::DiskFull;
        let review = Reason::NeedsReview;
        let mut watch = Watch::new();
        let seen = run(
            &mut watch,
            vec![
                with(Health::Problem, &[link]),               // first answer
                with(Health::Problem, &[link, disk]),         // disk is new
                with(Health::Problem, &[link]),               // disk goes
                with(Health::Problem, &[link, disk]),         // and comes back
                with(Health::Problem, &[link, disk, review]), // review is new
                with(Health::Attention, &[review]),           // better
                answered(Health::Good),
                with(Health::Problem, &[link]), // told again after good
            ],
        );
        assert_eq!(
            titles(&seen),
            [
                None,
                Some("The disk is full: Analyte is refusing new data"),
                None,
                None,
                Some("Something needs review"),
                None,
                None,
                Some("A machine’s connection is off"),
            ]
        );
        let review_notice = seen.get(4).and_then(|seen| seen.notice.clone());
        assert_eq!(
            review_notice.map(|notice| notice.body),
            Some("Open Analyte and look at Needs review. 2 more things need a look too.".into())
        );
    }

    // ADR 0023: kinds known before the engine went off are not news when it
    // answers again; a kind that appeared meanwhile is. Starting with the
    // engine off, its first answer is no news.
    #[test]
    fn coming_back_tells_only_kinds_that_are_new() {
        let link = Reason::MachineLinkOff;
        let mut watch = Watch::new();
        let seen = run(
            &mut watch,
            vec![
                answered(Health::Good),
                with(Health::Problem, &[link]),
                silent(),
                silent(),
                with(Health::Problem, &[link]),
                silent(),
                silent(),
                with(Health::Problem, &[link, Reason::SendingPaused]),
            ],
        );
        assert_eq!(
            titles(&seen),
            [
                None,
                Some("A machine’s connection is off"),
                None,
                Some("The engine isn’t answering"),
                None,
                None,
                Some("The engine isn’t answering"),
                Some("Sending to a lab app is paused"),
            ]
        );
        let mut watch = Watch::new();
        let seen = run(&mut watch, vec![silent(), with(Health::Problem, &[link])]);
        assert_eq!(titles(&seen), [None, None]);
    }

    // ADR 0019 (Engine off): a running engine counts as off after two
    // unanswered questions in a row, which notifies once; it is back with the
    // first answer, and coming back never notifies.
    #[test]
    fn off_needs_two_misses_and_back_needs_one_answer() {
        let mut watch = Watch::new();
        let seen = run(
            &mut watch,
            vec![
                answered(Health::Good),
                silent(),
                answered(Health::Good),
                silent(),
                Reply::Unrecognised("not the engine"),
                silent(),
                answered(Health::Problem),
            ],
        );
        let shown: Vec<Option<Shown>> = seen
            .iter()
            .map(|seen| seen.change.map(|change| change.to))
            .collect();
        assert_eq!(
            shown,
            [
                Some(Shown::Running(Health::Good)),
                None,
                None,
                None,
                Some(Shown::Off),
                None,
                Some(Shown::Running(Health::Problem)),
            ]
        );
        let off = seen
            .get(4)
            .and_then(|seen| seen.notice.clone())
            .expect("going off notifies");
        assert_eq!(off.title, "The engine isn’t answering");
        assert!(!off.body.contains("patient") && !off.body.contains("result"));
        assert_eq!(seen.get(6).and_then(|seen| seen.notice.clone()), None);
        assert_eq!(watch.last_answer(), Some((at(7), Health::Problem)));
    }

    // ADR 0019: when the app starts with the engine off, the window shows
    // Engine off at once, without a notification.
    #[test]
    fn starting_with_the_engine_off_shows_it_at_once_quietly() {
        let mut watch = Watch::new();
        let seen = watch.observe(silent(), at(1));
        assert_eq!(seen.change.map(|change| change.to), Some(Shown::Off));
        assert_eq!(seen.notice, None);
        assert_eq!(watch.last_answer(), None);
        assert_eq!(watch.last_reply(), Some(&silent()));
    }

    // ADR 0023: every kind has a short title and advice that send the reader
    // to Analyte, all different, and none names a patient or a result value.
    #[test]
    fn every_kind_has_plain_words() {
        let mut titles = Vec::new();
        for reason in Reason::ALL {
            let (title, advice) = (reason.title(), reason.advice());
            assert!(title.len() <= 64, "{title}");
            assert!(advice.contains("Analyte"), "{advice}");
            assert!(!title.ends_with('.'), "{title}");
            assert!(advice.ends_with('.'), "{advice}");
            for word in [title, advice] {
                assert!(!word.contains("patient") && !word.contains('_'), "{word}");
            }
            titles.push(title);
        }
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), Reason::ALL.len());
    }

    // ADR 0023 (title bar): the last notification stays as a line with its
    // time; once what it told of is gone the line says since when, it goes
    // back when it returns, the engine being off does not count as fixed,
    // and the line goes a day after the fix.
    #[test]
    fn the_last_notice_says_since_when_it_is_fixed() {
        let link = Reason::MachineLinkOff;
        let zone = TimeZone::UTC;
        // 2025-11-20 14:32:00 UTC.
        let start = 1_763_649_120;
        let mut watch = Watch::new();
        let mut step = |reply: Reply, offset: i64| {
            let now = at(start + offset);
            watch.observe(reply, now);
            watch.recap().and_then(|recap| recap.line(now, &zone))
        };
        assert_eq!(step(answered(Health::Good), -60), None);
        let told = "Notified 14:32 · A machine’s connection is off";
        assert_eq!(step(with(Health::Problem, &[link]), 0), Some(told.into()));
        // The engine is off: nobody can tell, and the engine off is the news.
        assert_eq!(step(silent(), 60), Some(told.into()));
        let off = step(silent(), 120);
        assert_eq!(
            off,
            Some("Notified 14:34 · The engine isn’t answering".into())
        );
        let back = "Notified 14:34 · The engine isn’t answering · Fixed since 14:35";
        assert_eq!(step(with(Health::Problem, &[link]), 180), Some(back.into()));
        assert_eq!(step(answered(Health::Good), 240), Some(back.into()));
        // A day after the fix the line goes.
        let day = 24 * 3600;
        assert_eq!(
            step(answered(Health::Good), 180 + day),
            Some(
                "Notified 20 Nov 14:34 · The engine isn’t answering · Fixed since 20 Nov 14:35"
                    .into()
            )
        );
        assert_eq!(step(answered(Health::Good), 181 + day), None);

        // Sending fails beside the link: the notice is about sending. It is
        // fixed while the link stays off, then comes back without a second
        // notice, as it was already told, and the line is no longer fixed.
        let failing = Reason::SendingFailing;
        let mut watch = Watch::new();
        let mut step = |reply: Reply, offset: i64| {
            let now = at(start + offset);
            watch.observe(reply, now);
            watch.recap().and_then(|recap| recap.line(now, &zone))
        };
        step(answered(Health::Good), -60);
        let told = "Notified 14:32 · Sending results to a lab app keeps failing";
        assert_eq!(
            step(with(Health::Problem, &[link, failing]), 0),
            Some(told.into())
        );
        assert_eq!(
            step(with(Health::Problem, &[link]), 480),
            Some(format!("{told} · Fixed since 14:40"))
        );
        assert_eq!(
            step(with(Health::Problem, &[link, failing]), 540),
            Some(told.into())
        );
        assert_eq!(watch.recap().map(|recap| recap.sent), Some(at(start)));
    }
}
