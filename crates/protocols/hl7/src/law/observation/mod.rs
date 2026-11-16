//! Validates LAW OBX observation segments per direction (TF-2b C.2/C.5/C.7); NTE and INV are in `notes_inventory`.
use super::fields::{characters, component_error, excluded_fields, field, part, primitive, scalar};
use super::order::limit;
use super::patient::{authority_value, coded, excluded, extras};
use super::{ControlDirection, Error, Profile, ResultStatus, content, raw};
use crate::Segment;

fn location(number: usize, component: usize) -> Error {
    component_error("OBX", number, component)
}

fn vendor(system: &[u8]) -> bool {
    system.len() == 5
        && system.starts_with(b"99")
        && system
            .get(2..)
            .is_some_and(|v| v.iter().all(u8::is_ascii_alphanumeric))
}

fn test_system(system: &[u8]) -> bool {
    matches!(system, b"LN" | b"JC10") || vendor(system)
}

fn components(
    value: &[u8],
    number: usize,
    rules: &[(usize, bool, usize)],
    direction: ControlDirection,
) -> Result<(), Error> {
    for &(position, required, maximum) in rules {
        primitive(
            part(value, position),
            required,
            maximum,
            location(number, position),
        )?;
    }
    let supported: Vec<_> = rules.iter().map(|rule| rule.0).collect();
    extras(value, b'^', &supported, direction, |pos| {
        location(number, pos)
    })
}

fn observation_code(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    for (pos, required, maximum) in [
        (1, true, 20),
        (2, true, 199),
        (3, true, 12),
        (4, false, 20),
        (10, false, 20),
    ] {
        primitive(part(value, pos), required, maximum, location(3, pos))?;
    }
    if !test_system(part(value, 3)) {
        return Err(location(3, 3));
    }
    for (id, text, system) in [(4, 5, 6), (10, 11, 12)] {
        if part(value, id).is_empty() {
            for pos in [text, system] {
                excluded(part(value, pos), direction, location(3, pos))?;
            }
        } else {
            primitive(part(value, text), true, 199, location(3, text))?;
            primitive(part(value, system), true, 6, location(3, system))?;
            let namespace = part(value, system);
            if id == 4 && namespace == b"IHELAW" {
                if !matches!(
                    part(value, id),
                    b"S_IMAGE" | b"S_GRAPH" | b"S_RAW" | b"S_OTHER"
                ) {
                    return Err(location(3, id));
                }
            } else if !test_system(namespace) {
                return Err(location(3, system));
            }
        }
    }
    extras(
        value,
        b'^',
        &[1, 2, 3, 4, 5, 6, 10, 11, 12],
        direction,
        |pos| location(3, pos),
    )?;
    limit(value, "OBX", 3, 250, direction)
}

fn positive(value: &[u8], maximum: usize, error: Error) -> Result<(), Error> {
    if !value.iter().all(u8::is_ascii_digit) || value.iter().all(|v| *v == b'0') {
        return Err(error);
    }
    primitive(value, true, maximum, error)
}

fn sub_id(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    positive(part(value, 1), 20, location(4, 1))?;
    for position in [2, 3] {
        let item = part(value, position);
        if !item.is_empty() {
            positive(item, 5, location(4, position))?;
        }
    }
    extras(value, b'^', &[1, 2, 3], direction, |pos| location(4, pos))?;
    limit(value, "OBX", 4, 20, direction)
}

fn timestamp(
    value: &[u8],
    name: &'static str,
    number: usize,
    direction: ControlDirection,
) -> Result<(), Error> {
    primitive(part(value, 1), true, 14, component_error(name, number, 1))?;
    if !super::profile::timestamp(part(value, 1), false) {
        return Err(component_error(name, number, 1));
    }
    extras(value, b'^', &[1], direction, |pos| {
        component_error(name, number, pos)
    })?;
    limit(value, name, number, 14, direction)
}

fn numeric(value: &[u8], maximum: usize, error: Error) -> Result<(), Error> {
    if !super::specimen::numeric(value) {
        return Err(error);
    }
    primitive(value, true, maximum, error)
}

fn data_type(value: &[u8]) -> bool {
    matches!(
        value,
        b"AP" | b"AU" | b"FT" | b"IM" | b"multipart" | b"NS" | b"SD" | b"SI" | b"TEXT" | b"TX"
    )
}

fn subtype(value: &[u8]) -> bool {
    value.is_empty()
        || matches!(
            value,
            b"BASIC"
                | b"DICOM"
                | b"FAX"
                | b"GIF"
                | b"HTML"
                | b"JOT"
                | b"JPEG"
                | b"Octet-stream"
                | b"PICT"
                | b"PostScript"
                | b"RTF"
                | b"SGML"
                | b"TIFF"
                | b"x-hl7-cda-level-one"
                | b"XML"
        )
        || value.starts_with(b"Z")
        || value.starts_with(b"z")
}

fn embedded(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    let source = part(value, 1);
    primitive(
        source.split(|b| *b == b'&').next().unwrap_or_default(),
        false,
        20,
        Error::Subcomponent {
            segment: "OBX",
            field: 5,
            component: 1,
            subcomponent: 1,
        },
    )?;
    extras(source, b'&', &[1], direction, |subcomponent| {
        Error::Subcomponent {
            segment: "OBX",
            field: 5,
            component: 1,
            subcomponent,
        }
    })?;
    for (pos, required, maximum) in [
        (2, true, 9),
        (3, false, 18),
        (4, true, 6),
        (5, true, usize::MAX),
    ] {
        primitive(part(value, pos), required, maximum, location(5, pos))?;
    }
    if !data_type(part(value, 2)) {
        return Err(location(5, 2));
    }
    if !subtype(part(value, 3)) {
        return Err(location(5, 3));
    }
    let bytes = part(value, 5);
    let valid = match part(value, 4) {
        b"A" => bytes.is_ascii(),
        b"Hex" => bytes.len().is_multiple_of(2) && bytes.iter().all(u8::is_ascii_hexdigit),
        b"Base64" => {
            let data = bytes.split(|b| *b == b'=').next().unwrap_or_default();
            let padding = bytes.len().checked_sub(data.len()).ok_or(location(5, 5))?;
            bytes.len().is_multiple_of(4)
                && padding <= 2
                && data
                    .iter()
                    .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'+' | b'/'))
                && bytes
                    .get(data.len()..)
                    .is_some_and(|v| v.iter().all(|b| *b == b'='))
        }
        _ => return Err(location(5, 4)),
    };
    if !valid {
        return Err(location(5, 5));
    }
    extras(value, b'^', &[1, 2, 3, 4, 5], direction, |pos| {
        location(5, pos)
    })
}

fn pointer(value: &[u8], profile: Profile<'_>, direction: ControlDirection) -> Result<(), Error> {
    for (pos, required, maximum) in [(1, true, 999), (3, false, 9), (4, false, 19)] {
        primitive(part(value, pos), required, maximum, location(5, pos))?;
    }
    let application = part(value, 2);
    if !application.is_empty() {
        let mut parts = application.split(|b| *b == b'&');
        let namespace = parts.next().unwrap_or_default();
        let universal = parts.next().unwrap_or_default();
        let kind = parts.next().unwrap_or_default();
        // TF-2x A.4 applies here; PID/OBR have separate authority overrides.
        if namespace.is_empty()
            || (!universal.is_empty() && kind != b"ISO")
            || (universal.is_empty() && !kind.is_empty())
        {
            return Err(location(5, 2));
        }
    }
    authority_value(part(value, 2), "OBX", 5, 2, profile, direction)?;
    // RP permits vendor-defined type/subtype values and never retrieves the pointer.
    extras(value, b'^', &[1, 2, 3, 4], direction, |pos| {
        location(5, pos)
    })
}

fn structured(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    primitive(part(value, 1), true, 2, location(5, 1))?;
    if !matches!(part(value, 1), b"<" | b">" | b"<=" | b">=" | b"=" | b"<>") {
        return Err(location(5, 1));
    }
    numeric(part(value, 2), 15, location(5, 2))?;
    primitive(part(value, 3), false, 1, location(5, 3))?;
    if !matches!(part(value, 3), b"" | b"-" | b"+" | b"/" | b"." | b":") {
        return Err(location(5, 3));
    }
    if !part(value, 4).is_empty() {
        numeric(part(value, 4), 15, location(5, 4))?;
    }
    extras(value, b'^', &[1, 2, 3, 4], direction, |pos| {
        location(5, pos)
    })
}

fn observation_value(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    let value = raw(segment, 5);
    if value.is_empty() {
        return Err(content("OBX", 5));
    }
    if value == b"\"\"" {
        return excluded(raw(segment, 2), direction, content("OBX", 2));
    }
    scalar(segment, "OBX", 2, true, 2)?;
    let repeating = part(raw(segment, 3), 6) == b"IHELAW"
        && matches!(part(raw(segment, 3), 4), b"S_RAW" | b"S_OTHER");
    field(segment, "OBX", 5, true, repeating)?;
    for value in value.split(|b| *b == b'~') {
        if characters(value) > 99999 {
            return Err(content("OBX", 5));
        }
        match raw(segment, 2) {
            b"CE" => {
                components(
                    value,
                    5,
                    &[(1, true, 20), (2, true, 199), (3, true, 12)],
                    direction,
                )?;
            }
            b"ED" => embedded(value, direction)?,
            b"EI" => components(value, 5, &[(1, true, 199)], direction)?,
            b"NM" => numeric(value, 16, content("OBX", 5))?,
            b"NA" => {
                for (index, axis) in value.split(|b| *b == b'^').enumerate() {
                    if index == 0 || !axis.is_empty() {
                        numeric(
                            axis,
                            16,
                            location(5, index.checked_add(1).ok_or(content("OBX", 5))?),
                        )?;
                    }
                }
            }
            b"RP" => pointer(value, profile, direction)?,
            b"SN" => structured(value, direction)?,
            b"ST" => primitive(value, true, 200, content("OBX", 5))?,
            b"TX" => primitive(value, true, 65536, content("OBX", 5))?,
            _ => return Err(content("OBX", 2)),
        }
    }
    Ok(())
}

fn units(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    if value == b"\"\"" {
        return Ok(());
    }
    for (position, required, maximum) in [(1, false, 20), (2, true, 199)] {
        primitive(
            part(value, position),
            required,
            maximum,
            location(6, position),
        )?;
    }
    if part(value, 1).is_empty() {
        excluded(part(value, 3), direction, location(6, 3))?;
    } else if part(value, 3) != b"UCUM" {
        return Err(location(6, 3));
    }
    extras(value, b'^', &[1, 2, 3], direction, |pos| location(6, pos))?;
    limit(value, "OBX", 6, 250, direction)
}

fn observer(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    let id = part(value, 1);
    if id != b"\"\"" {
        primitive(id, true, 15, location(16, 1))?;
    }
    extras(value, b'^', &[1], direction, |pos| location(16, pos))?;
    limit(value, "OBX", 16, 250, direction)
}

fn equipment(value: &[u8], ordinal: usize, direction: ControlDirection) -> Result<(), Error> {
    for (pos, required, maximum) in [(1, ordinal < 2, 50), (2, ordinal < 2, 20)] {
        primitive(part(value, pos), required, maximum, location(18, pos))?;
    }
    if ordinal == 0 {
        primitive(part(value, 3), false, 199, location(18, 3))?;
        primitive(part(value, 4), false, 6, location(18, 4))?;
        if (!part(value, 3).is_empty() || !part(value, 4).is_empty())
            && !crate::datatypes::universal_id(part(value, 3), part(value, 4))
        {
            return Err(location(18, 4));
        }
    } else {
        for pos in [3, 4] {
            excluded(part(value, pos), direction, location(18, pos))?;
        }
    }
    extras(value, b'^', &[1, 2, 3, 4], direction, |pos| {
        location(18, pos)
    })?;
    limit(value, "OBX", 18, 427, direction)
}

fn probability(value: &[u8]) -> Result<(), Error> {
    numeric(value, 5, content("OBX", 9))?;
    let unsigned = value
        .strip_prefix(b"+")
        .or_else(|| value.strip_prefix(b"-"))
        .unwrap_or(value);
    let mut pieces = unsigned.split(|b| *b == b'.');
    let integer = pieces.next().unwrap_or_default();
    let fraction = pieces.next().unwrap_or_default();
    let nonzero = unsigned.iter().any(|b| matches!(b, b'1'..=b'9'));
    let normalized = integer
        .iter()
        .position(|b| *b != b'0')
        .and_then(|i| integer.get(i..))
        .unwrap_or_default();
    if (value.starts_with(b"-") && nonzero)
        || !(normalized.is_empty() || (normalized == b"1" && fraction.iter().all(|b| *b == b'0')))
    {
        return Err(content("OBX", 9));
    }
    Ok(())
}

fn obx(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(
        segment,
        "OBX",
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 18, 19, 21, 29],
        direction,
    )?;
    scalar(segment, "OBX", 1, true, 4)?;
    positive(raw(segment, 1), 4, content("OBX", 1))?;
    field(segment, "OBX", 3, true, false)?;
    observation_code(raw(segment, 3), direction)?;
    field(
        segment,
        "OBX",
        4,
        direction == ControlDirection::Receive,
        false,
    )?;
    if !raw(segment, 4).is_empty() {
        sub_id(raw(segment, 4), direction)?;
    }
    observation_value(segment, profile, direction)?;
    let numeric = matches!(raw(segment, 2), b"NM" | b"SN") && raw(segment, 5) != b"\"\"";
    if numeric {
        if raw(segment, 6).is_empty() || raw(segment, 6).contains(&b'~') {
            return Err(content("OBX", 6));
        }
        units(raw(segment, 6), direction)?;
    } else {
        excluded(raw(segment, 6), direction, content("OBX", 6))?;
    }
    scalar(segment, "OBX", 7, false, 70)?;
    validate_flags(segment, direction)?;
    if direction == ControlDirection::Receive && profile.results.mass_spectrometry {
        scalar(segment, "OBX", 9, false, 5)?;
        if !raw(segment, 9).is_empty() {
            probability(raw(segment, 9))?;
        }
    } else {
        excluded(raw(segment, 9), direction, content("OBX", 9))?;
    }
    scalar(segment, "OBX", 11, true, 1)?;
    ResultStatus::parse(raw(segment, 11))?;
    if direction == ControlDirection::Send {
        field(segment, "OBX", 14, false, false)?;
        if !raw(segment, 14).is_empty() {
            timestamp(raw(segment, 14), "OBX", 14, direction)?;
        }
    }
    let observers = raw(segment, 16);
    if observers.is_empty() {
        return Err(content("OBX", 16));
    }
    for value in observers.split(|b| *b == b'~') {
        observer(value, direction)?;
    }
    field(segment, "OBX", 18, true, true)?;
    if raw(segment, 18).split(|b| *b == b'~').count() < 2 {
        return Err(content("OBX", 18));
    }
    for (ordinal, value) in raw(segment, 18).split(|b| *b == b'~').enumerate() {
        equipment(value, ordinal, direction)?;
    }
    field(segment, "OBX", 19, true, false)?;
    timestamp(raw(segment, 19), "OBX", 19, direction)?;
    field(segment, "OBX", 21, false, false)?;
    if !raw(segment, 21).is_empty() {
        components(raw(segment, 21), 21, &[(1, true, 50)], direction)?;
        limit(raw(segment, 21), "OBX", 21, 427, direction)?;
    }
    scalar(segment, "OBX", 29, true, 4)?;
    if !matches!(raw(segment, 29), b"AOE" | b"ASC" | b"RSLT" | b"SCI") {
        return Err(content("OBX", 29));
    }
    Ok(())
}

/// Checks C.2/C.5/C.7 with the sender's usages and preserves ignored receive data.
pub(super) fn validate(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    match segment.id() {
        b"OBX" => obx(segment, profile, direction),
        b"NTE" => nte(segment, direction),
        b"INV" => inv(segment, profile, direction),
        _ => Err(content("observation profile", 0)),
    }
}

fn validate_flags(segment: Segment<'_>, direction: ControlDirection) -> Result<(), Error> {
    // C.7-1 retains a five-character length from the old ID datatype. That cannot
    // hold C.7-17's required CWE namespace. Its explicit component profile governs.
    let flags = raw(segment, 8);
    if flags.is_empty() {
        return Err(content("OBX", 8));
    }
    if flags != b"\"\"" {
        for flag in flags.split(|b| *b == b'~') {
            components(
                flag,
                8,
                &[(1, true, 20), (2, false, 199), (3, true, 12)],
                direction,
            )?;
            if part(flag, 3) != b"HL70078" && !vendor(part(flag, 3)) {
                return Err(location(8, 3));
            }
            if part(flag, 3) == b"HL70078"
                && !matches!(
                    part(flag, 1),
                    b"<" | b">"
                        | b"L"
                        | b"H"
                        | b"LL"
                        | b"HH"
                        | b"N"
                        | b"A"
                        | b"POS"
                        | b"NEG"
                        | b"IND"
                        | b"QCF"
                        | b"S"
                        | b"I"
                        | b"R"
                        | b"SDD"
                        | b"IE"
                )
            {
                return Err(location(8, 1));
            }
        }
    }
    Ok(())
}

mod notes_inventory;
use notes_inventory::{inv, nte};
