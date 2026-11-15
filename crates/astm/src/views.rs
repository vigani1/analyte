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

impl Header<'_> {
    /// Recognizes the processing code while preserving unknown wire values.
    #[must_use]
    pub fn processing_code(self) -> Option<crate::ProcessingId> {
        self.processing_id()
            .map(|f| crate::ProcessingId::from_bytes(f.raw()))
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
impl Terminator<'_> {
    /// Recognizes completion reasons without conflating unknown codes with success.
    #[must_use]
    pub fn termination_code(self) -> Option<crate::TerminationCode> {
        self.code()
            .map(|f| crate::TerminationCode::from_bytes(f.raw()))
    }
}
