//! Read-only terminator record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 terminator positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Terminator<'a>(&'a Record);
impl<'a> Terminator<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Terminator).then_some(Self(record))
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
    /// Returns standard field 3 (code), retaining all wire content.
    #[must_use]
    pub fn code(self) -> Option<&'a Field> {
        self.0.field(3)
    }
}
impl Terminator<'_> {
    /// Recognizes completion reasons without conflating unknown codes with success.
    #[must_use]
    pub fn termination_code(self) -> Option<crate::TerminationCode> {
        self.code()
            .map(|f| crate::TerminationCode::from_bytes(f.raw()))
    }
}
