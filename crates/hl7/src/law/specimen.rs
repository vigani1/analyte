//! Specimen/container profiles from TF-2b C.11/C.12, B.1 and 2.2.4.
use super::control::{
    characters, component_error, excluded_fields, field, part, primitive, scalar,
};
use super::patient::{coded, excluded, extras, universal_type};
use super::{ControlDirection, Error, Profile, content, raw};
use crate::Segment;

fn sub(field: usize, position: usize) -> Error {
    Error::Subcomponent {
        segment: "SPM",
        field,
        component: 1,
        subcomponent: position,
    }
}

/// Parses a positive SI without changing its original spelling.
pub(super) fn set_id(value: &[u8]) -> Option<usize> {
    if value.is_empty() || value.len() > 4 || !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let number = value.iter().try_fold(0usize, |number, byte| {
        number
            .checked_mul(10)?
            .checked_add(usize::from(byte.checked_sub(b'0')?))
    })?;
    (number > 0).then_some(number)
}

fn eip(value: &[u8], number: usize, direction: ControlDirection) -> Result<(), Error> {
    let placer = part(value, 1);
    let mut pieces = placer.split(|b| *b == b'&');
    let id = pieces.next().unwrap_or_default();
    let namespace = pieces.next().unwrap_or_default();
    let universal = pieces.next().unwrap_or_default();
    let kind = pieces.next().unwrap_or_default();
    primitive(id, true, 50, sub(number, 1))?;
    primitive(namespace, false, 20, sub(number, 2))?;
    primitive(universal, false, 199, sub(number, 3))?;
    primitive(kind, false, 6, sub(number, 4))?;
    if !kind.is_empty() {
        universal_type(kind, sub(number, 4))?;
    }
    if namespace.is_empty() && (universal.is_empty() || kind.is_empty()) {
        return Err(sub(number, 2));
    }
    // EIP.1 is the only supported EI. Ignore filler IDs and extra subcomponents
    // on receive, but never carry them into manager-authored data.
    extras(placer, b'&', &[1, 2, 3, 4], direction, |position| {
        sub(number, position)
    })?;
    extras(value, b'^', &[1], direction, |position| {
        component_error("SPM", number, position)
    })?;
    let supported_length =
        placer
            .split(|b| *b == b'&')
            .take(4)
            .enumerate()
            .fold(0usize, |length, (index, value)| {
                length
                    .saturating_add(characters(value))
                    .saturating_add(usize::from(index != 0))
            });
    if supported_length > 80 {
        return Err(content("SPM", number));
    }
    super::order::limit(value, "SPM", number, 80, direction)
}

fn option_field(
    segment: Segment<'_>,
    name: &'static str,
    number: usize,
    enabled: bool,
    direction: ControlDirection,
    repeat: bool,
) -> Result<bool, Error> {
    let value = raw(segment, number);
    if !enabled {
        excluded(value, direction, content(name, number))?;
        return Ok(false);
    }
    field(segment, name, number, false, repeat)?;
    Ok(!value.is_empty())
}

fn specimen(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(
        segment,
        "SPM",
        &[1, 2, 3, 4, 7, 8, 9, 11, 13, 16, 17, 18, 27],
        direction,
    )?;
    scalar(segment, "SPM", 1, true, 4)?;
    if set_id(raw(segment, 1)).is_none() {
        return Err(content("SPM", 1));
    }
    for number in [2, 3] {
        if number == 3
            && profile.pooling_outside
            && profile.results_message
            && direction == ControlDirection::Receive
        {
            continue;
        }
        let enabled = profile.specimen || (number == 3 && profile.pooling_outside);
        if option_field(segment, "SPM", number, enabled, direction, number == 3)? {
            for value in raw(segment, number).split(|b| *b == b'~') {
                eip(value, number, direction)?;
            }
        } else if number == 3 && profile.pooling_outside {
            return Err(content("SPM", 3));
        }
    }
    validate_role_and_type(segment, profile, direction)?;
    for (number, system) in [
        (7, b"HL70488".as_slice()),
        (8, b""),
        (9, b""),
        (16, b"HL70489"),
        (27, b""),
    ] {
        if option_field(
            segment,
            "SPM",
            number,
            profile.specimen && direction == ControlDirection::Send,
            direction,
            false,
        )? {
            coded(raw(segment, number), "SPM", number, system, direction)?;
        }
    }
    validate_specimen_times(segment, profile, direction)?;
    if profile.pooling_outside {
        scalar(segment, "SPM", 13, true, 6)?;
        let value = raw(segment, 13);
        if !value.iter().all(u8::is_ascii_digit) || value.iter().all(|b| *b == b'0') {
            return Err(content("SPM", 13));
        }
    } else {
        excluded(raw(segment, 13), direction, content("SPM", 13))?;
    }
    Ok(())
}

fn ei(value: &[u8], number: usize, direction: ControlDirection) -> Result<(), Error> {
    primitive(part(value, 1), true, 50, component_error("SAC", number, 1))?;
    extras(value, b'^', &[1], direction, |position| {
        component_error("SAC", number, position)
    })?;
    super::order::limit(value, "SAC", number, 80, direction)
}

// NM is a signed decimal spelling, not a floating point computation. This
// avoids rounding or overflow while retaining exactly what the app supplied.
pub(super) fn numeric(value: &[u8]) -> bool {
    let unsigned = if matches!(value.first(), Some(b'+' | b'-')) {
        value.get(1..).unwrap_or_default()
    } else {
        value
    };
    let mut decimal = false;
    let mut digit = false;
    for byte in unsigned {
        match byte {
            b'0'..=b'9' => digit = true,
            b'.' if !decimal => decimal = true,
            _ => return false,
        }
    }
    digit
}

fn array(value: &[u8], number: usize) -> Result<(), Error> {
    for (index, axis) in value.split(|b| *b == b'^').enumerate() {
        let position = index.checked_add(1).ok_or(content("SAC", number))?;
        primitive(
            axis,
            position == 1,
            16,
            component_error("SAC", number, position),
        )?;
        if !axis.is_empty() && (!numeric(axis) || (position == 1 && axis.contains(&b'.'))) {
            return Err(component_error("SAC", number, position));
        }
    }
    // Every NA component is supported, so all axes count on receive as well.
    if characters(value) > 80 {
        return Err(content("SAC", number));
    }
    Ok(())
}

fn units(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    primitive(part(value, 1), false, 20, component_error("SAC", 24, 1))?;
    primitive(part(value, 2), true, 199, component_error("SAC", 24, 2))?;
    if part(value, 1).is_empty() {
        excluded(part(value, 3), direction, component_error("SAC", 24, 3))?;
    } else if part(value, 3) != b"UCUM" {
        return Err(component_error("SAC", 24, 3));
    }
    extras(value, b'^', &[1, 2, 3], direction, |position| {
        component_error("SAC", 24, position)
    })?;
    super::order::limit(value, "SAC", 24, 250, direction)
}

fn dilution(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    excluded(part(value, 1), direction, component_error("SAC", 29, 1))?;
    if part(value, 2) != b"1" {
        return Err(component_error("SAC", 29, 2));
    }
    if part(value, 3) != b":" {
        return Err(component_error("SAC", 29, 3));
    }
    let denominator = part(value, 4);
    primitive(denominator, true, 15, component_error("SAC", 29, 4))?;
    if !numeric(denominator)
        || denominator.first() == Some(&b'-')
        || !denominator.iter().any(|byte| matches!(byte, b'1'..=b'9'))
    {
        return Err(component_error("SAC", 29, 4));
    }
    extras(value, b'^', &[2, 3, 4], direction, |position| {
        component_error("SAC", 29, position)
    })?;
    super::order::limit(value, "SAC", 29, 20, direction)
}

fn container(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(
        segment,
        "SAC",
        &[3, 4, 9, 10, 11, 13, 14, 15, 21, 22, 24, 29],
        direction,
    )?;
    // C.11: manager position identification is available only through a parent.
    // Restricted DC replies have separate query-echo predicates.
    if direction == ControlDirection::Send
        && !profile.negative_query
        && (!raw(segment, 3).is_empty() || raw(segment, 4).is_empty())
    {
        for number in [10, 11, 13, 14] {
            excluded(raw(segment, number), direction, content("SAC", number))?;
        }
    }
    for number in [3, 4, 10, 13] {
        let value = raw(segment, number);
        if number == 3
            && value == b"\"\""
            && (direction == ControlDirection::Receive || profile.negative_query)
        {
            continue;
        }
        field(segment, "SAC", number, false, false)?;
        if !value.is_empty() {
            ei(value, number, direction)?;
        }
    }
    if !profile.negative_query && raw(segment, 3).is_empty() && raw(segment, 4).is_empty() {
        return Err(content("SAC", 3));
    }
    if direction == ControlDirection::Send
        && !profile.negative_query
        && raw(segment, 3).is_empty()
        && !raw(segment, 4).is_empty()
        && raw(segment, 10).is_empty()
        && raw(segment, 13).is_empty()
    {
        return Err(content("SAC", 10));
    }
    validate_container_position(segment, direction)?;
    if option_field(
        segment,
        "SAC",
        9,
        profile.container && direction == ControlDirection::Send,
        direction,
        false,
    )? {
        coded(raw(segment, 9), "SAC", 9, b"HL70378", direction)?;
        if part(raw(segment, 9), 3) != b"HL70378" {
            return Err(component_error("SAC", 9, 3));
        }
    }
    for number in [21, 22] {
        if option_field(
            segment,
            "SAC",
            number,
            profile.container && direction == ControlDirection::Send,
            direction,
            false,
        )? {
            scalar(segment, "SAC", number, false, 20)?;
            if !numeric(raw(segment, number)) {
                return Err(content("SAC", number));
            }
        }
    }
    if option_field(
        segment,
        "SAC",
        24,
        profile.container && direction == ControlDirection::Send,
        direction,
        false,
    )? {
        units(raw(segment, 24), direction)?;
    }
    if option_field(
        segment,
        "SAC",
        29,
        profile.container && direction == ControlDirection::Send,
        direction,
        false,
    )? {
        dilution(raw(segment, 29), direction)?;
    }
    Ok(())
}

/// Applies the manager's specimen/container field profiles with resolved options.
pub(super) fn validate(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    match segment.id() {
        b"SPM" => specimen(segment, profile, direction),
        b"SAC" => container(segment, profile, direction),
        _ => Err(content("specimen profile", 0)),
    }
}

fn validate_role_and_type(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    field(segment, "SPM", 11, true, false)?;
    let role_value = raw(segment, 11);
    let identifier = part(role_value, 1);
    let system = part(role_value, 3);
    coded(
        role_value,
        "SPM",
        11,
        if identifier == b"U" {
            b"IHELAW"
        } else {
            b"HL70369"
        },
        direction,
    )?;
    // A vendor may reuse a spelling; only standard-system codes carry LAW's
    // patient/control/unknown predicates. Never infer a clinical role from text alone.
    let role = if matches!(system, b"HL70369" | b"IHELAW") {
        identifier
    } else {
        b""
    };
    if role == b"U" && system != b"IHELAW" {
        return Err(component_error("SPM", 11, 3));
    }
    if (role == b"U" && !profile.negative_query)
        || (direction == ControlDirection::Send && role == b"L" && !profile.pooling_outside)
    {
        return Err(content("SPM", 11));
    }
    if profile.pooling_outside && role != b"L" {
        return Err(content("SPM", 11));
    }
    let specimen_type = raw(segment, 4);
    if specimen_type != b"\"\"" {
        field(segment, "SPM", 4, true, false)?;
    }
    if matches!(role, b"Q" | b"U") {
        if specimen_type != b"\"\"" {
            return Err(content("SPM", 4));
        }
    } else if specimen_type == b"\"\"" {
        if direction == ControlDirection::Send {
            return Err(content("SPM", 4));
        }
    } else {
        coded(specimen_type, "SPM", 4, b"HL70487", direction)?;
    }
    Ok(())
}

fn validate_specimen_times(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    for number in [17, 18] {
        if option_field(
            segment,
            "SPM",
            number,
            profile.specimen && direction == ControlDirection::Send,
            direction,
            false,
        )? {
            let start = part(raw(segment, number), 1);
            let timestamp = if number == 17 {
                start.split(|b| *b == b'&').next().unwrap_or_default()
            } else {
                start
            };
            let error = if number == 17 {
                sub(17, 1)
            } else {
                component_error("SPM", 18, 1)
            };
            primitive(timestamp, true, 14, error)?;
            if !super::profile::timestamp(timestamp, false) {
                return Err(if number == 17 {
                    sub(17, 1)
                } else {
                    component_error("SPM", 18, 1)
                });
            }
            if number == 17 {
                extras(start, b'&', &[1], direction, |position| sub(17, position))?;
            }
            extras(raw(segment, number), b'^', &[1], direction, |position| {
                component_error("SPM", number, position)
            })?;
            super::order::limit(raw(segment, number), "SPM", number, 26, direction)?;
        }
    }
    Ok(())
}

fn validate_container_position(
    segment: Segment<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    for (identity, position) in [(10, 11), (13, 14)] {
        let value = raw(segment, position);
        if direction == ControlDirection::Send && raw(segment, identity).is_empty() {
            excluded(value, direction, content("SAC", position))?;
        } else {
            field(
                segment,
                "SAC",
                position,
                direction == ControlDirection::Send,
                false,
            )?;
            if !value.is_empty() {
                array(value, position)?;
            }
        }
    }
    let location = raw(segment, 15);
    if direction == ControlDirection::Send
        && raw(segment, 10).is_empty()
        && raw(segment, 13).is_empty()
    {
        excluded(location, direction, content("SAC", 15))?;
    } else {
        field(segment, "SAC", 15, false, true)?;
        if !location.is_empty() {
            for value in location.split(|b| *b == b'~') {
                coded(value, "SAC", 15, b"", direction)?;
            }
        }
    }
    Ok(())
}
