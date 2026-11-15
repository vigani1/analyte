//! Independent version-1 evidence types; never serialize model types as the wire contract.
use serde::{Deserialize, Serialize};
use uuid::{Uuid, Variant, Version};

/// Failure to construct bounded source text.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    /// A required identifier or reason is empty.
    #[error("required text is empty")]
    Empty,
    /// The UTF-8 byte limit was exceeded.
    #[error("text exceeds {limit} bytes")]
    TooLong {
        /// Maximum permitted bytes.
        limit: usize,
    },
    /// Identifiers cannot contain control characters.
    #[error("identifier contains a control character")]
    Control,
}

macro_rules! text_type {
    ($name:ident, $limit:expr, $required:expr, $controls:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            /// Copies text exactly after checking its byte bound and character rules.
            ///
            /// # Errors
            /// Returns the violated text constraint; never trims or normalizes.
            pub fn new(value: &str) -> Result<Self, TextError> {
                if $required && value.is_empty() {
                    return Err(TextError::Empty);
                }
                if value.len() > $limit {
                    return Err(TextError::TooLong { limit: $limit });
                }
                if !$controls && value.chars().any(char::is_control) {
                    return Err(TextError::Control);
                }
                Ok(Self(value.to_owned()))
            }
            /// Returns the exact supplied text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str($doc)
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        $name::new(value).map_err(E::custom)
                    }
                }
                // Validate before the model allocates its owned copy. The format
                // reader must also bound its input document before deserializing.
                d.deserialize_str(Visitor)
            }
        }
        impl TryFrom<String> for $name {
            type Error = TextError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(&value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

text_type!(
    SampleId,
    256,
    true,
    false,
    "A sample ID retaining leading zeros and case; at most 256 UTF-8 bytes."
);

text_type!(
    Identifier,
    256,
    true,
    false,
    "An identifier or assigning system, nonempty and at most 256 UTF-8 bytes."
);

text_type!(
    SourceText,
    16384,
    false,
    true,
    "Unmodified source text, including controls, bounded to 16 KiB."
);

text_type!(
    Reason,
    1024,
    true,
    false,
    "An explicit nonempty reason, bounded to 1024 UTF-8 bytes."
);

/// Invalid exact decimal notation (only optional sign, digits and a decimal point).
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("decimal must contain digits and at most one decimal point, within 256 bytes")]
pub struct DecimalError;
/// Exact written decimal, preserving sign, leading zeros and fractional precision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Decimal(String);
impl Decimal {
    /// Accepts plain decimal notation without guessing separators or exponents.
    /// # Errors
    /// Rejects malformed notation and text exceeding 256 bytes.
    pub fn new(text: &str) -> Result<Self, DecimalError> {
        if text.is_empty() || text.len() > 256 {
            return Err(DecimalError);
        }
        let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
        let mut parts = unsigned.split('.');
        let integer = parts.next().ok_or(DecimalError)?;
        let fraction = parts.next();
        if integer.is_empty()
            || !integer.bytes().all(|b| b.is_ascii_digit())
            || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
            || parts.next().is_some()
        {
            return Err(DecimalError);
        }
        Ok(Self(text.to_owned()))
    }

    /// Returns all digits and punctuation exactly as supplied.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of written fractional digits, including trailing zeros.
    #[must_use]
    pub fn scale(&self) -> usize {
        self.0.split_once('.').map_or(0, |(_, f)| f.len())
    }
}

impl TryFrom<String> for Decimal {
    type Error = DecimalError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<Decimal> for String {
    fn from(value: Decimal) -> Self {
        value.0
    }
}

/// A source comparator; no clinical interpretation is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    /// Strict upper bound.
    Less,
    /// Inclusive upper bound.
    LessOrEqual,
    /// Strict lower bound.
    Greater,
    /// Inclusive lower bound.
    GreaterOrEqual,
}

/// An explicitly parsed number, with precision retained in its decimal text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericValue {
    /// Exact number.
    pub number: Decimal,
    /// Bound supplied by the source, if present.
    pub comparator: Option<Comparator>,
}

/// Source value text always survives, even when parsing is unavailable or fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultValue {
    /// Original numeric, qualitative or error token, unchanged.
    pub original: SourceText,
    /// Explicit interpretation by the mapping layer; never inferred here.
    pub parsed: ValueInterpretation,
}

/// An explicit source-to-UCUM mapping, not a claim of terminology validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitMapping {
    /// Exact source unit this mapping applies to.
    pub source: SourceText,
    /// UCUM expression chosen in configuration.
    pub target: Identifier,
}

/// Unit evidence; mapped units can only be constructed using a matching mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UnitWire", into = "UnitWire")]
pub struct Units {
    original: SourceText,
    mapping: Option<UnitMapping>,
}

#[derive(Serialize, Deserialize)]
struct UnitWire {
    original: SourceText,
    mapping: Option<UnitMapping>,
}

/// Mapping source does not match the value's original unit.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("unit mapping source does not match original unit")]
pub struct UnitMappingError;
impl Units {
    /// Retains an unmapped unit.
    #[must_use]
    pub fn original(original: SourceText) -> Self {
        Self {
            original,
            mapping: None,
        }
    }

    /// Applies only an explicitly supplied mapping with an exact source match.
    /// # Errors
    /// Rejects a mapping for another source unit.
    pub fn mapped(original: SourceText, mapping: UnitMapping) -> Result<Self, UnitMappingError> {
        if original != mapping.source {
            return Err(UnitMappingError);
        }
        Ok(Self {
            original,
            mapping: Some(mapping),
        })
    }

    /// Returns the original unit without normalization.
    #[must_use]
    pub fn original_text(&self) -> &SourceText {
        &self.original
    }

    /// Returns mapping evidence, when explicitly applied.
    #[must_use]
    pub fn mapping(&self) -> Option<&UnitMapping> {
        self.mapping.as_ref()
    }
}

impl TryFrom<UnitWire> for Units {
    type Error = UnitMappingError;
    fn try_from(w: UnitWire) -> Result<Self, Self::Error> {
        match w.mapping {
            Some(m) => Self::mapped(w.original, m),
            None => Ok(Self::original(w.original)),
        }
    }
}

impl From<Units> for UnitWire {
    fn from(u: Units) -> Self {
        Self {
            original: u.original,
            mapping: u.mapping,
        }
    }
}

/// Whether the source already multiplied its value by the dilution factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DilutionApplied {
    /// Already multiplied.
    Yes,
    /// Not multiplied.
    No,
    /// Source evidence does not establish this.
    Unknown,
}

/// Dilution evidence; no multiplication is performed by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dilution {
    /// Factor as explicitly reported, if supplied.
    pub factor: Option<Decimal>,
    /// Application evidence, independent of factor availability.
    pub applied: DilutionApplied,
}

/// Explicit value interpretation; unknown types require parking, never empty success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueInterpretation {
    /// Exact decimal with optional comparator.
    Numeric(NumericValue),
    /// Source qualitative, textual or error token; original text remains authoritative.
    Text,
    /// A coded value, retaining code system and display text independently.
    Coded(CodedValue),
    /// Structured number such as a range or ratio; no arithmetic is performed.
    StructuredNumeric(StructuredNumeric),
    /// Encapsulated data retained in its original encoding, without decoding expansion.
    Attachment(Attachment),
    /// Unsupported type; original text is preserved and the result must be parked.
    Unsupported {
        /// Source type code.
        source_type: Identifier,
        /// Specific reason interpretation is unavailable.
        reason: Reason,
    },
}

/// Source code and display label are independent; labels are never identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodedValue {
    /// Code plus assigning code system.
    pub code: NamespacedId,
    /// Display text exactly as reported.
    pub text: SourceText,
}

/// Structured numeric components, preserving the source separator without guessing meaning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredNumeric {
    /// Source comparator if supplied.
    pub comparator: Option<Comparator>,
    /// First exact number when supplied.
    pub first: Option<Decimal>,
    /// Source separator (for example a ratio or range marker).
    pub separator: SourceText,
    /// Second exact number when supplied.
    pub second: Option<Decimal>,
}

/// Bounded encoded attachment evidence; hosts retain raw bytes separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Source media/type identifier.
    pub media_type: Identifier,
    /// Source encoding identifier; no automatic decoding.
    pub encoding: Identifier,
    /// Encoded payload, bounded to the source-text limit.
    pub data: SourceText,
}

impl ValueInterpretation {
    /// Unsupported values must be retained raw and parked, never delivered as success.
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        match self {
            Self::Unsupported { .. } => true,
            Self::Numeric(_)
            | Self::Text
            | Self::Coded(_)
            | Self::StructuredNumeric(_)
            | Self::Attachment(_) => false,
        }
    }
}

/// How a sample identifier reached the analyzer; unknown remains explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleSource {
    /// Read by the analyzer's scanner.
    ScannedByMachine,
    /// Entered at the analyzer.
    TypedOnMachine,
    /// Supplied in a work list.
    WorkList,
    /// Assigned by the analyzer.
    InstrumentAssigned,
    /// Not established by source evidence.
    Unknown,
}

/// Carrier category; position never substitutes for sample identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierKind {
    /// Rack carrier.
    Rack,
    /// Disk carrier.
    Disk,
    /// Tray carrier.
    Tray,
    /// Another explicitly named carrier.
    Other(Identifier),
}

/// Source-reported carrier and slot, including nonnumeric labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplePosition {
    /// Carrier category.
    pub kind: CarrierKind,
    /// Carrier identity when reported.
    pub carrier: Option<Identifier>,
    /// Slot as sent, preserving leading zeros.
    pub slot: Identifier,
}

/// Sample identity with provenance and optional physical position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleIdentity {
    /// String ID, never converted to a number.
    pub id: SampleId,
    /// Origin of this ID.
    pub source: SampleSource,
    /// Physical position when reported.
    pub position: Option<SamplePosition>,
}

/// Source classification only; no QC decision is made here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QcClass {
    /// Patient specimen.
    Patient,
    /// Control material.
    Control,
    /// Calibration material.
    Calibration,
    /// Not classified by the source.
    Unknown,
}

/// Analyte summary, retained beside the source's own code. The summary never
/// replaces the source status; it only groups source codes with one meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    /// Not yet available, for example in process on the instrument.
    Pending,
    /// Preliminary observation.
    Preliminary,
    /// Final observation.
    Final,
    /// Corrected revision of an earlier result.
    Corrected,
    /// An earlier result is withdrawn and must no longer be used (for example
    /// HL7 OBX-11 D or W); the raw history is kept.
    Withdrawn,
    /// The test could not be performed (for example HL7 X, ASTM X).
    NotPerformed,
    /// Source status not mapped to a summary.
    Unknown,
}

/// Written time precision; the original text retains fractional digits exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimePrecision {
    /// Year only.
    Year,
    /// Through month.
    Month,
    /// Through day.
    Day,
    /// Through hour.
    Hour,
    /// Through minute.
    Minute,
    /// Through second.
    Second,
    /// Subsecond digits, retained in original text.
    FractionalSecond,
    /// Precision could not be established.
    Unknown,
}

/// Source time evidence without inventing a timezone for a partial/local time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTime {
    /// Time exactly as reported.
    pub original: SourceText,
    /// Parsed local date/time if available; never supplies an implied offset.
    pub parsed: Option<jiff::civil::DateTime>,
    /// Explicit source offset if supplied.
    pub offset: Option<UtcOffset>,
    /// Written precision.
    pub precision: TimePrecision,
}

/// A bounded sequence, capped at 256 entries including during deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EvidenceList<T>(Vec<T>);
/// A source supplied too many evidence entries.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("evidence list exceeds 256 entries")]
pub struct ListError;
impl<T> EvidenceList<T> {
    /// Accepts at most 256 entries.
    /// # Errors
    /// Rejects a longer list.
    pub fn new(entries: Vec<T>) -> Result<Self, ListError> {
        if entries.len() > 256 {
            return Err(ListError);
        }
        Ok(Self(entries))
    }

    /// Borrows entries without permitting unbounded mutation.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for EvidenceList<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = EvidenceList<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("at most 256 evidence entries")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = seq.next_element()? {
                    if entries.len() == 256 {
                        return Err(serde::de::Error::custom(ListError));
                    }
                    entries.push(entry);
                }
                Ok(EvidenceList(entries))
            }
        }
        d.deserialize_seq(Visitor(std::marker::PhantomData))
    }
}

/// One source hierarchy node; the flat path avoids recursive untrusted trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceNode {
    /// One-based record or segment position in the source message.
    pub occurrence: u64,
    /// Source record/segment category, unchanged.
    pub kind: Identifier,
    /// Source sequence identifier when supplied.
    pub sequence: Option<SourceText>,
    /// Source status at this level when supplied.
    pub status: Option<SourceText>,
}

/// A result retains source evidence beside summaries, without clinical decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultPart {
    /// Test code with assigning system.
    pub test: NamespacedId,
    /// Original and optional parsed value.
    pub values: EvidenceList<ResultValue>,
    /// Units if present.
    pub units: Option<Units>,
    /// Dilution evidence if present.
    pub dilution: Option<Dilution>,
    /// Source flags, unchanged.
    pub flags: EvidenceList<SourceText>,
    /// Source reference range, uninterpreted.
    pub reference_range: Option<SourceText>,
    /// Source comments, unchanged.
    pub comments: EvidenceList<SourceText>,
    /// Source material classification.
    pub qc_class: QcClass,
    /// Original result status code.
    pub source_status: SourceText,
    /// Analyte summary, never replacing the source status.
    pub status: ResultStatus,
    /// Ordered path from source message to result, preserving hierarchy.
    pub source_path: EvidenceList<SourceNode>,
    /// Named source times (collection, analysis, etc.).
    pub times: EvidenceList<NamedSourceTime>,
    /// Host receive instant, independent of machine clock.
    pub received_at: jiff::Timestamp,
}

/// A time with its source-defined role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedSourceTime {
    /// Source role rather than a guessed clinical meaning.
    pub role: Identifier,
    /// Source time evidence.
    pub time: SourceTime,
}

/// Source UTC offset in seconds, bounded by Jiff's supported offset range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "i32", into = "i32")]
pub struct UtcOffset(i32);
/// Offset is outside the supported civil-time range.
#[derive(Debug, thiserror::Error)]
#[error("UTC offset is outside -25:59:59 through +25:59:59")]
pub struct OffsetError;
impl TryFrom<i32> for UtcOffset {
    type Error = OffsetError;
    fn try_from(seconds: i32) -> Result<Self, Self::Error> {
        jiff::tz::Offset::from_seconds(seconds).map_err(|_| OffsetError)?;
        Ok(Self(seconds))
    }
}

impl From<UtcOffset> for i32 {
    fn from(offset: UtcOffset) -> Self {
        offset.0
    }
}

/// One replacement unit can contain several source segments and repeated values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultDocument {
    /// The receipt whose raw bytes this result was read from.
    pub receipt: ReceiptId,
    /// The sample the machine reported; absence requires operator parking.
    pub sample: Option<SampleIdentity>,
    /// Processing mode of the source message; only production may be delivered.
    pub processing: ProcessingMode,
    /// Identity may resolve even if a correction arrives before its original.
    pub identity: IdentityStatus,
    /// Source parts in their original order; no scalar flattening or deduplication.
    pub parts: NonEmptyEvidenceList<ResultPart>,
}

/// A replacement unit must contain source evidence, even when its value is unsupported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyEvidenceList<T>(EvidenceList<T>);
/// Replacement evidence cannot be absent.
#[derive(Debug, thiserror::Error)]
#[error("replacement unit has no source parts")]
pub struct EmptyEvidence;
impl<T> NonEmptyEvidenceList<T> {
    /// Requires at least one entry in an already bounded list.
    /// # Errors
    /// Rejects an empty replacement unit.
    pub fn new(entries: EvidenceList<T>) -> Result<Self, EmptyEvidence> {
        if entries.as_slice().is_empty() {
            return Err(EmptyEvidence);
        }
        Ok(Self(entries))
    }

    /// Returns source parts in order, retaining repeated content.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyEvidenceList<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(EvidenceList::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// A supplied UUID is not an RFC `UUIDv7`.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("identity must be an RFC `UUIDv7`")]
pub struct InvalidUuid;
macro_rules! uuid_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "Uuid", into = "Uuid")]
        pub struct $name(Uuid);
        impl TryFrom<Uuid> for $name {
            type Error = InvalidUuid;
            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                if value.get_version() != Some(Version::SortRand)
                    || value.get_variant() != Variant::RFC4122
                {
                    return Err(InvalidUuid);
                }
                Ok(Self(value))
            }
        }
        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

uuid_id!(
    ReceiptId,
    "A host-created `UUIDv7` identifying one receipt, retained on reprocessing."
);

uuid_id!(
    DeliveryId,
    "A host-created `UUIDv7` identifying immutable delivery bytes, retained on retries."
);

/// An identifier with its assigning system; neither part can be empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespacedId {
    /// Assigning system, kept exactly.
    pub namespace: Identifier,
    /// Value assigned by that system.
    pub value: Identifier,
}

/// A trusted observation and its Analyte revision; reruns use a new observation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResultIdentity {
    /// Stable observation key, never derived from a value, ordinal or test name.
    pub observation: ObservationIdentity,
    /// Revision assigned by Analyte, independently of source action/status.
    pub revision: Revision,
}

/// Resolution evidence; missing evidence never becomes a guessed result key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatus {
    /// Sufficient evidence identifies the observation and revision.
    Resolved(ResultIdentity),
    /// Operator reconciliation is required.
    Unresolved(Reason),
}

/// Analyte-assigned revision number; zero is a valid first revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Revision(pub u64);
/// Observation key from source identity evidence, with namespaced order context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservationIdentity {
    /// Observation code and code system, never a display/test name.
    pub code: NamespacedId,
    /// Source sub-ID; empty is distinct from unavailable identity evidence.
    pub sub_id: SourceText,
    /// Source run evidence (for example a completion time or run counter);
    /// empty when the machine gives none, in which case a rerun cannot be told
    /// from a retransmission and differing content is parked as a conflict.
    pub run: SourceText,
    /// Namespaced order identity; missing context requires unresolved status.
    pub order: NamespacedId,
}

/// Mode is explicit so training and debugging cannot silently become production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingMode {
    /// Eligible for production delivery.
    Production,
    /// Training evidence only.
    Training,
    /// Debugging evidence only.
    Debug,
    /// Quality control evidence only.
    QualityControl,
}

impl ProcessingMode {
    /// Only production mode is eligible for production delivery.
    #[must_use]
    pub fn may_deliver_to_production(self) -> bool {
        self == Self::Production
    }
}

/// Order progress; persistence and machine commands remain the host's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    /// Requested by the app, not sent.
    Requested,
    /// Sent, awaiting machine acceptance.
    Sent,
    /// Machine explicitly accepted.
    Accepted,
    /// Machine reported completion.
    Done,
    /// Explicitly rejected.
    Rejected,
    /// Cancellation requested, not yet confirmed.
    CancelRequested,
    /// Cancellation explicitly confirmed.
    CancelConfirmed,
    /// Send outcome unknown; reconciliation required, never automatic resend.
    Uncertain,
}

mod conversion;
