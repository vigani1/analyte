//! Read-only order record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 order positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Order<'a>(&'a Record);
impl<'a> Order<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Order).then_some(Self(record))
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
    /// Returns standard field 3 (specimen id), retaining all wire content.
    #[must_use]
    pub fn specimen_id(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (instrument specimen id), retaining all wire content.
    #[must_use]
    pub fn instrument_specimen_id(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (test id), retaining all wire content.
    #[must_use]
    pub fn test_id(self) -> Option<&'a Field> {
        self.0.field(5)
    }
    /// Returns standard field 6 (priority), retaining all wire content.
    #[must_use]
    pub fn priority(self) -> Option<&'a Field> {
        self.0.field(6)
    }
    /// Returns standard field 7 (ordered at), retaining all wire content.
    #[must_use]
    pub fn ordered_at(self) -> Option<&'a Field> {
        self.0.field(7)
    }
    /// Returns standard field 8 (collected at), retaining all wire content.
    #[must_use]
    pub fn collected_at(self) -> Option<&'a Field> {
        self.0.field(8)
    }
    /// Returns standard field 9 (collection end), retaining all wire content.
    #[must_use]
    pub fn collection_end(self) -> Option<&'a Field> {
        self.0.field(9)
    }
    /// Returns standard field 10 (volume), retaining all wire content.
    #[must_use]
    pub fn volume(self) -> Option<&'a Field> {
        self.0.field(10)
    }
    /// Returns standard field 11 (collector), retaining all wire content.
    #[must_use]
    pub fn collector(self) -> Option<&'a Field> {
        self.0.field(11)
    }
    /// Returns standard field 12 (action), retaining all wire content.
    #[must_use]
    pub fn action(self) -> Option<&'a Field> {
        self.0.field(12)
    }
    /// Returns standard field 13 (hazard), retaining all wire content.
    #[must_use]
    pub fn hazard(self) -> Option<&'a Field> {
        self.0.field(13)
    }
    /// Returns standard field 14 (clinical information), retaining all wire content.
    #[must_use]
    pub fn clinical_information(self) -> Option<&'a Field> {
        self.0.field(14)
    }
    /// Returns standard field 15 (received at), retaining all wire content.
    #[must_use]
    pub fn received_at(self) -> Option<&'a Field> {
        self.0.field(15)
    }
    /// Returns standard field 16 (specimen descriptor), retaining all wire content.
    #[must_use]
    pub fn specimen_descriptor(self) -> Option<&'a Field> {
        self.0.field(16)
    }
    /// Returns standard field 17 (physician), retaining all wire content.
    #[must_use]
    pub fn physician(self) -> Option<&'a Field> {
        self.0.field(17)
    }
    /// Returns standard field 18 (phone), retaining all wire content.
    #[must_use]
    pub fn phone(self) -> Option<&'a Field> {
        self.0.field(18)
    }
    /// Returns standard field 19 (user 1), retaining all wire content.
    #[must_use]
    pub fn user_1(self) -> Option<&'a Field> {
        self.0.field(19)
    }
    /// Returns standard field 20 (user 2), retaining all wire content.
    #[must_use]
    pub fn user_2(self) -> Option<&'a Field> {
        self.0.field(20)
    }
    /// Returns standard field 21 (laboratory 1), retaining all wire content.
    #[must_use]
    pub fn laboratory_1(self) -> Option<&'a Field> {
        self.0.field(21)
    }
    /// Returns standard field 22 (laboratory 2), retaining all wire content.
    #[must_use]
    pub fn laboratory_2(self) -> Option<&'a Field> {
        self.0.field(22)
    }
    /// Returns standard field 23 (reported at), retaining all wire content.
    #[must_use]
    pub fn reported_at(self) -> Option<&'a Field> {
        self.0.field(23)
    }
    /// Returns standard field 24 (charge), retaining all wire content.
    #[must_use]
    pub fn charge(self) -> Option<&'a Field> {
        self.0.field(24)
    }
    /// Returns standard field 25 (section), retaining all wire content.
    #[must_use]
    pub fn section(self) -> Option<&'a Field> {
        self.0.field(25)
    }
    /// Returns standard field 26 (report type), retaining all wire content.
    #[must_use]
    pub fn report_type(self) -> Option<&'a Field> {
        self.0.field(26)
    }
    /// Returns standard field 27 (reserved), retaining all wire content.
    #[must_use]
    pub fn reserved(self) -> Option<&'a Field> {
        self.0.field(27)
    }
    /// Returns standard field 28 (collection location), retaining all wire content.
    #[must_use]
    pub fn collection_location(self) -> Option<&'a Field> {
        self.0.field(28)
    }
    /// Returns standard field 29 (infection), retaining all wire content.
    #[must_use]
    pub fn infection(self) -> Option<&'a Field> {
        self.0.field(29)
    }
    /// Returns standard field 30 (service), retaining all wire content.
    #[must_use]
    pub fn service(self) -> Option<&'a Field> {
        self.0.field(30)
    }
    /// Returns standard field 31 (institution), retaining all wire content.
    #[must_use]
    pub fn institution(self) -> Option<&'a Field> {
        self.0.field(31)
    }
}
