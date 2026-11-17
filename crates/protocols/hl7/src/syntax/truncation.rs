//! Preserves sender truncation markers and refuses incomplete clinical input.
use super::{Error, Message};

impl Message {
    /// Refuses sender-marked incomplete data before typed interpretation or delivery.
    /// The original bytes remain available through `raw`.
    /// # Errors
    /// Returns the first marked field, excluding MSH delimiter declarations.
    pub fn require_complete_values(&self) -> Result<(), Error> {
        for (index, segment) in self.segments().enumerate() {
            for (number, field) in segment.fields().enumerate() {
                if segment.id() == b"MSH" && number < 2 {
                    continue;
                }
                if field.is_truncated() {
                    return Err(Error::Truncated {
                        segment: index.saturating_add(1),
                        field: number.saturating_add(1),
                    });
                }
            }
        }
        Ok(())
    }
}
