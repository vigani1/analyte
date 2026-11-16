//! Validates LAW patient and visit segments (TF-2b C.9/C.10).
use super::fields::{characters, component_error, excluded_fields, field, part, primitive, scalar};
use super::{ControlDirection, Error, Profile, content, raw};
use crate::Segment;

/// A trusted association between a local authority name and its universal identity.
/// The host supplies the association; syntactic validity alone cannot prove it.
#[derive(Debug, Clone, Copy)]
pub struct AuthorityBinding<'a> {
    namespace: &'a [u8],
    universal: &'a [u8],
    kind: &'a [u8],
}

impl<'a> AuthorityBinding<'a> {
    /// Validates the shape of an association asserted by the host configuration.
    /// # Errors
    /// Returns a PID-3.4 subcomponent location for invalid authority primitives.
    pub fn new(namespace: &'a [u8], universal: &'a [u8], kind: &'a [u8]) -> Result<Self, Error> {
        for (position, value, maximum) in [(1, namespace, 20), (2, universal, 199), (3, kind, 6)] {
            primitive(value, true, maximum, sub(4, position))?;
        }
        universal_value(universal, kind, sub(4, 2))?;
        Ok(Self {
            namespace,
            universal,
            kind,
        })
    }

    pub(super) fn matches(self, namespace: &[u8], universal: &[u8], kind: &[u8]) -> bool {
        self.namespace == namespace && self.universal == universal && self.kind == kind
    }
}

fn sub(component: usize, subcomponent: usize) -> Error {
    Error::Subcomponent {
        segment: "PID",
        field: 3,
        component,
        subcomponent,
    }
}

pub(super) fn universal_value(value: &[u8], kind: &[u8], error: Error) -> Result<(), Error> {
    if crate::datatypes::universal_id(value, kind) {
        Ok(())
    } else {
        Err(error)
    }
}

/// Omits unsupported elements when sending, and ignores them when receiving.
pub(super) fn excluded(
    value: &[u8],
    direction: ControlDirection,
    error: Error,
) -> Result<(), Error> {
    if direction == ControlDirection::Send && !value.is_empty() {
        return Err(error);
    }
    Ok(())
}

/// Checks only supported composite positions; every other position has X usage.
pub(super) fn extras(
    value: &[u8],
    separator: u8,
    supported: &[usize],
    direction: ControlDirection,
    error: impl Fn(usize) -> Error,
) -> Result<(), Error> {
    for (index, value) in value.split(|byte| *byte == separator).enumerate() {
        let position = index.checked_add(1).ok_or_else(|| error(0))?;
        if !supported.contains(&position) {
            excluded(value, direction, error(position))?;
        }
    }
    Ok(())
}

/// Validates a profiled CE/CWE using a supplied standard namespace or vendor namespace.
pub(super) fn coded(
    value: &[u8],
    name: &'static str,
    number: usize,
    standard: &[u8],
    direction: ControlDirection,
) -> Result<(), Error> {
    for (position, required, maximum) in [(1, true, 20), (2, false, 199), (3, true, 12)] {
        primitive(
            part(value, position),
            required,
            maximum,
            component_error(name, number, position),
        )?;
    }
    let system = part(value, 3);
    if system != standard && !vendor(system) {
        return Err(component_error(name, number, 3));
    }
    extras(value, b'^', &[1, 2, 3], direction, |position| {
        component_error(name, number, position)
    })?;
    if direction == ControlDirection::Send && characters(value) > 250 {
        return Err(content(name, number));
    }
    Ok(())
}

fn vendor(system: &[u8]) -> bool {
    system.len() == 5
        && system.starts_with(b"99")
        && system
            .get(2..)
            .is_some_and(|suffix| suffix.iter().all(u8::is_ascii_alphanumeric))
}

/// Checks the legal-name profile while preserving every truncatable name primitive.
fn name(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    for position in [2, 3, 4] {
        primitive(
            part(value, position),
            false,
            usize::MAX,
            component_error("PID", 5, position),
        )?;
    }
    let family = part(value, 1);
    let surname = family.split(|b| *b == b'&').next().unwrap_or_default();
    primitive(
        surname,
        false,
        usize::MAX,
        Error::Subcomponent {
            segment: "PID",
            field: 5,
            component: 1,
            subcomponent: 1,
        },
    )?;
    extras(family, b'&', &[1], direction, |position| {
        Error::Subcomponent {
            segment: "PID",
            field: 5,
            component: 1,
            subcomponent: position,
        }
    })?;
    primitive(part(value, 7), true, 1, component_error("PID", 5, 7))?;
    if part(value, 7) != b"L" {
        return Err(component_error("PID", 5, 7));
    }
    extras(value, b'^', &[1, 2, 3, 4, 7], direction, |position| {
        component_error("PID", 5, position)
    })
}

fn identifier(
    value: &[u8],
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    primitive(part(value, 1), true, 50, component_error("PID", 3, 1))?;
    let authority = part(value, 4);
    authority_value(authority, "PID", 3, 4, profile, direction)?;
    extras(value, b'^', &[1, 4], direction, |position| {
        component_error("PID", 3, position)
    })?;
    if direction == ControlDirection::Send
        && (characters(authority) > 227 || characters(value) > 278)
    {
        return Err(content("PID", 3));
    }
    Ok(())
}

/// Validates a nested HD and checks mixed authority names against host bindings.
pub(super) fn authority_value(
    value: &[u8],
    name: &'static str,
    number: usize,
    component: usize,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    let location = |subcomponent| Error::Subcomponent {
        segment: name,
        field: number,
        component,
        subcomponent,
    };
    let mut parts = value.split(|b| *b == b'&');
    let namespace = parts.next().unwrap_or_default();
    let universal = parts.next().unwrap_or_default();
    let kind = parts.next().unwrap_or_default();
    primitive(namespace, false, 20, location(1))?;
    primitive(universal, false, 199, location(2))?;
    if universal.is_empty() {
        excluded(kind, direction, location(3))?;
    } else {
        primitive(kind, true, 6, location(3))?;
        universal_value(universal, kind, location(2))?;
    }
    if !namespace.is_empty() && !universal.is_empty() {
        let known = profile
            .authorities
            .iter()
            .any(|binding| binding.matches(namespace, universal, kind));
        let conflict = profile.authorities.iter().any(|binding| {
            binding.namespace == namespace && !binding.matches(namespace, universal, kind)
        });
        if !known || conflict {
            return Err(Error::Authority {
                segment: name,
                field: number,
                component,
            });
        }
    }
    extras(value, b'&', &[1, 2, 3], direction, |position| {
        location(position)
    })?;
    if direction == ControlDirection::Send && characters(value) > 227 {
        return Err(component_error(name, number, component));
    }
    Ok(())
}

fn birth(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    let timestamp = part(value, 1);
    primitive(timestamp, true, 14, component_error("PID", 7, 1))?;
    let valid = if timestamp.len() == 8 {
        let mut complete = timestamp.to_vec();
        complete.extend_from_slice(b"000000");
        super::profile::timestamp(&complete, false)
    } else {
        super::profile::timestamp(timestamp, false)
    };
    if !valid {
        return Err(component_error("PID", 7, 1));
    }
    extras(value, b'^', &[1], direction, |position| {
        component_error("PID", 7, position)
    })?;
    if direction == ControlDirection::Send && characters(value) > 26 {
        return Err(content("PID", 7));
    }
    Ok(())
}

fn patient(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(segment, "PID", &[3, 5, 7, 8, 10, 35], direction)?;
    for number in [3, 5] {
        // C.9 explicitly permits null when an identifier/name is not applicable.
        let value = raw(segment, number);
        if value == b"\"\"" {
            continue;
        }
        field(segment, "PID", number, true, false)?;
        match number {
            3 => identifier(value, profile, direction)?,
            5 => name(value, direction)?,
            _ => return Err(content("PID", number)),
        }
    }
    for number in [7, 8, 10, 35] {
        field(segment, "PID", number, false, false)?;
        let value = raw(segment, number);
        if value.is_empty() {
            continue;
        }
        match number {
            7 => birth(value, direction)?,
            8 => {
                scalar(segment, "PID", 8, false, 1)?;
                if !matches!(value, b"F" | b"M" | b"U") {
                    return Err(content("PID", 8));
                }
            }
            10 => coded(value, "PID", 10, b"HL70005", direction)?,
            35 => coded(value, "PID", 35, b"", direction)?,
            _ => return Err(content("PID", number)),
        }
    }
    Ok(())
}

fn visit(segment: Segment<'_>, direction: ControlDirection) -> Result<(), Error> {
    excluded_fields(segment, "PV1", &[2, 3], direction)?;
    scalar(segment, "PV1", 2, true, 1)?;
    if !matches!(
        raw(segment, 2),
        b"E" | b"I" | b"O" | b"P" | b"R" | b"B" | b"C" | b"N" | b"U"
    ) {
        return Err(content("PV1", 2));
    }
    field(segment, "PV1", 3, false, false)?;
    let value = raw(segment, 3);
    if !value.is_empty() {
        primitive(
            part(value, 2),
            true,
            usize::MAX,
            component_error("PV1", 3, 2),
        )?;
        extras(value, b'^', &[2], direction, |position| {
            component_error("PV1", 3, position)
        })?;
    }
    Ok(())
}

/// Checks the selected patient/visit option without inventing unavailable demographics.
pub(super) fn validate(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    if !profile.content.patient || profile.workflow.pooling_outside {
        return excluded(
            segment.raw(),
            direction,
            content(if segment.id() == b"PID" { "PID" } else { "PV1" }, 0),
        );
    }
    match segment.id() {
        b"PID" => patient(segment, profile, direction),
        b"PV1" => visit(segment, direction),
        _ => Err(content("patient profile", 0)),
    }
}
