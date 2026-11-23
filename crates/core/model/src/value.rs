//! Preserves exact result values and explicit unit mappings.
use crate::SourceText;
use serde::{Deserialize, Serialize};

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
#[serde(try_from = "ResultValueWire")]
pub struct ResultValue {
    /// Original numeric, qualitative or error token, unchanged.
    pub original: SourceText,
    /// Original reference text specific to this value form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<SourceText>,
    /// Fully decoded protocol-neutral text, only when all source encoding is understood.
    /// The original remains authoritative; absence never means an empty decoded value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decoded_text: Option<SourceText>,
    /// Explicit interpretation by the mapping layer; never inferred here.
    parsed: ValueInterpretation,
}

/// Decoded text is valid only for a fully interpreted textual value.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("decoded text requires a text interpretation")]
pub struct DecodedTextError;

impl ResultValue {
    /// Retains an interpretation without decoded text.
    #[must_use]
    pub fn new(original: SourceText, parsed: ValueInterpretation) -> Self {
        Self {
            original,
            reference: None,
            parsed,
            decoded_text: None,
        }
    }
    /// Retains fully decoded text with an explicitly textual interpretation.
    #[must_use]
    pub fn text(original: SourceText, decoded_text: Option<SourceText>) -> Self {
        Self {
            original,
            reference: None,
            decoded_text,
            parsed: ValueInterpretation::Text,
        }
    }
    /// Returns the explicit interpretation without permitting mutation.
    #[must_use]
    pub fn parsed(&self) -> &ValueInterpretation {
        &self.parsed
    }
    /// Returns decoded text only for textual interpretations.
    #[must_use]
    pub fn decoded_text(&self) -> Option<&SourceText> {
        self.decoded_text.as_ref()
    }
}

#[derive(Deserialize)]
struct ResultValueWire {
    original: SourceText,
    reference: Option<SourceText>,
    decoded_text: Option<SourceText>,
    parsed: ValueInterpretation,
}
impl TryFrom<ResultValueWire> for ResultValue {
    type Error = DecodedTextError;
    fn try_from(value: ResultValueWire) -> Result<Self, Self::Error> {
        if value.decoded_text.is_some() && !matches!(value.parsed, ValueInterpretation::Text) {
            return Err(DecodedTextError);
        }
        Ok(Self {
            original: value.original,
            reference: value.reference,
            decoded_text: value.decoded_text,
            parsed: value.parsed,
        })
    }
}

/// An explicit source-to-UCUM mapping, not a claim of terminology validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitMapping {
    /// Exact source unit this mapping applies to.
    pub source: SourceText,
    /// UCUM expression chosen in configuration.
    pub target: crate::Identifier,
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
    /// Dilution text exactly as supplied, before any optional interpretation.
    pub original: Option<SourceText>,
    /// Factor as explicitly reported, if supplied.
    pub factor: Option<Decimal>,
    /// Application evidence, independent of factor availability.
    pub applied: DilutionApplied,
}

/// Explicit value interpretation; unknown types require parking, never empty success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Source explicitly reports that no result was available.
    Absent,
    /// Qualitative reaction explicitly coded by the source.
    Qualitative(QualitativeReaction),
    /// Unsupported type; original text is preserved and the result must be parked.
    Unsupported {
        /// Source type code.
        source_type: crate::Identifier,
        /// Specific reason interpretation is unavailable.
        reason: crate::Reason,
    },
}

/// Qualitative response written by the analyzer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualitativeReaction {
    /// Source code indicates a negative reaction.
    Negative,
    /// Source code indicates a borderline reaction.
    Borderline,
    /// Source code indicates a positive reaction.
    Positive,
}

/// Source code and display label are independent; labels are never identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodedValue {
    /// Code plus assigning code system.
    pub code: crate::NamespacedId,
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
    pub media_type: crate::Identifier,
    /// Source encoding identifier; no automatic decoding.
    pub encoding: crate::Identifier,
    /// Encoded payload, bounded to the source-text limit.
    pub data: SourceText,
}

impl ValueInterpretation {
    /// Unsupported values must be retained raw and parked, never delivered as success.
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        match self {
            Self::Unsupported { .. } => true,
            Self::Absent
            | Self::Qualitative(_)
            | Self::Numeric(_)
            | Self::Text
            | Self::Coded(_)
            | Self::StructuredNumeric(_)
            | Self::Attachment(_) => false,
        }
    }
}
