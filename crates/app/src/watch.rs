//! Decides what the tray shows from each answer of the engine's `/status`, and when a change earns a notification.
//!
//! The engine counts as off only after [`MISSES_FOR_OFF`] questions in a row
//! went unanswered, so a restart from the setup page does not flash "off".
//! A notification is sent only when the state gets worse (good, then
//! attention, then problem, then off), once per change, and never for the
//! first state the app sees: it says nothing about patients or results.
use crate::engine::status::{Health, Reply};
use jiff::Timestamp;

/// How many unanswered questions in a row make the engine count as off once
/// it was seen running; before the first answer one is enough.
pub const MISSES_FOR_OFF: u32 = 2;

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
    pub const fn words(self) -> &'static str {
        match self {
            Self::Checking => "Checking the engine…",
            Self::Running(health) => health.words(),
            Self::Off => "The engine isn’t answering",
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

/// A notification's words: the state, and that clicking opens the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notice {
    /// The state in plain words.
    pub title: &'static str,
    /// That a click opens the window.
    pub body: &'static str,
}

impl Change {
    /// The notification this change earns: only a change to a worse state,
    /// never the first state seen.
    #[must_use]
    pub fn notice(self) -> Option<Notice> {
        let (Some(from), Some(to)) = (self.from.rank(), self.to.rank()) else {
            return None;
        };
        if to <= from {
            return None;
        }
        let body = match self.to {
            Shown::Off => "Click to open Analyte and see what the app saw.",
            Shown::Checking | Shown::Running(_) => "Click to open Analyte and see what.",
        };
        Some(Notice {
            title: self.to.words(),
            body,
        })
    }
}

/// What the app has seen of the engine so far.
#[derive(Clone, Debug)]
pub struct Watch {
    shown: Shown,
    /// Unanswered questions since the last answer.
    misses: u32,
    /// When the engine last answered, and with which state.
    last_answer: Option<(Timestamp, Health)>,
    /// The last question's reply.
    last_reply: Option<Reply>,
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
            misses: 0,
            last_answer: None,
            last_reply: None,
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

    /// Takes one reply received at `now`; gives the change of what is shown, if any.
    pub fn observe(&mut self, reply: Reply, now: Timestamp) -> Option<Change> {
        let next = match &reply {
            Reply::Answered(health) => {
                self.misses = 0;
                self.last_answer = Some((now, *health));
                Shown::Running(*health)
            }
            Reply::Unrecognised(_) | Reply::Silent(_) => {
                self.misses = self.misses.saturating_add(1);
                match self.shown {
                    Shown::Running(_) if self.misses < MISSES_FOR_OFF => self.shown,
                    Shown::Checking | Shown::Running(_) | Shown::Off => Shown::Off,
                }
            }
        };
        self.last_reply = Some(reply);
        let from = std::mem::replace(&mut self.shown, next);
        (from != next).then_some(Change { from, to: next })
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

    /// Feeds replies one second apart; gives each change.
    fn run(watch: &mut Watch, replies: Vec<Reply>) -> Vec<Option<Change>> {
        replies
            .into_iter()
            .zip(1..)
            .map(|(reply, second)| watch.observe(reply, at(second)))
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
    // good, attention, problem, off; never the first state, never a better one.
    #[test]
    fn only_a_worse_state_notifies() {
        let mut watch = Watch::new();
        let changes = run(
            &mut watch,
            vec![
                Reply::Answered(Health::Attention),
                Reply::Answered(Health::Attention),
                Reply::Answered(Health::Problem),
                Reply::Answered(Health::Unknown),
                Reply::Answered(Health::Good),
                Reply::Answered(Health::Attention),
            ],
        );
        let notices: Vec<Option<&str>> = changes
            .iter()
            .map(|change| change.and_then(Change::notice).map(|notice| notice.title))
            .collect();
        assert_eq!(
            notices,
            [
                None, // the first state seen
                None, // no change
                Some("A problem needs you"),
                None, // as bad as before
                None, // better
                Some("Something needs you"),
            ]
        );
        assert_eq!(changes.iter().flatten().count(), 5);
    }

    // ADR 0019 (Engine off): a running engine counts as off after two
    // unanswered questions in a row, which notifies once; it is back with the
    // first answer, and coming back never notifies.
    #[test]
    fn off_needs_two_misses_and_back_needs_one_answer() {
        let mut watch = Watch::new();
        let changes = run(
            &mut watch,
            vec![
                Reply::Answered(Health::Good),
                silent(),
                Reply::Answered(Health::Good),
                silent(),
                Reply::Unrecognised("not the engine"),
                silent(),
                Reply::Answered(Health::Problem),
            ],
        );
        let shown: Vec<Option<Shown>> = changes.iter().map(|c| c.map(|c| c.to)).collect();
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
        let off = changes
            .get(4)
            .copied()
            .flatten()
            .and_then(Change::notice)
            .expect("going off notifies");
        assert_eq!(off.title, "The engine isn’t answering");
        assert!(!off.body.contains("patient") && !off.body.contains("result"));
        assert_eq!(
            changes.get(6).copied().flatten().and_then(Change::notice),
            None
        );
        assert_eq!(watch.last_answer(), Some((at(7), Health::Problem)));
    }

    // ADR 0019: when the app starts with the engine off, the window shows
    // Engine off at once, without a notification.
    #[test]
    fn starting_with_the_engine_off_shows_it_at_once_quietly() {
        let mut watch = Watch::new();
        let change = watch.observe(silent(), at(1)).expect("a change");
        assert_eq!(change.to, Shown::Off);
        assert_eq!(change.notice(), None);
        assert_eq!(watch.last_answer(), None);
        assert_eq!(watch.last_reply(), Some(&silent()));
    }
}
