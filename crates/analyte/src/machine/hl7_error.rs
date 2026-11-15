//! Shared HL7/LAW error evidence: never invent a location for an unknown cause.
use analyte_hl7::{AckError, Terminator};
use analyte_mapping::{Output, Position};

#[derive(Debug, Clone)]
pub(crate) struct Location {
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
                found = Some(Position {
                    record: index.checked_add(1)?,
                    field,
                });
            }
        }
        Self::at(raw, ending, found?)
    }
    fn at(raw: &[u8], ending: Terminator, position: Position) -> Option<Self> {
        if position.record == 0 || position.field == 0 {
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
            .nth(position.record.checked_sub(1)?)?
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
            .take(position.record)
            .filter(|s| s.get(..3) == Some(segment.as_slice()))
            .count();
        Some(Self {
            segment,
            occurrence: u16::try_from(occurrence).ok()?,
            field: u16::try_from(position.field).ok()?,
        })
    }
}

pub(crate) fn mapping_location(
    mapping: &Result<Output, crate::Error>,
    raw: &[u8],
    ending: Terminator,
) -> Option<Location> {
    let position = match mapping {
        Ok(output) => output.findings.first()?.position,
        Err(error) => {
            if let Some(error) = error.downcast_ref::<analyte_hl7::Error>() {
                return syntax_location(error, raw, ending);
            }
            match error.downcast_ref::<analyte_mapping::Error>()? {
                analyte_mapping::Error::Limit { position, .. }
                | analyte_mapping::Error::Structure { position, .. }
                | analyte_mapping::Error::Encoding { position }
                | analyte_mapping::Error::Text { position, .. } => *position,
                // Control wrappers do not always identify the same field as their cause.
                analyte_mapping::Error::Hl7 { source, .. } => {
                    return syntax_location(source, raw, ending);
                }
                analyte_mapping::Error::Syntax => return None,
            }
        }
    };
    Location::at(raw, ending, position)
}

fn syntax_location(error: &analyte_hl7::Error, raw: &[u8], ending: Terminator) -> Option<Location> {
    use analyte_hl7::Error;
    match error {
        Error::Control { segment, field } => {
            Location::named(raw, ending, segment.as_bytes(), *field)
        }
        Error::Limit { segment, field, .. } | Error::Escape { segment, field, .. } => Location::at(
            raw,
            ending,
            Position {
                record: *segment,
                field: *field,
            },
        ),
        Error::Encoding
        | Error::Truncation
        | Error::Terminator
        | Error::Segment(_)
        | Error::UnsupportedCharset
        | Error::CharacterEncoding
        | Error::CharsetMismatch
        | Error::NotStored
        | Error::AckMode => None,
    }
}

pub(crate) fn build<'a>(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl7_251_2_15_5_2_location_requires_source_occurrence_and_keeps_numeric_segment_ids() {
        let raw = b"MSH|^~\\&|\rPV1|1\rOBX|1\rOBX|2\r";
        let precise = syntax_location(
            &analyte_hl7::Error::Escape {
                segment: 4,
                field: 5,
                offset: 0,
            },
            raw,
            Terminator::Cr,
        );
        let error = build(precise.as_ref(), b"102", b"synthetic");
        assert_eq!(
            (error.segment, error.occurrence, error.field),
            (b"OBX".as_slice(), 2, 5)
        );
        assert!(
            syntax_location(
                &analyte_hl7::Error::Control {
                    segment: "OBX",
                    field: 5
                },
                raw,
                Terminator::Cr
            )
            .is_none()
        );
        let precise = syntax_location(
            &analyte_hl7::Error::Control {
                segment: "PV1",
                field: 2,
            },
            raw,
            Terminator::Cr,
        );
        let error = build(precise.as_ref(), b"102", b"synthetic");
        assert_eq!(
            (error.segment, error.occurrence, error.field),
            (b"PV1".as_slice(), 1, 2)
        );
        assert!(
            Location::at(
                raw,
                Terminator::Cr,
                Position {
                    record: 99,
                    field: 5
                }
            )
            .is_none()
        );
        assert!(
            Location::at(
                raw,
                Terminator::Cr,
                Position {
                    record: 4,
                    field: 0
                }
            )
            .is_none()
        );
    }
}
