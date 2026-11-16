//! Reads ASTM query record fields.
use crate::{Field, Kind, Record};
/// Read-only LIS02 query positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Query<'a>(&'a Record);
impl<'a> Query<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Query).then_some(Self(record))
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
    /// Returns standard field 3 (range start), retaining all wire content.
    #[must_use]
    pub fn range_start(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (range end), retaining all wire content.
    #[must_use]
    pub fn range_end(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (test id), retaining all wire content.
    #[must_use]
    pub fn test_id(self) -> Option<&'a Field> {
        self.0.field(5)
    }
    /// Returns standard field 6 (time basis), retaining all wire content.
    #[must_use]
    pub fn time_basis(self) -> Option<&'a Field> {
        self.0.field(6)
    }
    /// Returns standard field 7 (beginning), retaining all wire content.
    #[must_use]
    pub fn beginning(self) -> Option<&'a Field> {
        self.0.field(7)
    }
    /// Returns standard field 8 (ending), retaining all wire content.
    #[must_use]
    pub fn ending(self) -> Option<&'a Field> {
        self.0.field(8)
    }
    /// Returns standard field 9 (physician), retaining all wire content.
    #[must_use]
    pub fn physician(self) -> Option<&'a Field> {
        self.0.field(9)
    }
    /// Returns standard field 10 (phone), retaining all wire content.
    #[must_use]
    pub fn phone(self) -> Option<&'a Field> {
        self.0.field(10)
    }
    /// Returns standard field 11 (user 1), retaining all wire content.
    #[must_use]
    pub fn user_1(self) -> Option<&'a Field> {
        self.0.field(11)
    }
    /// Returns standard field 12 (user 2), retaining all wire content.
    #[must_use]
    pub fn user_2(self) -> Option<&'a Field> {
        self.0.field(12)
    }
    /// Returns standard field 13 (status), retaining all wire content.
    #[must_use]
    pub fn status(self) -> Option<&'a Field> {
        self.0.field(13)
    }
}
