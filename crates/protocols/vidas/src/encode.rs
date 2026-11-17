//! Writes VIDAS tagged-field messages through the parser's validation (BCI Appendix A chapter 4).
use crate::syntax::{Error, Message, SampleLimit};

impl Message {
    /// Builds through the same validation boundary, without reordering or truncation.
    /// # Errors
    /// Returns the same syntax errors as parsing; pipe/control injection is rejected.
    pub fn new(fields: &[(&str, &str)], limit: SampleLimit) -> Result<Self, Error> {
        let mut text = String::new();
        for (tag, value) in fields {
            if tag.len() != 2
                || value.contains('|')
                || text
                    .len()
                    .checked_add(tag.len())
                    .and_then(|n| n.checked_add(value.len()))
                    .and_then(|n| n.checked_add(1))
                    .is_none_or(|n| n > 1920)
            {
                return Err(Error::Syntax);
            }
            text.push_str(tag);
            text.push_str(value);
            text.push('|');
        }
        Self::parse(&text, limit)
    }
}
