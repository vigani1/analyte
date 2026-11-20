//! Holds the small display values many admin pages share: a boxed notice, words with their exact value on hover, and the confirmation that asks for a reason.

use super::Tone;

/// A boxed sentence: a tone, a title and, when needed, more text.
pub(in crate::admin) struct Notice {
    pub(in crate::admin) tone: Tone,
    pub(in crate::admin) title: String,
    pub(in crate::admin) text: String,
}

/// Words with their exact value on hover; `exact` is empty when there is none.
#[derive(Clone)]
pub(in crate::admin) struct Hover {
    pub(in crate::admin) shown: String,
    pub(in crate::admin) exact: String,
}

/// A confirmation drawn over a page that asks for a reason before an audited
/// action: a real form with the session's form token, and Cancel back to the
/// page. The page's own fields, such as hidden ones, come from its template.
pub(in crate::admin) struct ReasonDialog {
    pub(in crate::admin) action: String,
    pub(in crate::admin) title: &'static str,
    pub(in crate::admin) text: String,
    pub(in crate::admin) placeholder: &'static str,
    pub(in crate::admin) hint: &'static str,
    pub(in crate::admin) button: &'static str,
    pub(in crate::admin) csrf: String,
    pub(in crate::admin) cancel: String,
    /// What was typed, shown again after a refusal.
    pub(in crate::admin) reason: String,
    pub(in crate::admin) error: Option<&'static str>,
}
