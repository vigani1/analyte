//! Read-only manufacturer record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 manufacturer positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Manufacturer<'a>(&'a Record);
impl<'a> Manufacturer<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Manufacturer).then_some(Self(record))
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
}
