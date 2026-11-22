//! Defines version-1 time evidence types and validates their wire values.
use super::{Identifier, SourceText};
use serde::{Deserialize, Serialize};
/// Written time precision; the original text retains fractional digits exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceTime {
    /// Time exactly as reported.
    pub original: SourceText,
    /// Parsed local date/time if available; never supplies an implied offset.
    #[schemars(with = "Option<crate::schema::LocalDateTime>")]
    pub parsed: Option<jiff::civil::DateTime>,
    /// Explicit source offset if supplied.
    pub offset: Option<UtcOffset>,
    /// Written precision.
    pub precision: TimePrecision,
}

/// A time with its source-defined role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
impl schemars::JsonSchema for UtcOffset {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UtcOffset".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "integer",
            "minimum": -93599,
            "maximum": 93599,
            "description": "A UTC offset in seconds, -25:59:59 through +25:59:59.",
        })
    }
}
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

impl From<analyte_model::UtcOffset> for UtcOffset {
    fn from(v: analyte_model::UtcOffset) -> Self {
        Self(i32::from(v))
    }
}
