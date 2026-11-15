//! POCT01-A2 Appendix B section 8 values, retaining source spelling.
use crate::Element;

/// Invalid data type content; no source payload is included in the error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DataError {
    /// The required value attribute is absent.
    #[error("required value attribute is missing")]
    MissingValue,
    /// A null conflicts with a concrete value or has an unknown code.
    #[error("invalid null representation")]
    Null,
    /// A quantity has no unit.
    #[error("quantity requires a unit")]
    MissingUnit,
    /// A number is not legal POCT numeric text.
    #[error("invalid numeric representation")]
    Number,
    /// A timestamp is malformed or has an invalid calendar component.
    #[error("invalid timestamp representation")]
    Time,
    /// A coded value has an invalid code or coding-system qualification.
    #[error("invalid coded value")]
    Code,
    /// The attribute is not defined for this data type.
    #[error("unsupported data type attribute")]
    Attribute,
    /// The interval literal does not have two valid bounds.
    #[error("invalid interval representation")]
    Interval,
}

/// An explicit POCT null; absence of an element is a separate condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Null {
    /// No information.
    NoInformation,
    /// Not applicable.
    NotApplicable,
    /// Unknown.
    Unknown,
    /// Not asked.
    NotAsked,
    /// Asked but unknown.
    AskedUnknown,
    /// Not available.
    NotAvailable,
    /// Other.
    Other,
    /// Positive infinity.
    PositiveInfinity,
    /// Negative infinity.
    NegativeInfinity,
}

impl Null {
    /// Returns the source-defined XML null code without changing its meaning.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::NoInformation => "NI",
            Self::NotApplicable => "NA",
            Self::Unknown => "UNK",
            Self::NotAsked => "NASK",
            Self::AskedUnknown => "ASKU",
            Self::NotAvailable => "NAV",
            Self::Other => "OTH",
            Self::PositiveInfinity => "PINF",
            Self::NegativeInfinity => "NINF",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, DataError> {
        match value {
            "NI" => Ok(Self::NoInformation),
            "NA" => Ok(Self::NotApplicable),
            "UNK" => Ok(Self::Unknown),
            "NASK" => Ok(Self::NotAsked),
            "ASKU" => Ok(Self::AskedUnknown),
            "NAV" => Ok(Self::NotAvailable),
            "OTH" => Ok(Self::Other),
            "PINF" => Ok(Self::PositiveInfinity),
            "NINF" => Ok(Self::NegativeInfinity),
            _ => Err(DataError::Null),
        }
    }
}

/// A concrete value or an explicit source null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nullable<T> {
    /// Value explicitly supplied by the device.
    Value(T),
    /// Explicit null supplied by the device.
    Null(Null),
}

/// A coded value with the source system and display text, without terminology guesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// Original code.
    pub value: String,
    /// Source display label.
    pub display: Option<String>,
    /// Coding-system OID when present.
    pub system: Option<String>,
    /// Source coding-system name; absence denotes the POCT default.
    pub system_name: Option<String>,
    /// Source coding-system version.
    pub version: Option<String>,
}

/// Quantity preserving numeric precision and the original unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quantity {
    /// Exact written value; never converted to binary floating point.
    pub value: String,
    /// Original unit. No implicit UCUM mapping is performed.
    pub unit: String,
}

/// A source timestamp with explicit precision and an optional supplied offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp {
    original: String,
    offset_seconds: Option<i32>,
}

impl Timestamp {
    /// Checks POCT separated, possibly truncated date/time notation.
    /// # Errors
    /// Rejects invalid dates, invalid offsets and malformed precision.
    pub fn parse(value: &str) -> Result<Self, DataError> {
        if value.len() > 256 {
            return Err(DataError::Time);
        }
        let split = value.char_indices().find(|(i, c)| {
            *i >= 4 && matches!(c, '+' | '-') && value.get(..*i).is_some_and(|s| s.contains('T'))
        });
        let (local, offset_seconds) = if let Some((i, _)) = split {
            let local = value.get(..i).ok_or(DataError::Time)?;
            let offset = value.get(i..).ok_or(DataError::Time)?;
            (local, Some(parse_offset(offset)?))
        } else {
            (value, None)
        };
        let (date, time) = local
            .split_once('T')
            .map_or((local, None), |(d, t)| (d, Some(t)));
        let parts: Vec<&str> = date.split('-').collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err(DataError::Time);
        }
        let year_text = parts.first().ok_or(DataError::Time)?;
        if year_text.len() != 4 || !year_text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(DataError::Time);
        }
        let year = year_text.parse::<i16>().map_err(|_| DataError::Time)?;
        let month = date_component(parts.get(1).copied(), 1)?;
        let day = date_component(parts.get(2).copied(), 1)?;
        jiff::civil::Date::new(year, month, day).map_err(|_| DataError::Time)?;
        if let Some(time) = time {
            if parts.len() != 3 {
                return Err(DataError::Time);
            }
            let parts: Vec<&str> = time.split(':').collect();
            if parts.is_empty() || parts.len() > 3 {
                return Err(DataError::Time);
            }
            let hour = date_component(parts.first().copied(), 0)?;
            let minute = date_component(parts.get(1).copied(), 0)?;
            let (second, fraction) = parts
                .get(2)
                .copied()
                .unwrap_or("00")
                .split_once('.')
                .map_or((parts.get(2).copied().unwrap_or("00"), None), |(s, f)| {
                    (s, Some(f))
                });
            let second = date_component(Some(second), 0)?;
            if fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit())) {
                return Err(DataError::Time);
            }
            jiff::civil::Time::new(hour, minute, second, 0).map_err(|_| DataError::Time)?;
        }
        Ok(Self {
            original: value.to_owned(),
            offset_seconds,
        })
    }

    /// Exact source spelling, including fractional precision.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Offset explicitly supplied by the source; never inferred from the host.
    #[must_use]
    pub fn offset_seconds(&self) -> Option<i32> {
        self.offset_seconds
    }
}

fn date_component(text: Option<&str>, default: i8) -> Result<i8, DataError> {
    let Some(text) = text else {
        return Ok(default);
    };
    if text.len() != 2 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DataError::Time);
    }
    text.parse().map_err(|_| DataError::Time)
}

/// A typed field. Additional source attributes remain in the original document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// String, organization or formatted person name.
    Text(Nullable<String>),
    /// Coded value or preferred coding of a coded-with-equivalents field.
    Code(Nullable<Code>),
    /// Quantity with an explicit original unit.
    Quantity(Nullable<Quantity>),
    /// Timestamp with original precision and offset evidence.
    Time(Nullable<Timestamp>),
    /// Integer notation, retained without narrowing to a machine integer.
    Integer(Nullable<String>),
    /// Real notation, retained without loss of written precision.
    Real(Nullable<String>),
    /// Interval notation with optional original unit.
    Interval(Nullable<(String, Option<String>)>),
    /// Encapsulated text; compression and encoding remain in the document.
    Encapsulated(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    Code,
    SimpleCode,
    Quantity,
    Time,
    Integer,
    Real,
    Interval,
    Encapsulated,
    Person,
}

pub(crate) fn attr<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

pub(crate) fn parse(element: &Element, kind: Kind) -> Result<Value, DataError> {
    validate_attributes(element, kind)?;
    if kind == Kind::Encapsulated {
        return Ok(Value::Encapsulated(element.text().to_owned()));
    }
    let null = attr(element, "NULL").map(Null::parse).transpose()?;
    if null.is_some() && (attr(element, "V").is_some() || attr(element, "U").is_some()) {
        return Err(DataError::Null);
    }
    let literal = attr(element, "V");
    if null.is_none() && literal.is_none_or(str::is_empty) {
        return Err(DataError::MissingValue);
    }
    macro_rules! nullable {
        ($expr:expr) => {
            if let Some(null) = null {
                Nullable::Null(null)
            } else {
                Nullable::Value($expr)
            }
        };
    }
    let text = literal.unwrap_or("");
    Ok(match kind {
        Kind::Text | Kind::Person => Value::Text(nullable!(text.to_owned())),
        Kind::Code | Kind::SimpleCode => Value::Code(nullable!(Code {
            value: text.to_owned(),
            display: attr(element, "DN").map(str::to_owned),
            system: attr(element, "S").map(str::to_owned),
            system_name: attr(element, "SN").map(str::to_owned),
            version: attr(element, "SV").map(str::to_owned)
        })),
        Kind::Quantity => Value::Quantity(nullable!({
            number(text, false)?;
            Quantity {
                value: text.to_owned(),
                unit: attr(element, "U")
                    .filter(|u| !u.is_empty())
                    .ok_or(DataError::MissingUnit)?
                    .to_owned(),
            }
        })),
        Kind::Time => Value::Time(nullable!(Timestamp::parse(text)?)),
        Kind::Integer => Value::Integer(nullable!({
            number(text, true)?;
            text.to_owned()
        })),
        Kind::Real => Value::Real(nullable!({
            number(text, false)?;
            text.to_owned()
        })),
        Kind::Interval => Value::Interval(nullable!({
            interval(text)?;
            (text.to_owned(), attr(element, "U").map(str::to_owned))
        })),
        Kind::Encapsulated => return Err(DataError::Attribute),
    })
}

pub(crate) fn number(text: &str, integer: bool) -> Result<(), DataError> {
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut parts = text.split('.');
    let whole = parts.next().ok_or(DataError::Number)?;
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DataError::Number);
    }
    if let Some(fraction) = parts.next()
        && (integer || fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(DataError::Number);
    }
    if parts.next().is_some() {
        return Err(DataError::Number);
    }
    Ok(())
}

fn interval(text: &str) -> Result<(), DataError> {
    let mut chars = text.chars();
    if !chars.next().is_some_and(|c| matches!(c, '[' | ']'))
        || !chars.next_back().is_some_and(|c| matches!(c, '[' | ']'))
    {
        return Err(DataError::Interval);
    }
    let (low, high) = chars.as_str().split_once(';').ok_or(DataError::Interval)?;
    if low != "-inf" {
        number(low, false).map_err(|_| DataError::Interval)?;
    }
    if high != "+inf" {
        number(high, false).map_err(|_| DataError::Interval)?;
    }
    Ok(())
}

fn validate_attributes(element: &Element, kind: Kind) -> Result<(), DataError> {
    for (key, _) in element.attributes() {
        let allowed = matches!(key.as_str(), "V" | "NULL" | "VT" | "PROB" | "T")
            || (matches!(kind, Kind::Code | Kind::SimpleCode) && key == "DN")
            || (kind == Kind::Code && matches!(key.as_str(), "S" | "SN" | "SV" | "ORIGTXT"))
            || (matches!(kind, Kind::Quantity | Kind::Interval) && key == "U")
            || (kind == Kind::Encapsulated
                && matches!(key.as_str(), "ENC" | "COMPN" | "IC" | "MT"));
        if !allowed {
            return Err(DataError::Attribute);
        }
    }
    if let Some(prob) = attr(element, "PROB") {
        number(prob, false)?;
        let probability = prob.parse::<f64>().map_err(|_| DataError::Number)?;
        if !(0.0..=1.0).contains(&probability) {
            return Err(DataError::Number);
        }
    }
    if let Some(t) = attr(element, "T") {
        let legal = match kind {
            Kind::Text => t == "ST" || t == "ON",
            Kind::Person => t == "PN",
            Kind::SimpleCode => t == "CS",
            Kind::Code => matches!(t, "CV" | "CE" | "CS"),
            Kind::Quantity => t == "PQ",
            Kind::Time => t == "TS",
            Kind::Integer => t == "INT",
            Kind::Real => t == "REAL",
            Kind::Interval => matches!(t, "IVL_PQ" | "PQ"),
            Kind::Encapsulated => matches!(t, "ED" | "ST"),
        };
        if !legal {
            return Err(DataError::Attribute);
        }
    }
    if kind == Kind::Code && attr(element, "S").is_some() && attr(element, "SN").is_none() {
        return Err(DataError::Code);
    }
    if let Some(encoding) = attr(element, "ENC")
        && !matches!(encoding, "TXT" | "B64")
    {
        return Err(DataError::Attribute);
    }
    if let Some(compression) = attr(element, "COMPN")
        && !matches!(compression, "DF" | "GZ" | "ZL" | "Z")
    {
        return Err(DataError::Attribute);
    }
    Ok(())
}

fn parse_offset(text: &str) -> Result<i32, DataError> {
    if text.len() != 6 || text.get(3..4) != Some(":") {
        return Err(DataError::Time);
    }
    let hours = date_component(text.get(1..3), 0)?;
    let minutes = date_component(text.get(4..6), 0)?;
    if hours > 23 || minutes > 59 {
        return Err(DataError::Time);
    }
    let seconds = i32::from(hours)
        .checked_mul(3600)
        .and_then(|h| {
            i32::from(minutes)
                .checked_mul(60)
                .and_then(|m| h.checked_add(m))
        })
        .ok_or(DataError::Time)?;
    if text.starts_with('-') {
        seconds.checked_neg().ok_or(DataError::Time)
    } else if text.starts_with('+') {
        Ok(seconds)
    } else {
        Err(DataError::Time)
    }
}
