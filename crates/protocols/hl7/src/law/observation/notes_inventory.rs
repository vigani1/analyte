//! Validates LAW NTE comment and INV substance segments without interpreting their content.
use super::*;
pub(super) fn nte(segment: Segment<'_>, direction: ControlDirection) -> Result<(), Error> {
    excluded_fields(segment, "NTE", &[1, 2, 3, 4], direction)?;
    scalar(segment, "NTE", 1, true, 4)?;
    positive(raw(segment, 1), 4, content("NTE", 1))?;
    scalar(segment, "NTE", 2, true, 8)?;
    let source = match direction {
        ControlDirection::Send => b"A",
        ControlDirection::Receive => b"Z",
    };
    if raw(segment, 2) != source {
        return Err(content("NTE", 2));
    }
    field(segment, "NTE", 3, true, false)?;
    text(
        raw(segment, 3),
        true,
        usize::MAX,
        content("NTE", 3),
        TextKind::Ft,
        direction,
    )?;
    field(segment, "NTE", 4, false, false)?;
    if !raw(segment, 4).is_empty() {
        let value = raw(segment, 4);
        for (position, required, maximum) in [(1, true, 20), (2, false, 199), (3, true, 12)] {
            primitive(
                part(value, position),
                required,
                maximum,
                component_error("NTE", 4, position),
            )?;
        }
        let system = part(value, 3);
        // C.5-3 explicitly permits an extended vendor namespace (99zzz...), up to 12.
        if system.len() < 5
            || !system.starts_with(b"99")
            || !system
                .get(2..)
                .is_some_and(|v| v.iter().all(u8::is_ascii_alphanumeric))
        {
            return Err(component_error("NTE", 4, 3));
        }
        extras(value, b'^', &[1, 2, 3], direction, |position| {
            component_error("NTE", 4, position)
        })?;
        limit(value, "NTE", 4, 250, direction)?;
    }
    Ok(())
}

pub(super) fn inv(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    if direction == ControlDirection::Send || !profile.results.contributing_substances {
        return excluded(segment.raw(), direction, content("INV", 0));
    }
    excluded_fields(segment, "INV", &[1, 2, 3, 4, 12, 16], direction)?;
    for (number, system) in [
        (1, b"HL70451".as_slice()),
        (2, b"HL70383"),
        (3, b"HL70384"),
        (4, b""),
    ] {
        field(segment, "INV", number, number != 4, false)?;
        let value = raw(segment, number);
        if !value.is_empty() {
            if number == 1 {
                for (position, required, maximum) in [(1, true, 50), (2, false, 199), (3, true, 7)]
                {
                    primitive(
                        part(value, position),
                        required,
                        maximum,
                        component_error("INV", number, position),
                    )?;
                }
                if part(value, 3) != system && !vendor(part(value, 3)) {
                    return Err(component_error("INV", number, 3));
                }
                extras(value, b'^', &[1, 2, 3], direction, |position| {
                    component_error("INV", number, position)
                })?;
            } else {
                coded(value, "INV", number, system, direction)?;
            }
            let maximum = if number == 4 { 5 } else { 7 };
            primitive(
                part(value, 3),
                true,
                maximum,
                component_error("INV", number, 3),
            )?;
            if number == 2
                && part(value, 3) == b"HL70383"
                && !matches!(
                    part(value, 1),
                    b"EW"
                        | b"EE"
                        | b"CW"
                        | b"CE"
                        | b"QW"
                        | b"QE"
                        | b"NW"
                        | b"NE"
                        | b"OW"
                        | b"OE"
                        | b"OK"
                        | b"NA"
                )
            {
                return Err(component_error("INV", 2, 1));
            }
            if number == 3
                && part(value, 3) == b"HL70384"
                && !matches!(
                    part(value, 1),
                    b"SR"
                        | b"MR"
                        | b"DI"
                        | b"PT"
                        | b"RC"
                        | b"CO"
                        | b"PW"
                        | b"LW"
                        | b"SW"
                        | b"SC"
                        | b"LI"
                        | b"OT"
                )
            {
                return Err(component_error("INV", 3, 1));
            }
        }
    }
    field(segment, "INV", 12, false, false)?;
    if !raw(segment, 12).is_empty() {
        timestamp(raw(segment, 12), "INV", 12, direction)?;
    }
    scalar(segment, "INV", 16, false, 50)
}
