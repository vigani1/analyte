//! Read-only patient record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 patient positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Patient<'a>(&'a Record);
impl<'a> Patient<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Patient).then_some(Self(record))
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
    /// Returns standard field 3 (practice id), retaining all wire content.
    #[must_use]
    pub fn practice_id(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (laboratory id), retaining all wire content.
    #[must_use]
    pub fn laboratory_id(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (third id), retaining all wire content.
    #[must_use]
    pub fn third_id(self) -> Option<&'a Field> {
        self.0.field(5)
    }
    /// Returns standard field 6 (name), retaining all wire content.
    #[must_use]
    pub fn name(self) -> Option<&'a Field> {
        self.0.field(6)
    }
    /// Returns standard field 7 (maiden name), retaining all wire content.
    #[must_use]
    pub fn maiden_name(self) -> Option<&'a Field> {
        self.0.field(7)
    }
    /// Returns standard field 8 (birthdate), retaining all wire content.
    #[must_use]
    pub fn birthdate(self) -> Option<&'a Field> {
        self.0.field(8)
    }
    /// Returns standard field 9 (sex), retaining all wire content.
    #[must_use]
    pub fn sex(self) -> Option<&'a Field> {
        self.0.field(9)
    }
    /// Returns standard field 10 (race), retaining all wire content.
    #[must_use]
    pub fn race(self) -> Option<&'a Field> {
        self.0.field(10)
    }
    /// Returns standard field 11 (address), retaining all wire content.
    #[must_use]
    pub fn address(self) -> Option<&'a Field> {
        self.0.field(11)
    }
    /// Returns standard field 12 (reserved), retaining all wire content.
    #[must_use]
    pub fn reserved(self) -> Option<&'a Field> {
        self.0.field(12)
    }
    /// Returns standard field 13 (phone), retaining all wire content.
    #[must_use]
    pub fn phone(self) -> Option<&'a Field> {
        self.0.field(13)
    }
    /// Returns standard field 14 (physician), retaining all wire content.
    #[must_use]
    pub fn physician(self) -> Option<&'a Field> {
        self.0.field(14)
    }
    /// Returns standard field 15 (special 1), retaining all wire content.
    #[must_use]
    pub fn special_1(self) -> Option<&'a Field> {
        self.0.field(15)
    }
    /// Returns standard field 16 (special 2), retaining all wire content.
    #[must_use]
    pub fn special_2(self) -> Option<&'a Field> {
        self.0.field(16)
    }
    /// Returns standard field 17 (height), retaining all wire content.
    #[must_use]
    pub fn height(self) -> Option<&'a Field> {
        self.0.field(17)
    }
    /// Returns standard field 18 (weight), retaining all wire content.
    #[must_use]
    pub fn weight(self) -> Option<&'a Field> {
        self.0.field(18)
    }
    /// Returns standard field 19 (diagnosis), retaining all wire content.
    #[must_use]
    pub fn diagnosis(self) -> Option<&'a Field> {
        self.0.field(19)
    }
    /// Returns standard field 20 (medications), retaining all wire content.
    #[must_use]
    pub fn medications(self) -> Option<&'a Field> {
        self.0.field(20)
    }
    /// Returns standard field 21 (diet), retaining all wire content.
    #[must_use]
    pub fn diet(self) -> Option<&'a Field> {
        self.0.field(21)
    }
    /// Returns standard field 22 (practice 1), retaining all wire content.
    #[must_use]
    pub fn practice_1(self) -> Option<&'a Field> {
        self.0.field(22)
    }
    /// Returns standard field 23 (practice 2), retaining all wire content.
    #[must_use]
    pub fn practice_2(self) -> Option<&'a Field> {
        self.0.field(23)
    }
    /// Returns standard field 24 (admission dates), retaining all wire content.
    #[must_use]
    pub fn admission_dates(self) -> Option<&'a Field> {
        self.0.field(24)
    }
    /// Returns standard field 25 (admission status), retaining all wire content.
    #[must_use]
    pub fn admission_status(self) -> Option<&'a Field> {
        self.0.field(25)
    }
    /// Returns standard field 26 (location), retaining all wire content.
    #[must_use]
    pub fn location(self) -> Option<&'a Field> {
        self.0.field(26)
    }
    /// Returns standard field 27 (diagnosis scheme), retaining all wire content.
    #[must_use]
    pub fn diagnosis_scheme(self) -> Option<&'a Field> {
        self.0.field(27)
    }
    /// Returns standard field 28 (diagnosis codes), retaining all wire content.
    #[must_use]
    pub fn diagnosis_codes(self) -> Option<&'a Field> {
        self.0.field(28)
    }
    /// Returns standard field 29 (religion), retaining all wire content.
    #[must_use]
    pub fn religion(self) -> Option<&'a Field> {
        self.0.field(29)
    }
    /// Returns standard field 30 (marital status), retaining all wire content.
    #[must_use]
    pub fn marital_status(self) -> Option<&'a Field> {
        self.0.field(30)
    }
    /// Returns standard field 31 (isolation), retaining all wire content.
    #[must_use]
    pub fn isolation(self) -> Option<&'a Field> {
        self.0.field(31)
    }
    /// Returns standard field 32 (language), retaining all wire content.
    #[must_use]
    pub fn language(self) -> Option<&'a Field> {
        self.0.field(32)
    }
    /// Returns standard field 33 (service), retaining all wire content.
    #[must_use]
    pub fn service(self) -> Option<&'a Field> {
        self.0.field(33)
    }
    /// Returns standard field 34 (institution), retaining all wire content.
    #[must_use]
    pub fn institution(self) -> Option<&'a Field> {
        self.0.field(34)
    }
    /// Returns standard field 35 (dosage category), retaining all wire content.
    #[must_use]
    pub fn dosage_category(self) -> Option<&'a Field> {
        self.0.field(35)
    }
}
