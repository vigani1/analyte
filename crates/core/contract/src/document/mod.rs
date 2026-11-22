//! Holds independent version-1 document types without serializing model types.
mod text;
pub use text::{Identifier, Reason, SampleId, SourceText, TextError};
mod value;
pub use value::{
    Attachment, CodedValue, Comparator, Decimal, DecimalError, Dilution, DilutionApplied,
    NumericValue, ResultValue, StructuredNumeric, UnitMapping, UnitMappingError, Units,
    ValueInterpretation,
};
mod sample;
pub use sample::{CarrierKind, SampleIdentity, SamplePosition, SampleSource};
mod result;
pub use result::{QcClass, ResultDocument, ResultPart, ResultStatus, SourceNode};
mod time;
pub use time::{NamedSourceTime, OffsetError, SourceTime, TimePrecision, UtcOffset};
mod list;
pub use list::{EmptyEvidence, EvidenceList, ListError, NonEmptyEvidenceList};
mod conversion;
mod identity;
pub use identity::{
    DeliveryId, IdentityStatus, InvalidModel, InvalidUuid, ModelId, NamespacedId,
    ObservationIdentity, ReceiptId, ResultIdentity, Revision,
};
mod lifecycle;
pub use lifecycle::{OrderState, ProcessingMode};
