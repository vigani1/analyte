//! Manager control profiles from `PaLM` TF-2b Rev 11, C.1, C.3, C.4 and 2.2.
use super::{Error, content, raw};
use crate::{Message, Options, Segment};

/// Whether control content is being received or prepared for transmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlDirection {
    /// Ignore unsupported elements while preserving the original message (TF-2b 2.2.1).
    Receive,
    /// Require unsupported elements to be empty before transmission.
    Send,
}

/// Parses bounded LAW wire syntax, allowing C.4's compatible `2.5` version identifiers.
/// Original bytes, including ignored elements, remain unchanged. Call a LAW transaction
/// view or [`validate_control`] next; parsing alone does not establish conformance.
/// # Errors
/// Returns a syntax error for malformed encoding, escapes, or configured size limits.
pub fn parse_message(bytes: &[u8], options: Options) -> Result<Message, crate::Error> {
    Message::parse_law(bytes, options)
}

/// Validates MSH, MSA and ERR fields/components and their message predicates.
/// RE availability is supplied by the caller: omission means no value is available;
/// supplied values are always checked. No identities or clinical values are invented.
/// Other segment groups retain their separate transaction validators.
/// # Errors
/// Reports the exact segment/field/component for invalid content, or field zero for
/// segment cardinality. Correlation to a sent message remains the transaction's job.
pub fn validate_control(message: &Message, direction: ControlDirection) -> Result<(), Error> {
    let msh = message.segments().next().ok_or(content("MSH", 0))?;
    validate_segment(msh, direction)?;
    let kind = part(raw(msh, 9), 1);
    let response = matches!(kind, b"ACK" | b"RSP" | b"ORL");
    let mut msa = None;
    let mut error_count = 0usize;
    for (ordinal, segment) in message.segments().enumerate().skip(1) {
        match segment.id() {
            b"MSH" => return Err(content("MSH", 0)),
            b"MSA" => {
                if !response || ordinal != 1 || msa.is_some() || error_count != 0 {
                    return Err(content("MSA", 0));
                }
                validate_segment(segment, direction)?;
                msa = Some(raw(segment, 1));
            }
            b"ERR" => {
                if !matches!(msa, Some(b"AE" | b"AR")) {
                    return Err(content("ERR", 0));
                }
                validate_segment(segment, direction)?;
                error_count = error_count.checked_add(1).ok_or(content("ERR", 0))?;
            }
            _ => {}
        }
    }
    if response && msa.is_none() {
        return Err(content("MSA", 0));
    }
    if matches!(msa, Some(b"AE" | b"AR")) && error_count == 0 {
        return Err(content("ERR", 0));
    }
    Ok(())
}

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

fn components(
    value: &[u8],
    name: &'static str,
    field: usize,
    rules: &[(bool, usize)],
    direction: ControlDirection,
) -> Result<(), Error> {
    // Empty trailing positions still consume encoded field length on output.
    // Incoming X components are ignored rather than counted against the profile.
    let maximum = match (name, field) {
        ("MSH", 3..=6) => 227,
        ("MSH", 7) => 26,
        ("MSH", 9) => 15,
        ("MSH", 11) => 3,
        ("MSH", 12) => 60,
        ("MSH", 21) => 427,
        ("ERR", 2) => 18,
        ("ERR", 3 | 5) => 705,
        _ => return Err(content(name, field)),
    };
    if direction == ControlDirection::Send && characters(value) > maximum {
        return Err(content(name, field));
    }
    for (index, (required, maximum)) in rules.iter().enumerate() {
        let position = index.checked_add(1).ok_or(content(name, field))?;
        primitive(
            part(value, position),
            *required,
            *maximum,
            component_error(name, field, position),
        )?;
    }
    if direction == ControlDirection::Send {
        for (index, extra) in value.split(|b| *b == b'^').enumerate().skip(rules.len()) {
            if !extra.is_empty() {
                return Err(component_error(
                    name,
                    field,
                    index.checked_add(1).ok_or(content(name, field))?,
                ));
            }
        }
    }
    Ok(())
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

/// Checks one control segment using the selected directional usage policy.
pub(super) fn validate_segment(
    segment: Segment<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    match segment.id() {
        b"MSH" => header(segment, direction),
        b"MSA" => {
            excluded_fields(segment, "MSA", &[1, 2], direction)?;
            scalar(segment, "MSA", 1, true, 2)?;
            scalar(segment, "MSA", 2, true, 50)?;
            if !matches!(raw(segment, 1), b"AA" | b"AE" | b"AR") {
                return Err(content("MSA", 1));
            }
            Ok(())
        }
        b"ERR" => error(segment, direction),
        _ => Err(content("LAW control", 0)),
    }
}

/// Dispatches received control segments separately from other segment groups.
pub(super) fn received_segment(segment: Segment<'_>) -> Result<(), Error> {
    match segment.id() {
        b"MSH" | b"MSA" | b"ERR" => validate_segment(segment, ControlDirection::Receive),
        _ => super::Profile::receive().validate_segment(segment, ControlDirection::Receive),
    }
}

fn header(segment: Segment<'_>, direction: ControlDirection) -> Result<(), Error> {
    excluded_fields(
        segment,
        "MSH",
        &[1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 15, 16, 18, 21],
        direction,
    )?;
    if raw(segment, 1) != b"|" {
        return Err(content("MSH", 1));
    }
    if raw(segment, 2) != b"^~\\&" {
        return Err(content("MSH", 2));
    }
    for number in [3, 4, 5, 6] {
        field(segment, "MSH", number, false, false)?;
        let value = raw(segment, number);
        if !value.is_empty() {
            components(value, "MSH", number, &[(number != 6, 20)], direction)?;
        }
    }
    field(segment, "MSH", 7, true, false)?;
    components(raw(segment, 7), "MSH", 7, &[(true, 19)], direction)?;
    if !super::profile::timestamp(part(raw(segment, 7), 1), true) {
        return Err(component_error("MSH", 7, 1));
    }
    field(segment, "MSH", 9, true, false)?;
    let message_type = raw(segment, 9);
    components(
        message_type,
        "MSH",
        9,
        &[(true, 3), (true, 3), (true, 7)],
        direction,
    )?;
    let kind = part(message_type, 1);
    let event = part(message_type, 2);
    let structure = part(message_type, 3);
    let (transaction, triggered) = match (kind, event, structure) {
        (b"QBP", b"Q11", b"QBP_Q11") | (b"RSP", b"K11", b"RSP_K11") => {
            (b"LAB-27".as_slice(), kind == b"QBP")
        }
        (b"OML", b"O33", b"OML_O33") | (b"ORL", b"O34", b"ORL_O42") => {
            (b"LAB-28".as_slice(), kind == b"OML")
        }
        (b"OUL", b"R22", b"OUL_R22") | (b"ACK", b"R22", b"ACK") => {
            (b"LAB-29".as_slice(), kind == b"OUL")
        }
        _ => return Err(content("MSH", 9)),
    };
    scalar(segment, "MSH", 10, true, 50)?;
    field(segment, "MSH", 11, true, false)?;
    components(raw(segment, 11), "MSH", 11, &[(true, 1)], direction)?;
    if part(raw(segment, 11), 1) != b"P" {
        return Err(component_error("MSH", 11, 1));
    }
    field(segment, "MSH", 12, true, false)?;
    components(raw(segment, 12), "MSH", 12, &[(true, 60)], direction)?;
    if !part(raw(segment, 12), 1).starts_with(b"2.5") {
        return Err(component_error("MSH", 12, 1));
    }
    for (number, expected) in [(15, b"NE".as_slice()), (16, b"AL".as_slice())] {
        if triggered {
            scalar(segment, "MSH", number, true, 2)?;
            if raw(segment, number) != expected {
                return Err(content("MSH", number));
            }
        } else if direction == ControlDirection::Send && !raw(segment, number).is_empty() {
            return Err(content("MSH", number));
        }
    }
    scalar(segment, "MSH", 18, true, 16)?;
    if raw(segment, 18) != b"UNICODE UTF-8" {
        return Err(content("MSH", 18));
    }
    field(segment, "MSH", 21, true, true)?;
    for (index, value) in raw(segment, 21).split(|b| *b == b'~').enumerate() {
        components(value, "MSH", 21, &[(true, 50), (true, 20)], direction)?;
        if index == 0 && (part(value, 1) != transaction || part(value, 2) != b"IHE") {
            return Err(content("MSH", 21));
        }
    }
    Ok(())
}

fn error(segment: Segment<'_>, direction: ControlDirection) -> Result<(), Error> {
    excluded_fields(segment, "ERR", &[2, 3, 4, 5, 8], direction)?;
    field(segment, "ERR", 2, false, true)?;
    if !raw(segment, 2).is_empty() {
        for value in raw(segment, 2).split(|b| *b == b'~') {
            components(
                value,
                "ERR",
                2,
                &[
                    (true, 3),
                    (true, 2),
                    (false, 2),
                    (false, 2),
                    (false, 2),
                    (false, 2),
                ],
                direction,
            )?;
            let id = part(value, 1);
            if id.len() != 3
                || !id.first().is_some_and(u8::is_ascii_uppercase)
                || !id
                    .iter()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            {
                return Err(component_error("ERR", 2, 1));
            }
            for position in 2..=6 {
                let number = part(value, position);
                if !number.is_empty()
                    && (!number.iter().all(u8::is_ascii_digit) || number.iter().all(|b| *b == b'0'))
                {
                    return Err(component_error("ERR", 2, position));
                }
            }
        }
    }
    for number in [3, 5] {
        field(segment, "ERR", number, number == 3, false)?;
        let value = raw(segment, number);
        if value.is_empty() {
            continue;
        }
        components(
            value,
            "ERR",
            number,
            &[
                (true, 20),
                (false, 199),
                (true, if number == 3 { 7 } else { 12 }),
            ],
            direction,
        )?;
        let code = part(value, 1);
        let system = part(value, 3);
        if number == 3 {
            if !matches!(
                code,
                b"100" | b"101" | b"102" | b"103" | b"200" | b"201" | b"202" | b"203" | b"207"
            ) {
                return Err(component_error("ERR", number, 1));
            }
            if system != b"HL70357" {
                return Err(component_error("ERR", number, 3));
            }
        } else if system.len() != 5
            || !system.starts_with(b"99")
            || !system
                .get(2..)
                .is_some_and(|suffix| suffix.iter().all(u8::is_ascii_alphanumeric))
        {
            return Err(component_error("ERR", number, 3));
        }
    }
    scalar(segment, "ERR", 4, true, 2)?;
    if raw(segment, 4) != b"E" {
        return Err(content("ERR", 4));
    }
    // C.1's 250# is truncatable, not a hard maximum. Preserve the entire TX;
    // the parser's message bound remains the resource ceiling (TF-2b 2.2.3).
    scalar(segment, "ERR", 8, false, usize::MAX)
}

// Reply headers use only supported incoming components. Ignored X content must
// remain in the receipt, rather than leak back out as manager-authored data.
/// Selects supported source components for a newly authored reply header.
pub(super) fn reply_field(segment: Segment<'_>, number: usize) -> &[u8] {
    match number {
        3 | 4 | 5 | 6 | 7 | 11 | 12 => part(raw(segment, number), 1),
        _ => raw(segment, number),
    }
}

/// Checks the LAW ST identity boundary without changing spelling or encoding.
pub(super) fn identity(value: &[u8]) -> Result<(), Error> {
    primitive(value, true, 50, content("MSH", 10))?;
    if value.iter().any(|byte| matches!(byte, b'|' | b'~')) {
        return Err(content("MSH", 10));
    }
    Ok(())
}
