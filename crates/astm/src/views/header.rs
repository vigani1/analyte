//! Read-only header record positions belong beside the ASTM grammar.
use crate::{Field, Kind, Record};
/// Read-only LIS02 header positions; no clinical defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct Header<'a>(&'a Record);
impl<'a> Header<'a> {
    /// Returns a view only for the matching validated record type.
    #[must_use]
    pub fn new(record: &'a Record) -> Option<Self> {
        (record.kind() == Kind::Header).then_some(Self(record))
    }
    /// Preserves access to unknown and manufacturer-defined positions.
    #[must_use]
    pub fn field(self, position: usize) -> Option<&'a Field> {
        self.0.field(position)
    }
    /// Returns standard field 2 (delimiter definition), retaining all wire content.
    #[must_use]
    pub fn delimiter_definition(self) -> Option<&'a Field> {
        self.0.field(2)
    }
    /// Returns standard field 3 (control id), retaining all wire content.
    #[must_use]
    pub fn control_id(self) -> Option<&'a Field> {
        self.0.field(3)
    }
    /// Returns standard field 4 (password), retaining all wire content.
    #[must_use]
    pub fn password(self) -> Option<&'a Field> {
        self.0.field(4)
    }
    /// Returns standard field 5 (sender), retaining all wire content.
    #[must_use]
    pub fn sender(self) -> Option<&'a Field> {
        self.0.field(5)
    }
    /// Returns standard field 6 (address), retaining all wire content.
    #[must_use]
    pub fn address(self) -> Option<&'a Field> {
        self.0.field(6)
    }
    /// Returns standard field 7 (reserved), retaining all wire content.
    #[must_use]
    pub fn reserved(self) -> Option<&'a Field> {
        self.0.field(7)
    }
    /// Returns standard field 8 (phone), retaining all wire content.
    #[must_use]
    pub fn phone(self) -> Option<&'a Field> {
        self.0.field(8)
    }
    /// Returns standard field 9 (characteristics), retaining all wire content.
    #[must_use]
    pub fn characteristics(self) -> Option<&'a Field> {
        self.0.field(9)
    }
    /// Returns standard field 10 (receiver), retaining all wire content.
    #[must_use]
    pub fn receiver(self) -> Option<&'a Field> {
        self.0.field(10)
    }
    /// Returns standard field 11 (instructions), retaining all wire content.
    #[must_use]
    pub fn instructions(self) -> Option<&'a Field> {
        self.0.field(11)
    }
    /// Returns standard field 12 (processing id), retaining all wire content.
    #[must_use]
    pub fn processing_id(self) -> Option<&'a Field> {
        self.0.field(12)
    }
    /// Returns standard field 13 (version), retaining all wire content.
    #[must_use]
    pub fn version(self) -> Option<&'a Field> {
        self.0.field(13)
    }
    /// Returns standard field 14 (timestamp), retaining all wire content.
    #[must_use]
    pub fn timestamp(self) -> Option<&'a Field> {
        self.0.field(14)
    }
}
impl Header<'_> {
    /// Recognizes the processing code while preserving unknown wire values.
    #[must_use]
    pub fn processing_code(self) -> Option<crate::ProcessingId> {
        self.processing_id()
            .map(|f| crate::ProcessingId::from_bytes(f.raw()))
    }
}
