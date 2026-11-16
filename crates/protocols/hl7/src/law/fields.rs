//! Checks LAW field lengths, components, required values and identities.
use super::{ControlDirection, Error, content, raw};
use crate::Segment;

pub(super) fn part(value: &[u8], position: usize) -> &[u8] {
    position
        .checked_sub(1)
        .and_then(|index| value.split(|b| *b == b'^').nth(index))
        .unwrap_or_default()
}

pub(super) fn component_error(segment: &'static str, field: usize, component: usize) -> Error {
    Error::Component {
        segment,
        field,
        component,
    }
}

// HL7 lengths count characters, not UTF-8 octets. Delimiter escapes represent
// one character. Other escapes remain uninterpreted, as in the syntax layer.
pub(super) fn characters(value: &[u8]) -> usize {
    let mut pieces = value.split(|b| *b == b'\\');
    let count = |piece: &[u8]| piece.iter().filter(|byte| (**byte & 0xc0) != 0x80).count();
    let mut length = count(pieces.next().unwrap_or_default());
    while let Some(code) = pieces.next() {
        let tail = pieces.next();
        let encoded = if tail.is_some() && matches!(code, b"F" | b"S" | b"R" | b"T" | b"E") {
            1
        } else {
            count(code).saturating_add(if tail.is_some() { 2 } else { 1 })
        };
        length = length
            .saturating_add(encoded)
            .saturating_add(count(tail.unwrap_or_default()));
    }
    length
}

pub(super) fn primitive(
    value: &[u8],
    required: bool,
    maximum: usize,
    error: Error,
) -> Result<(), Error> {
    let delimiters = crate::Delimiters {
        truncation: None,
        field: b'|',
        component: b'^',
        repetition: b'~',
        escape: b'\\',
        subcomponent: b'&',
    };
    if crate::syntax::validate_escapes(value, delimiters, 0, 0).is_err() {
        return Err(error);
    }
    if (required && value.is_empty())
        || value == b"\"\""
        || characters(value) > maximum
        || value.iter().any(|b| matches!(b, b'^' | b'&'))
        || std::str::from_utf8(value).is_err()
        || std::str::from_utf8(value).is_ok_and(|text| text.chars().any(char::is_control))
    {
        return Err(error);
    }
    Ok(())
}

pub(super) fn field(
    segment: Segment<'_>,
    name: &'static str,
    number: usize,
    required: bool,
    repeat: bool,
) -> Result<(), Error> {
    let value = raw(segment, number);
    if (required && value.is_empty()) || value == b"\"\"" || (!repeat && value.contains(&b'~')) {
        return Err(content(name, number));
    }
    Ok(())
}

pub(super) fn scalar(
    segment: Segment<'_>,
    name: &'static str,
    number: usize,
    required: bool,
    maximum: usize,
) -> Result<(), Error> {
    field(segment, name, number, required, false)?;
    primitive(
        raw(segment, number),
        required,
        maximum,
        content(name, number),
    )
}

pub(super) fn excluded_fields(
    segment: Segment<'_>,
    name: &'static str,
    allowed: &[usize],
    direction: ControlDirection,
) -> Result<(), Error> {
    if direction == ControlDirection::Send {
        for (index, field) in segment.fields().enumerate() {
            let number = index.checked_add(1).ok_or(content(name, 0))?;
            if !allowed.contains(&number) && !field.raw().is_empty() {
                return Err(content(name, number));
            }
        }
    }
    Ok(())
}

/// Checks the LAW ST identity boundary without changing spelling or encoding.
pub(super) fn identity(value: &[u8]) -> Result<(), Error> {
    primitive(value, true, 50, content("MSH", 10))?;
    if value.iter().any(|byte| matches!(byte, b'|' | b'~')) {
        return Err(content("MSH", 10));
    }
    Ok(())
}
