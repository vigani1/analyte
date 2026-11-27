//! Asks the engine's public `GET /status` how things are, within a time and size limit, and reads its word and the kinds of thing behind it.
//!
//! The answer is the Overview's verdict as `good`, `attention` or `problem`,
//! or `unknown` when the engine could not read its own state, with the kind
//! of each thing that needs someone from a closed list (ADR 0023). Anything
//! else (no connection, a timeout, another program's answer) is not the
//! engine answering, and the app says what it saw instead of guessing a state.
use analyte_i18n::text as tr;
use std::time::Duration;

/// How long one question may take, connection included: the engine reads
/// its state within 1.5 s and answers within 1.75 s.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// The most of an answer the app reads; the real one is at most about 460
/// bytes, with every kind listed.
const LIMIT: usize = 1024;

/// The most kinds the app reads from one answer; the engine sends each of
/// each known kind at most once.
const MOST_REASONS: usize = 32;

/// A kind of thing that needs someone, as the engine's `/status` names it
/// (ADR 0023, ADR 0026), declared most serious first, as the engine lists them: every
/// kind that stops data now before every kind that only needs a look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    /// Free space is at or below the reserve: new data is refused.
    DiskFull,
    /// Disk space is running low.
    DiskLow,
    /// A lab app cannot be sent to: keys missing or not set up any more.
    LabAppBlocked,
    /// Tries to send to a lab app keep failing.
    SendingFailing,
    /// Analyte stopped working with a machine.
    MachineStopped,
    /// A machine's connection is off.
    MachineLinkOff,
    /// Sending to a lab app is paused.
    SendingPaused,
    /// A machine has sent nothing for too long.
    MachineSilent,
    /// A machine refused a connection from a device not on its list.
    MachineRefusedConnection,
    /// A result has waited too long to be sent.
    ResultWaiting,
    /// Items on Needs review.
    NeedsReview,
    /// Orders whose outcome on the machine is not known.
    OrdersUncertain,
    /// Orders a machine cancelled on its own.
    OrdersCancelled,
    /// Order sending is paused after a restore.
    OrdersPausedAfterRestore,
    /// The order list from the lab app is out of date.
    OrderListStale,
    /// A machine accepts connections from any device on the network.
    MachineOpenToAnyDevice,
    /// A machine reports software outside its checked versions or an unknown version.
    MachineSoftwareVersionChanged,
    /// A machine's selected software version has no manual coverage.
    MachineSoftwareVersionUnchecked,
    /// A machine software version warning could not be saved.
    SoftwareWarningWriteFailed,
}

impl Reason {
    /// Every kind, most serious first.
    pub const ALL: [Self; 19] = [
        Self::DiskFull,
        Self::DiskLow,
        Self::LabAppBlocked,
        Self::SendingFailing,
        Self::MachineStopped,
        Self::MachineLinkOff,
        Self::SendingPaused,
        Self::MachineSilent,
        Self::MachineRefusedConnection,
        Self::ResultWaiting,
        Self::NeedsReview,
        Self::OrdersUncertain,
        Self::OrdersCancelled,
        Self::OrdersPausedAfterRestore,
        Self::OrderListStale,
        Self::MachineOpenToAnyDevice,
        Self::MachineSoftwareVersionChanged,
        Self::MachineSoftwareVersionUnchecked,
        Self::SoftwareWarningWriteFailed,
    ];

    /// The engine's word for this kind.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::DiskFull => "disk_full",
            Self::DiskLow => "disk_low",
            Self::LabAppBlocked => "lab_app_blocked",
            Self::SendingFailing => "sending_failing",
            Self::MachineStopped => "machine_stopped",
            Self::MachineLinkOff => "machine_link_off",
            Self::SendingPaused => "sending_paused",
            Self::MachineSilent => "machine_silent",
            Self::MachineSoftwareVersionChanged => "machine_software_version_changed",
            Self::MachineSoftwareVersionUnchecked => "machine_software_version_unchecked",
            Self::SoftwareWarningWriteFailed => "software_warning_write_failed",
            Self::MachineRefusedConnection => "machine_refused_connection",
            Self::ResultWaiting => "result_waiting",
            Self::NeedsReview => "needs_review",
            Self::OrdersUncertain => "orders_uncertain",
            Self::OrdersCancelled => "orders_cancelled",
            Self::OrdersPausedAfterRestore => "orders_paused_after_restore",
            Self::OrderListStale => "order_list_stale",
            Self::MachineOpenToAnyDevice => "machine_open_to_any_device",
        }
    }

    fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.word() == word)
    }

    /// What happened, as a notification's title says it.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::DiskFull => tr("desktop-the-disk-is-full-analyte-is-refusing-new-data"),
            Self::DiskLow => tr("desktop-the-disk-is-almost-full"),
            Self::LabAppBlocked => tr("desktop-results-can-t-be-sent-to-a-lab-app"),
            Self::SendingFailing => tr("desktop-sending-results-to-a-lab-app-keeps-failing"),
            Self::MachineStopped => tr("desktop-analyte-stopped-working-with-a-machine"),
            Self::MachineLinkOff => tr("desktop-a-machine-s-connection-is-off"),
            Self::SendingPaused => tr("desktop-sending-to-a-lab-app-is-paused"),
            Self::MachineSilent => tr("desktop-a-machine-has-gone-quiet"),
            Self::MachineSoftwareVersionChanged => tr("desktop-check-a-machine-s-software-version"),
            Self::SoftwareWarningWriteFailed => {
                tr("desktop-analyte-could-not-save-a-software-version-warning")
            }
            Self::MachineSoftwareVersionUnchecked => {
                tr("desktop-a-machine-is-using-an-unchecked-software-version")
            }
            Self::MachineRefusedConnection => {
                tr("desktop-a-machine-refused-a-connection-from-another-device")
            }
            Self::ResultWaiting => tr("desktop-a-result-is-waiting-too-long-to-be-sent"),
            Self::NeedsReview => tr("desktop-something-needs-review"),
            Self::OrdersUncertain => tr("desktop-some-orders-have-an-unknown-outcome"),
            Self::OrdersCancelled => tr("desktop-a-machine-cancelled-some-orders"),
            Self::OrdersPausedAfterRestore => tr("desktop-orders-are-paused-after-a-restore"),
            Self::OrderListStale => tr("desktop-the-order-list-from-the-lab-app-is-out"),
            Self::MachineOpenToAnyDevice => {
                tr("desktop-a-machine-accepts-connections-from-any-device")
            }
        }
    }

    /// What to do, as a notification's body says it.
    #[must_use]
    pub fn advice(self) -> &'static str {
        match self {
            Self::DiskFull => tr("desktop-free-up-space-on-this-pc-open-analyte-to"),
            Self::DiskLow => tr("desktop-free-up-space-soon-or-analyte-will-refuse-new"),
            Self::LabAppBlocked => tr("desktop-open-analyte-to-see-which-lab-app-and-what"),
            Self::SendingFailing => tr("desktop-open-analyte-to-see-which-lab-app-and-what-2"),
            Self::MachineStopped | Self::MachineSilent | Self::MachineSoftwareVersionUnchecked => {
                tr("desktop-open-analyte-to-see-which-machine")
            }
            Self::MachineSoftwareVersionChanged => {
                tr("desktop-open-analyte-to-see-which-machine-and-check-its")
            }
            Self::SoftwareWarningWriteFailed => {
                tr("desktop-ask-it-to-check-the-analyte-service-log-for")
            }
            Self::MachineLinkOff => tr("desktop-open-analyte-to-see-which-machine-and-what-to"),
            Self::SendingPaused => tr("desktop-results-wait-until-it-is-resumed-open-analyte-to"),
            Self::ResultWaiting => tr("desktop-open-analyte-to-see-what-holds-it-up"),
            Self::NeedsReview => tr("desktop-open-analyte-and-look-at-needs-review"),
            Self::OrdersUncertain => tr("desktop-open-analyte-to-see-which-orders-to-check-on"),
            Self::OrdersCancelled => tr("desktop-open-analyte-to-see-which-orders"),
            Self::OrdersPausedAfterRestore => {
                tr("desktop-it-must-check-the-orders-before-sending-starts-again")
            }
            Self::OrderListStale => tr("desktop-open-analyte-to-see-when-it-last-came"),
            Self::MachineRefusedConnection => {
                tr("desktop-open-analyte-to-see-which-machine-and-which-device")
            }
            Self::MachineOpenToAnyDevice => {
                tr("desktop-open-analyte-to-see-which-machine-and-limit-it")
            }
        }
    }

    const fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// A set of known status reasons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reasons(u32);

impl Reasons {
    /// No kind.
    pub const NONE: Self = Self(0);

    /// The set of `reasons`.
    #[must_use]
    pub fn of(reasons: &[Reason]) -> Self {
        reasons
            .iter()
            .fold(Self::NONE, |set, reason| set.with(*reason))
    }

    /// This set and `reason`.
    #[must_use]
    pub const fn with(self, reason: Reason) -> Self {
        Self(self.0 | reason.bit())
    }

    /// Whether `reason` is in the set.
    #[must_use]
    pub const fn contains(self, reason: Reason) -> bool {
        self.0 & reason.bit() != 0
    }

    /// The kinds in this set and not in `other`.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The kinds in either set.
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many kinds the set holds.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// The most serious kind in the set.
    #[must_use]
    pub fn first(self) -> Option<Reason> {
        Reason::ALL
            .into_iter()
            .find(|reason| self.contains(*reason))
    }
}

/// What the engine said about the lab's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// Nothing needs anyone.
    Good,
    /// Something needs a look; nothing stops data now.
    Attention,
    /// Something stops data now.
    Problem,
    /// The engine runs but could not read its own state in time.
    Unknown,
}

impl Health {
    /// The engine's word for this state.
    fn of_word(word: &str) -> Option<Self> {
        match word {
            "good" => Some(Self::Good),
            "attention" => Some(Self::Attention),
            "problem" => Some(Self::Problem),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// The state in plain words, as the tray and notifications say it.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::Good => tr("desktop-all-good"),
            Self::Attention => tr("desktop-something-needs-you"),
            Self::Problem => tr("desktop-a-problem-needs-you"),
            Self::Unknown => tr("desktop-the-engine-could-not-read-its-state"),
        }
    }
}

/// What one question to `/status` brought back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The engine answered with its state and the kinds of thing behind it.
    Answered(Health, Reasons),
    /// Something answered at the engine's address, but not as the engine
    /// does; what was wrong, in plain words.
    Unrecognised(&'static str),
    /// Nothing answered in time.
    Silent(Silence),
}

/// Why nothing answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silence {
    /// Nothing accepts connections at the address: the engine, or its admin
    /// pages, are not running.
    Refused,
    /// The question took longer than [`TIMEOUT`].
    TimedOut,
    /// The connection broke before a whole answer came.
    Broken,
}

impl Silence {
    /// What the app observed, in plain words.
    #[must_use]
    pub fn words(self) -> &'static str {
        match self {
            Self::Refused => tr("desktop-nothing-accepts-connections-at-the-engine-s-address"),
            Self::TimedOut => tr("desktop-the-engine-did-not-answer-within-3-seconds"),
            Self::Broken => tr("desktop-the-connection-to-the-engine-broke-before-it-answered"),
        }
    }
}

/// The body `/status` sends; any further field is ignored, and an engine
/// from before ADR 0023 sends no `reasons`.
#[derive(serde::Deserialize)]
struct Body {
    state: String,
    version: String,
    #[serde(default)]
    reasons: Vec<String>,
}

/// Asks one engine's `/status`. It never follows a redirect and never uses
/// a proxy: the engine is on this computer.
pub struct Client {
    http: reqwest::Client,
    url: String,
}

impl Client {
    /// A client for `engine`.
    ///
    /// # Errors
    ///
    /// When the HTTP client cannot be set up on this system.
    pub fn new(engine: &super::Engine) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(TIMEOUT)
                .connect_timeout(TIMEOUT)
                .build()?,
            url: engine.status(),
        })
    }

    /// Asks once, within [`TIMEOUT`] and `LIMIT` bytes.
    pub async fn ask(&self) -> Reply {
        match tokio::time::timeout(TIMEOUT, self.exchange()).await {
            Ok(reply) => reply,
            Err(_) => Reply::Silent(Silence::TimedOut),
        }
    }

    async fn exchange(&self) -> Reply {
        let mut response = match self.http.get(&self.url).send().await {
            Ok(response) => response,
            Err(error) => return Reply::Silent(silence(&error)),
        };
        let status = response.status().as_u16();
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len().saturating_add(chunk.len()) > LIMIT {
                        return Reply::Unrecognised(tr(
                            "desktop-the-answer-is-larger-than-the-engine-s",
                        ));
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(error) => return Reply::Silent(silence(&error)),
            }
        }
        read(status, &body)
    }
}

/// The reply a whole answer with `status` and `body` gives.
fn read(status: u16, body: &[u8]) -> Reply {
    let Ok(Body {
        state,
        version,
        reasons,
    }) = serde_json::from_slice::<Body>(body)
    else {
        return Reply::Unrecognised(tr("desktop-the-answer-is-not-the-engine-s-status"));
    };
    let Some(health) = Health::of_word(&state) else {
        return Reply::Unrecognised(tr("desktop-the-answer-names-a-state-the-app-does-not"));
    };
    // The engine answers 200 with its verdict and 503 when it could not read it.
    let expected = if health == Health::Unknown { 503 } else { 200 };
    if status != expected || version.is_empty() {
        return Reply::Unrecognised(tr("desktop-the-answer-is-not-the-engine-s-status"));
    }
    // A kind from a newer engine is left out; the state still counts.
    let reasons = reasons
        .iter()
        .take(MOST_REASONS)
        .filter_map(|word| Reason::of_word(word))
        .fold(Reasons::NONE, Reasons::with);
    // The engine lists kinds only when something needs someone.
    if matches!(health, Health::Good | Health::Unknown) && !reasons.is_empty() {
        return Reply::Unrecognised(tr("desktop-the-answer-is-not-the-engine-s-status"));
    }
    Reply::Answered(health, reasons)
}

fn silence(error: &reqwest::Error) -> Silence {
    if error.is_timeout() {
        Silence::TimedOut
    } else if error.is_connect() {
        Silence::Refused
    } else {
        Silence::Broken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0019: the four words the engine's `/status` answers with, each with
    // the status code it comes with.
    #[test]
    fn the_engine_words_read_as_states() {
        for (status, word, health) in [
            (200, "good", Health::Good),
            (200, "attention", Health::Attention),
            (200, "problem", Health::Problem),
            (503, "unknown", Health::Unknown),
        ] {
            let body = format!("{{\"state\":\"{word}\",\"version\":\"0.1.0\"}}");
            assert_eq!(
                read(status, body.as_bytes()),
                Reply::Answered(health, Reasons::NONE)
            );
            let body = format!("{{\"state\":\"{word}\",\"reasons\":[],\"version\":\"0.1.0\"}}");
            assert_eq!(
                read(status, body.as_bytes()),
                Reply::Answered(health, Reasons::NONE)
            );
        }
    }

    // ADR 0023: the engine's kinds are read as a set; a kind a newer engine
    // added is left out and the state still counts; the most serious kind
    // is first whatever the order sent.
    #[test]
    fn the_engine_kinds_read_as_a_set() {
        let all: Vec<String> = Reason::ALL
            .iter()
            .rev()
            .map(|reason| format!("\"{}\"", reason.word()))
            .collect();
        let body = format!(
            "{{\"state\":\"problem\",\"reasons\":[{}],\"version\":\"0.1.0\"}}",
            all.join(",")
        );
        let Reply::Answered(Health::Problem, reasons) = read(200, body.as_bytes()) else {
            panic!("every kind reads");
        };
        assert_eq!(usize::try_from(reasons.len()), Ok(Reason::ALL.len()));
        assert_eq!(reasons.first(), Some(Reason::DiskFull));
        let body = "{\"state\":\"attention\",\"reasons\":[\"printer_jam\",\"needs_review\",\"needs_review\"],\"version\":\"0.1.0\"}";
        assert_eq!(
            read(200, body.as_bytes()),
            Reply::Answered(Health::Attention, Reasons::of(&[Reason::NeedsReview]))
        );
        let body = "{\"state\":\"attention\",\"reasons\":[\"printer_jam\"],\"version\":\"0.1.0\"}";
        assert_eq!(
            read(200, body.as_bytes()),
            Reply::Answered(Health::Attention, Reasons::NONE)
        );
    }

    // ADR 0023: the app knows the same kind words as the engine, in the same
    // most-serious-first order; both read them from one file.
    #[test]
    fn the_kinds_match_the_engine() {
        let shared: Vec<&str> = include_str!("../../../testing/fixtures/status-reasons.txt")
            .lines()
            .collect();
        let known: Vec<&str> = Reason::ALL.iter().map(|reason| reason.word()).collect();
        assert_eq!(known, shared);
    }

    // ADR 0023: the app reads only so many kinds, so a long list cannot
    // grow what it holds; past the 32nd entry nothing more is looked at.
    #[test]
    fn only_the_first_kinds_are_read() {
        let mut words = vec!["\"printer_jam\""; 32];
        words.push("\"disk_full\"");
        let body = format!(
            "{{\"state\":\"problem\",\"reasons\":[{}],\"version\":\"0.1.0\"}}",
            words.join(",")
        );
        assert_eq!(
            read(200, body.as_bytes()),
            Reply::Answered(Health::Problem, Reasons::NONE)
        );
    }

    // AGENTS.md (never guess): an answer that is not the engine's is never
    // read as a state, so another program on the port is not "All good".
    #[test]
    fn other_answers_are_not_a_state() {
        for (status, body) in [
            (200, "<html>sign in</html>"),
            (200, "{\"state\":\"fine\",\"version\":\"0.1.0\"}"),
            (200, "{\"state\":\"good\"}"),
            (200, "{\"state\":\"good\",\"version\":\"\"}"),
            (503, "{\"state\":\"good\",\"version\":\"0.1.0\"}"),
            (200, "{\"state\":\"unknown\",\"version\":\"0.1.0\"}"),
            (401, "{\"state\":\"good\",\"version\":\"0.1.0\"}"),
            (
                200,
                "{\"state\":\"good\",\"reasons\":[\"disk_full\"],\"version\":\"0.1.0\"}",
            ),
            (
                503,
                "{\"state\":\"unknown\",\"reasons\":[\"disk_full\"],\"version\":\"0.1.0\"}",
            ),
            (
                200,
                "{\"state\":\"problem\",\"reasons\":\"disk_full\",\"version\":\"0.1.0\"}",
            ),
            (
                200,
                "{\"state\":\"problem\",\"reasons\":[3],\"version\":\"0.1.0\"}",
            ),
        ] {
            assert!(
                matches!(read(status, body.as_bytes()), Reply::Unrecognised(_)),
                "{status} {body}"
            );
        }
    }
}
