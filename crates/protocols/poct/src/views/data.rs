//! Holds POCT data values and preserves their source spelling.
//! Kept whole: POCT data structures and their field accessors define one wire vocabulary.
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
    local: String,
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
        } else if let Some(local) = value.strip_suffix('Z') {
            if !local.contains('T') {
                return Err(DataError::Time);
            }
            (local, Some(0))
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
            local: local.to_owned(),
        })
    }

    /// Exact source spelling, including fractional precision.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Borrows the checked local calendar text, excluding the supplied offset.
    #[must_use]
    pub fn local(&self) -> &str {
        &self.local
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
    Encapsulated(EncapsulatedData),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    Code,
    SimpleCode,
    Concept,
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
        return Ok(Value::Encapsulated(EncapsulatedData {
            text: element.first_text().to_owned(),
            encoding: if attr(element, "ENC") == Some("B64") {
                Encoding::Base64
            } else {
                Encoding::Text
            },
            compression: attr(element, "COMPN").map(str::to_owned),
            integrity_check: attr(element, "IC").map(str::to_owned),
            media_type: attr(element, "MT").unwrap_or("text/plain").to_owned(),
            null: attr(element, "NULL").map(Null::parse).transpose()?,
        }));
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
        Kind::Code | Kind::SimpleCode | Kind::Concept => Value::Code(nullable!(Code {
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
        && (integer || !fraction.bytes().all(|b| b.is_ascii_digit()))
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
    for (key, supplied) in element.attributes() {
        if let Some(expected) = fixed_attribute(element, kind, key) {
            if supplied != expected {
                return Err(DataError::Attribute);
            }
            continue;
        }
        let allowed = matches!(key.as_str(), "NULL" | "VT" | "PROB" | "T")
            || (kind != Kind::Encapsulated && key == "V")
            || (matches!(kind, Kind::Code | Kind::SimpleCode | Kind::Concept) && key == "DN")
            || (matches!(kind, Kind::Code | Kind::Concept)
                && matches!(key.as_str(), "S" | "SN" | "SV" | "ORIGTXT"))
            || (matches!(kind, Kind::Quantity | Kind::Interval) && key == "U")
            || (kind == Kind::Encapsulated
                && matches!(key.as_str(), "ENC" | "COMPN" | "IC" | "MT"))
            || (matches!(kind, Kind::Text | Kind::Encapsulated) && key == "xml:lang")
            || (matches!(element.name(), "GIV" | "MID" | "FAM" | "PFX" | "SFX") && key == "QUAL")
            || (element.name() == "REF" && key == "USE");
        if !allowed {
            return Err(DataError::Attribute);
        }
    }
    if let Some(prob) = attr(element, "PROB") {
        number(prob, false)?;
        let negative = prob.starts_with('-');
        let unsigned = prob.strip_prefix(['+', '-']).unwrap_or(prob);
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let whole = whole.trim_start_matches('0');
        let fractional_zero = fraction.bytes().all(|b| b == b'0');
        let valid = if negative {
            whole.is_empty() && fractional_zero
        } else {
            whole.is_empty() || (whole == "1" && fractional_zero)
        };
        if !valid {
            return Err(DataError::Number);
        }
    }
    if let Some(t) = attr(element, "T") {
        let legal = match kind {
            Kind::Text => {
                t == match element.name() {
                    "DEV.manufacturer_name" => "ON",
                    "GIV" | "MID" | "FAM" | "PFX" | "SFX" => "PNXP",
                    "DEL" => "ADXP",
                    "REF" => "TEL",
                    _ => "ST",
                }
            }
            Kind::Person => t == "PN",
            Kind::SimpleCode => t == "CS",
            Kind::Concept => matches!(t, "CD" | "CE" | "CV" | "CS"),
            Kind::Code => {
                matches!(t, "CS" | "CV")
                    || (t == "CE"
                        && matches!(
                            element.name(),
                            "OBS.observation_id"
                                | "ORD.universal_service_id"
                                | "SPC.source_cd"
                                | "SPC.type_cd"
                                | "NOTE"
                        ))
            }
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
    if matches!(kind, Kind::Code | Kind::Concept)
        && ["S", "SN", "SV"]
            .iter()
            .any(|key| attr(element, key).is_some_and(str::is_empty))
    {
        return Err(DataError::Code);
    }
    if matches!(kind, Kind::Code | Kind::Concept)
        && (attr(element, "S").is_some() || attr(element, "SV").is_some())
        && attr(element, "SN").is_none()
    {
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

/// Encapsulated data encoding from Appendix B section 8.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Plain text, the POCT default.
    Text,
    /// Base64 text retained encoded; no expansion is performed.
    Base64,
}

/// Encapsulated source data. External references remain inert document nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncapsulatedData {
    /// Exact decoded XML character content, without decompression or base64 decoding.
    pub text: String,
    /// Source encoding or the standard's TXT default.
    pub encoding: Encoding,
    /// Compression code; compression is never executed here.
    pub compression: Option<String>,
    /// Integrity check as supplied; not a claim that it has been verified.
    pub integrity_check: Option<String>,
    /// Source media type or the standard's text/plain default.
    pub media_type: String,
    /// Explicit null, when supplied.
    pub null: Option<Null>,
}

fn value_type(element: &Element, kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::Text => {
            if element.name() == "REF" {
                "URL"
            } else if element.name().contains('.') && element.name() != "DEV.manufacturer_name" {
                "BIN"
            } else {
                "ST"
            }
        }
        Kind::Person | Kind::Time | Kind::Integer | Kind::Real => "ST",
        Kind::Quantity | Kind::Interval => "REAL",
        Kind::Code | Kind::SimpleCode | Kind::Concept | Kind::Encapsulated => return None,
    })
}

fn fixed_attribute<'a>(element: &Element, kind: Kind, key: &'a str) -> Option<&'a str> {
    let code = matches!(kind, Kind::Code | Kind::SimpleCode | Kind::Concept);
    match key {
        "V-T" | "DN-T" if code => Some("ST"),
        "V-T" => value_type(element, kind),
        "V-HL7_NAME" if kind != Kind::Encapsulated => Some(if code {
            "code"
        } else if kind == Kind::Person {
            "formatted"
        } else {
            "value"
        }),
        "VT-T" => Some(if element.name() == "REF" {
            "GTS"
        } else {
            "IVL_TS"
        }),
        "VT-HL7_NAME" => Some("validTime"),
        "PROB-T" => Some("REAL"),
        "PROB-HL7_NAME" => Some("probability"),
        "DN-HL7_NAME" if code => Some("displayName"),
        "S-T" | "SV-T" if matches!(kind, Kind::Code | Kind::Concept) => Some("OID"),
        "S-HL7_NAME" if matches!(kind, Kind::Code | Kind::Concept) => Some("codeSystem"),
        "SN-T" | "ORIGTXT-T" if matches!(kind, Kind::Code | Kind::Concept) => Some("ST"),
        "SN-HL7_NAME" if matches!(kind, Kind::Code | Kind::Concept) => Some("codeSystemName"),
        "SV-HL7_NAME" if matches!(kind, Kind::Code | Kind::Concept) => Some("codeSystemVersion"),
        "ORIGTXT-HL7_NAME" if matches!(kind, Kind::Code | Kind::Concept) => Some("originalText"),
        "U-T" if matches!(kind, Kind::Quantity | Kind::Interval) => Some("CS"),
        "U-HL7_NAME" if matches!(kind, Kind::Quantity | Kind::Interval) => Some("unit"),
        _ => content_attribute(element, kind, key),
    }
}

fn content_attribute<'a>(element: &Element, kind: Kind, key: &'a str) -> Option<&'a str> {
    match key {
        "MT-T" if matches!(kind, Kind::Text | Kind::Encapsulated) => Some("CS"),
        "MT-HL7_NAME" if matches!(kind, Kind::Text | Kind::Encapsulated) => Some("type"),
        "MT-DOMAIN" if kind == Kind::Encapsulated => Some("2.16.840.1.113883.6.10"),
        "MT" if kind == Kind::Text => Some("text/plain"),
        "xml:lang-T" if matches!(kind, Kind::Text | Kind::Encapsulated) => Some("CS"),
        "xml:lang-HL7_NAME" if matches!(kind, Kind::Text | Kind::Encapsulated) => Some("language"),
        "COMPN-T" if kind == Kind::Encapsulated => Some("CS"),
        "COMPN-HL7_NAME" if kind == Kind::Encapsulated => Some("compression"),
        "COMPN-DOMAIN" if kind == Kind::Encapsulated => Some("2.16.840.1.113883.5.1009"),
        "IC-T" if kind == Kind::Encapsulated => Some("BIN"),
        "IC-HL7_NAME" if kind == Kind::Encapsulated => Some("integrityCheck"),
        _ => name_attribute(element, key),
    }
}

fn name_attribute<'a>(element: &Element, key: &'a str) -> Option<&'a str> {
    match key {
        "QUAL-T" if matches!(element.name(), "GIV" | "MID" | "FAM" | "PFX" | "SFX") => {
            Some("SET_CS")
        }
        "QUAL-HL7_NAME" if matches!(element.name(), "GIV" | "MID" | "FAM" | "PFX" | "SFX") => {
            Some("qualifier")
        }
        "QUAL-DOMAIN" if matches!(element.name(), "GIV" | "MID" | "FAM" | "PFX" | "SFX") => {
            Some("2.16.840.1.113883.5.1014")
        }
        "USE-T" if element.name() == "REF" => Some("SET_CS"),
        "USE-HL7_NAME" if element.name() == "REF" => Some("use"),
        "USE-DOMAIN" if element.name() == "REF" => Some("2.16.840.1.113883.5.1011"),
        "HL7_NAME" => match element.name() {
            "NOTE" => Some("note"),
            "CONFID" => Some("confidence"),
            "REF" => Some("reference"),
            "THUMBNAIL" => Some("thumbnail"),
            "ORIGTXT" => Some("originalText"),
            "TRANSLTN" => Some("translation"),
            "GIV" => Some("given"),
            "MID" => Some("middle"),
            "FAM" => Some("family"),
            "PFX" => Some("prefix"),
            "SFX" => Some("suffix"),
            "DEL" => Some("delimiter"),
            _ => None,
        },
        _ => None,
    }
}
