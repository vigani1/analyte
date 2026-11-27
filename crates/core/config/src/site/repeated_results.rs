//! Parses the site's repeated-results choice: automatic or always ask (ADR 0035).
use serde::{Deserialize, Serialize};

/// How Analyte treats a result whose measurement key an earlier upload from
/// the same machine already retained (ADR 0035, 2026-11-26). The lab chooses
/// it in its site file; it can only make handling more manual.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatedResults {
    /// The definition's identity recipe tells a resend, a new result, a
    /// correction and a same-key change apart (the ADR 0035 behaviour).
    #[default]
    Automatic,
    /// Every repeat of a retained key is held in Needs review for a person;
    /// a first-seen key still flows normally.
    AlwaysAsk,
}

impl RepeatedResults {
    /// Every value, in the order Setup offers them.
    pub const ALL: [Self; 2] = [Self::Automatic, Self::AlwaysAsk];

    /// The spelling the site file stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::AlwaysAsk => "always_ask",
        }
    }

    /// The plain words the pages and `machines list` show.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Automatic => "handle automatically",
            Self::AlwaysAsk => "always ask me",
        }
    }

    /// Reads the site file spelling; any other text is refused.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.as_str() == text)
    }

    /// Whether this is the default, which the site file leaves unwritten.
    #[must_use]
    pub fn is_automatic(&self) -> bool {
        *self == Self::Automatic
    }
}

#[cfg(test)]
mod tests {
    use super::RepeatedResults;

    // ADR 0035 (2026-11-26): only the two spellings load; anything else is refused.
    #[test]
    fn only_the_two_spellings_are_accepted() {
        for value in RepeatedResults::ALL {
            assert_eq!(RepeatedResults::parse(value.as_str()), Some(value));
        }
        for text in ["", "Automatic", "ask", "always-ask", "manual", " automatic"] {
            assert_eq!(RepeatedResults::parse(text), None, "{text}");
        }
        assert_eq!(RepeatedResults::default(), RepeatedResults::Automatic);
    }
}
