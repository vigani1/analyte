//! Read-only comment record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 comment positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Comment<'a>(&'a Record);
impl<'a> Comment<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Comment).then_some(Self(record))
    }
    /// Preserves access to unknown and manufacturer-defined positions.
    #[must_use]
    pub fn field(self, position: usize) -> Option<&'a Field> {
        self.0.field(position)
    }
    /// Returns standard field 2 (sequence), retaining all wire content.
    #[must_use]
    pub fn sequence(self) -> Option<&'a Field> {
        self.0.field(2)
    }
    /// Returns standard field 3 (source), retaining all wire content.
    #[must_use]
    pub fn source(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (text), retaining all wire content.
    #[must_use]
    pub fn text(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (comment type), retaining all wire content.
    #[must_use]
    pub fn comment_type(self) -> Option<&'a Field> {
        self.0.field(5)
    }
}
