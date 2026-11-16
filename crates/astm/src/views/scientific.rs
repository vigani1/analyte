//! Read-only scientific record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 scientific positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Scientific<'a>(&'a Record);
impl<'a> Scientific<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Scientific).then_some(Self(record))
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
    /// Returns standard field 3 (method), retaining all wire content.
    #[must_use]
    pub fn method(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (instrument), retaining all wire content.
    #[must_use]
    pub fn instrument(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (reagents), retaining all wire content.
    #[must_use]
    pub fn reagents(self) -> Option<&'a Field> {
        self.0.field(5)
    }
    /// Returns standard field 6 (units), retaining all wire content.
    #[must_use]
    pub fn units(self) -> Option<&'a Field> {
        self.0.field(6)
    }
    /// Returns standard field 7 (qc), retaining all wire content.
    #[must_use]
    pub fn qc(self) -> Option<&'a Field> {
        self.0.field(7)
    }
    /// Returns standard field 8 (specimen descriptor), retaining all wire content.
    #[must_use]
    pub fn specimen_descriptor(self) -> Option<&'a Field> {
        self.0.field(8)
    }
    /// Returns standard field 9 (reserved), retaining all wire content.
    #[must_use]
    pub fn reserved(self) -> Option<&'a Field> {
        self.0.field(9)
    }
    /// Returns standard field 10 (container), retaining all wire content.
    #[must_use]
    pub fn container(self) -> Option<&'a Field> {
        self.0.field(10)
    }
    /// Returns standard field 11 (specimen id), retaining all wire content.
    #[must_use]
    pub fn specimen_id(self) -> Option<&'a Field> {
        self.0.field(11)
    }
    /// Returns standard field 12 (analyte), retaining all wire content.
    #[must_use]
    pub fn analyte(self) -> Option<&'a Field> {
        self.0.field(12)
    }
    /// Returns standard field 13 (result), retaining all wire content.
    #[must_use]
    pub fn result(self) -> Option<&'a Field> {
        self.0.field(13)
    }
    /// Returns standard field 14 (result units), retaining all wire content.
    #[must_use]
    pub fn result_units(self) -> Option<&'a Field> {
        self.0.field(14)
    }
    /// Returns standard field 15 (collected at), retaining all wire content.
    #[must_use]
    pub fn collected_at(self) -> Option<&'a Field> {
        self.0.field(15)
    }
    /// Returns standard field 16 (result at), retaining all wire content.
    #[must_use]
    pub fn result_at(self) -> Option<&'a Field> {
        self.0.field(16)
    }
    /// Returns standard field 17 (preprocessing), retaining all wire content.
    #[must_use]
    pub fn preprocessing(self) -> Option<&'a Field> {
        self.0.field(17)
    }
    /// Returns standard field 18 (diagnosis), retaining all wire content.
    #[must_use]
    pub fn diagnosis(self) -> Option<&'a Field> {
        self.0.field(18)
    }
    /// Returns standard field 19 (birthdate), retaining all wire content.
    #[must_use]
    pub fn birthdate(self) -> Option<&'a Field> {
        self.0.field(19)
    }
    /// Returns standard field 20 (sex), retaining all wire content.
    #[must_use]
    pub fn sex(self) -> Option<&'a Field> {
        self.0.field(20)
    }
    /// Returns standard field 21 (race), retaining all wire content.
    #[must_use]
    pub fn race(self) -> Option<&'a Field> {
        self.0.field(21)
    }
}
