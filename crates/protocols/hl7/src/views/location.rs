//! Reads typed HL7 location fields.
use crate::{AckError, Terminator};
#[derive(Debug, Clone)]
/// A source-backed segment occurrence and field for acknowledgment errors.
pub struct Location {
    segment: [u8; 3],
    occurrence: u16,
    field: u16,
}
impl Location {
    fn named(raw: &[u8], ending: Terminator, name: &[u8], field: usize) -> Option<Self> {
        let separator = match ending {
            Terminator::Cr | Terminator::CrLf => b'\r',
            Terminator::Lf => b'\n',
        };
        let mut found = None;
        for (index, segment) in raw.split(|b| *b == separator).enumerate() {
            if segment.strip_prefix(b"\n").unwrap_or(segment).get(..3) == Some(name) {
                if found.is_some() {
                    return None;
                }
                found = Some(index.checked_add(1)?);
            }
        }
        Self::at(raw, ending, found?, field)
    }
    /// Resolves a one-based record and field without guessing absent locations.
    #[must_use]
    pub fn at(raw: &[u8], ending: Terminator, record: usize, field: usize) -> Option<Self> {
        if record == 0 || field == 0 {
            return None;
        }
        let separator = match ending {
            Terminator::Cr | Terminator::CrLf => b'\r',
            Terminator::Lf => b'\n',
        };
        let segments = || {
            raw.split(|b| *b == separator)
                .map(|s| s.strip_prefix(b"\n").unwrap_or(s))
        };
        let segment: [u8; 3] = segments()
            .nth(record.checked_sub(1)?)?
            .get(..3)?
            .try_into()
            .ok()?;
        if !segment
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        {
            return None;
        }
        let occurrence = segments()
            .take(record)
            .filter(|s| s.get(..3) == Some(segment.as_slice()))
            .count();
        Some(Self {
            segment,
            occurrence: u16::try_from(occurrence).ok()?,
            field: u16::try_from(field).ok()?,
        })
    }
}

/// Resolves an HL7 syntax error only when its raw source location is known.
#[must_use]
pub fn syntax_location(error: &crate::Error, raw: &[u8], ending: Terminator) -> Option<Location> {
    use crate::Error;
    match error {
        Error::Control { segment, field } => {
            Location::named(raw, ending, segment.as_bytes(), *field)
        }
        Error::Limit { segment, field, .. }
        | Error::Escape { segment, field, .. }
        | Error::Truncated { segment, field } => Location::at(raw, ending, *segment, *field),
        Error::WithdrawnQuery { .. }
        | Error::EditionRules(_)
        | Error::Encoding
        | Error::Truncation
        | Error::Terminator
        | Error::Segment(_)
        | Error::UnsupportedCharset
        | Error::CharacterEncoding
        | Error::CharsetMismatch
        | Error::UndeclaredCharset
        | Error::NotStored
        | Error::AckMode => None,
    }
}

/// Creates an acknowledgment error, retaining an unknown location as unknown.
#[must_use]
pub fn location_error<'a>(
    location: Option<&'a Location>,
    code: &'a [u8],
    application_code: &'a [u8],
) -> AckError<'a> {
    match location {
        Some(location) => AckError {
            segment: &location.segment,
            occurrence: location.occurrence,
            field: location.field,
            code,
            application_code,
        },
        None => AckError::unlocated(code, application_code),
    }
}
