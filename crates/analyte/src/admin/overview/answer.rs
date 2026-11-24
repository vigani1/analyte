//! The Overview's verdict as data: the one answer `GET /overview/verdict` gives and `analyte overview` reads, with the same lines, in the same order, as the page.
//!
//! Each line holds the words the page shows (names and counts, never payloads
//! or sample IDs, safety rule 10), where the page fixes it and the command
//! that does the same from the command line. The endpoint is protected like
//! every page; only `GET /status` is public, and it says no names or counts.
use super::attention::{Overall, Thing};
use crate::admin::view::Tone;
use serde::{Deserialize, Serialize};

/// The overall state, as the verdict shows it, or unknown when the engine
/// could not read it in time. Unknown is never "All good".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Verdict {
    /// "All good": nothing needs anyone.
    Good,
    /// Something needs a look, but nothing stops data now.
    Attention,
    /// At least one thing stops data now.
    Problem,
    /// The engine could not read how things are.
    Unknown,
}

impl Verdict {
    /// The process exit code `analyte overview` and `analyte doctor` give for
    /// scripts: 0 good, 1 attention, 2 problem, 3 unknown.
    pub(crate) const fn exit_code(self) -> u8 {
        match self {
            Self::Good => 0,
            Self::Attention => 1,
            Self::Problem => 2,
            Self::Unknown => 3,
        }
    }

    const fn of(overall: Overall) -> Self {
        match overall {
            Overall::Good => Self::Good,
            Overall::Attention => Self::Attention,
            Overall::Problem => Self::Problem,
        }
    }
}

/// How serious one line is: a problem stops data now; attention needs a look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    Problem,
    Attention,
}

/// One thing that needs someone, as the Overview lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Need {
    pub(crate) severity: Severity,
    /// The kind `GET /status` names, such as `needs_review`.
    pub(crate) reason: String,
    /// The machine or lab app it is about, by name, when it is about one.
    pub(crate) about: Option<String>,
    /// The line's own words, such as "3 items need review".
    pub(crate) text: String,
    /// What was observed and what to check, when the page shows one.
    pub(crate) detail: Option<String>,
    /// The page or Overview section that fixes it, such as "Needs review".
    pub(crate) page: String,
    /// That page's address on the admin site, such as `/parked`.
    pub(crate) href: String,
    /// The command that shows or fixes it, when one does.
    pub(crate) command: Option<String>,
}

impl Need {
    /// The line for `thing`, with its fix's page and command.
    pub(super) fn of(thing: &Thing) -> Self {
        Self {
            severity: match thing.tone() {
                Tone::Problem => Severity::Problem,
                // A line is a problem or needs a look; no other tone is used.
                Tone::Attention | Tone::Good | Tone::Neutral => Severity::Attention,
            },
            reason: thing.reason().word().to_owned(),
            about: thing.about().map(|name| name.as_str().to_owned()),
            text: thing.text.clone(),
            detail: thing.detail.clone(),
            page: thing.fix.label().to_owned(),
            href: thing.fix.href().to_owned(),
            command: thing.fix.command().map(str::to_owned),
        }
    }
}

/// The whole answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Answer {
    pub(crate) verdict: Verdict,
    /// Every thing that needs someone, in Overview display order; empty when all is
    /// good or the state is unknown.
    pub(crate) needs: Vec<Need>,
    /// Why the state is unknown: what the engine could not read.
    pub(crate) unread: Option<String>,
    /// The engine's version, such as `0.1.0`.
    pub(crate) version: String,
}

impl Answer {
    /// The verdict on `things`, exactly as the page draws it.
    pub(super) fn of(things: &[Thing]) -> Self {
        Self {
            verdict: Verdict::of(Overall::of(things)),
            needs: things.iter().map(Need::of).collect(),
            unread: None,
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    /// The verdict's title, as the page shows it: "All good", "3 things need
    /// you", or for an unknown state that Analyte cannot say how things are.
    pub(crate) fn title(&self) -> String {
        match self.verdict {
            Verdict::Unknown => "Analyte cannot say how things are right now".into(),
            Verdict::Good | Verdict::Attention | Verdict::Problem => {
                super::page::title(self.needs.len())
            }
        }
    }

    /// The line under the title, as the page shows it, or what could not be read.
    pub(crate) fn sub(&self) -> String {
        match (&self.verdict, &self.unread) {
            (Verdict::Unknown, Some(unread)) => unread.clone(),
            (Verdict::Unknown, None) => "The engine could not read its state.".into(),
            (Verdict::Good | Verdict::Attention | Verdict::Problem, _) => {
                super::page::sub(self.needs.len()).to_owned()
            }
        }
    }

    /// The answer when the state could not be read: `unread` says what.
    pub(super) fn unknown(unread: &str) -> Self {
        Self {
            verdict: Verdict::Unknown,
            needs: Vec::new(),
            unread: Some(unread.to_owned()),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}
