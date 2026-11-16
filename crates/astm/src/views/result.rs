//! Read-only result record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 result positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Result<'a>(&'a Record);
impl<'a> Result<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Result).then_some(Self(record))
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
    /// Returns standard field 3 (test id), retaining all wire content.
    #[must_use]
    pub fn test_id(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (value), retaining all wire content.
    #[must_use]
    pub fn value(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (units), retaining all wire content.
    #[must_use]
    pub fn units(self) -> Option<&'a Field> {
        self.0.field(5)
    }
    /// Returns standard field 6 (ranges), retaining all wire content.
    #[must_use]
    pub fn ranges(self) -> Option<&'a Field> {
        self.0.field(6)
    }
    /// Returns standard field 7 (flags), retaining all wire content.
    #[must_use]
    pub fn flags(self) -> Option<&'a Field> {
        self.0.field(7)
    }
    /// Returns standard field 8 (abnormality basis), retaining all wire content.
    #[must_use]
    pub fn abnormality_basis(self) -> Option<&'a Field> {
        self.0.field(8)
    }
    /// Returns standard field 9 (status), retaining all wire content.
    #[must_use]
    pub fn status(self) -> Option<&'a Field> {
        self.0.field(9)
    }
    /// Returns standard field 10 (normative change), retaining all wire content.
    #[must_use]
    pub fn normative_change(self) -> Option<&'a Field> {
        self.0.field(10)
    }
    /// Returns standard field 11 (operator), retaining all wire content.
    #[must_use]
    pub fn operator(self) -> Option<&'a Field> {
        self.0.field(11)
    }
    /// Returns standard field 12 (started at), retaining all wire content.
    #[must_use]
    pub fn started_at(self) -> Option<&'a Field> {
        self.0.field(12)
    }
    /// Returns standard field 13 (completed at), retaining all wire content.
    #[must_use]
    pub fn completed_at(self) -> Option<&'a Field> {
        self.0.field(13)
    }
    /// Returns standard field 14 (instrument), retaining all wire content.
    #[must_use]
    pub fn instrument(self) -> Option<&'a Field> {
        self.0.field(14)
    }
}
impl Result<'_> {
    /// Keeps every standard result status distinct and retains unknown codes.
    #[must_use]
    pub fn status_code(self) -> Option<crate::ResultStatus> {
        self.status()
            .map(|f| crate::ResultStatus::from_bytes(f.raw()))
    }
}
