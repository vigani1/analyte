//! Holds protocol-neutral identities and evidence without I/O or clinical interpretation.
//!
//! A receipt identifies what arrived. A delivery identifies one immutable payload
//! sent to an app; retries keep that ID. A result identifies an observation and
//! its revision: a correction changes the revision, a rerun the observation.
//! Hosts generate `UUIDv7` IDs once and persist them; this crate validates them.
#![forbid(unsafe_code)]

mod evidence;
mod execution;
mod identity;
mod lifecycle;
mod result;
mod text;
mod value;

pub use execution::{AwosExecution, AwosExecutionConflict, AwosExecutionProjection};
pub use identity::{
    DeliveryId, Digest, IdentityConflict, IdentityStatus, InvalidUuid, NamespacedId,
    ObservationIdentity, ReceiptId, ReceiptMetadata, ResultIdentity, Revision, RevisionExhausted,
};
pub use lifecycle::{
    OrderEvent, OrderIneligible, OrderState, OrderTransition, ProcessingMode, TransitionError,
};
pub use result::{
    CarrierKind, EmptyEvidence, EvidenceList, ListError, NamedSourceTime, NonEmptyEvidenceList,
    OffsetError, QcClass, ResultContent, ResultPart, ResultStatus, SampleIdentity, SamplePosition,
    SampleSource, SourceNode, SourceTime, TimePrecision, UtcOffset,
};
pub use text::{Identifier, Reason, SampleId, SourceText, TextError};
pub use value::{
    Attachment, CodedValue, Comparator, Decimal, DecimalError, DecodedTextError, Dilution,
    DilutionApplied, NumericValue, ResultValue, StructuredNumeric, UnitMapping, UnitMappingError,
    Units, ValueInterpretation,
};

mod departures;
pub use departures::{AppliedDeparture, DepartureEvidence, DepartureMeaning, DepartureSpan};
