//! Windows sign-in for the Analyte app (ADR 0021): who may sign in, the pipe's two lines and, on Windows, the pipe the engine serves them on.
pub(super) mod line;
#[cfg(windows)]
mod pipe;
pub(super) mod rule;
#[cfg(windows)]
pub(super) use pipe::{Start, Unredeemed, start};

/// The longest account name kept, in bytes (ADR 0021 §5: it must fit the
/// audit actor; a longer one is refused, never cut).
const NAME_LIMIT: usize = 256;

/// A verified account's name as Windows gave it, `DOMAIN\name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::admin) struct AccountName(String);

/// Why text is not an account name the engine keeps.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a DOMAIN\\name account name of at most 256 bytes")]
pub(in crate::admin) struct NotName;

impl AccountName {
    /// Accepts `DOMAIN\name` with both parts non-empty, one backslash, no `:`
    /// (Windows names cannot hold one, so the audit spelling reads back) and no
    /// control characters, in at most [`NAME_LIMIT`] bytes.
    /// # Errors
    /// [`NotName`] for anything else.
    pub(in crate::admin) fn new(text: String) -> Result<Self, NotName> {
        let parts = text.split_once('\\');
        let shaped = parts.is_some_and(|(domain, name)| {
            !domain.is_empty() && !name.is_empty() && !name.contains('\\')
        });
        let clean = !text.chars().any(|c| c == ':' || c.is_control());
        if shaped && clean && text.len() <= NAME_LIMIT {
            Ok(Self(text))
        } else {
            Err(NotName)
        }
    }

    /// `DOMAIN\name`.
    pub(in crate::admin) fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0021 §5: names are DOMAIN\name, bounded, without ':' or controls.
    #[test]
    fn account_names_are_domain_and_name() {
        let name = AccountName::new("LAB-PC\\maria".into());
        assert_eq!(name.as_ref().map(AccountName::as_str), Ok("LAB-PC\\maria"));
        for bad in [
            "maria",
            "\\maria",
            "LAB-PC\\",
            "LAB-PC\\ma\\ria",
            "LAB-PC\\ma:ria",
            "LAB-PC\\ma\nria",
        ] {
            assert_eq!(AccountName::new(bad.into()), Err(NotName), "{bad}");
        }
        let long = format!("LAB-PC\\{}", "m".repeat(NAME_LIMIT));
        assert_eq!(AccountName::new(long), Err(NotName));
    }
}
