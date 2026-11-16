//! Order and timing profiles from TF-2b C.6/C.8/C.14.
use super::control::{
    characters, component_error, excluded_fields, field, part, primitive, scalar,
};
use super::patient::{authority_value, coded, excluded, extras};
use super::{ControlDirection, Error, Profile, content, raw};
use crate::Segment;

/// LAW order-control codes, independent of message acknowledgment and app delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderControl {
    /// Request new work.
    New,
    /// Accept a new request.
    Accepted,
    /// Reject a new request.
    UnableToAccept,
    /// Request cancellation.
    Cancel,
    /// Confirm cancellation.
    Cancelled,
    /// Refuse cancellation.
    UnableToCancel,
    /// Report a genuine negative query answer.
    Discontinue,
    /// Report a change of analyzer work status.
    StatusChange,
}

impl OrderControl {
    /// Parses only the C.8.2 subset without inventing a default.
    /// # Errors
    /// Returns ORC-1 for an unsupported spelling.
    pub fn parse(value: &[u8]) -> Result<Self, Error> {
        match value {
            b"NW" => Ok(Self::New),
            b"OK" => Ok(Self::Accepted),
            b"UA" => Ok(Self::UnableToAccept),
            b"CA" => Ok(Self::Cancel),
            b"CR" => Ok(Self::Cancelled),
            b"UC" => Ok(Self::UnableToCancel),
            b"DC" => Ok(Self::Discontinue),
            b"SC" => Ok(Self::StatusChange),
            _ => Err(content("ORC", 1)),
        }
    }

    /// Returns the exact profiled code for this variant.
    #[must_use]
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::New => b"NW",
            Self::Accepted => b"OK",
            Self::UnableToAccept => b"UA",
            Self::Cancel => b"CA",
            Self::Cancelled => b"CR",
            Self::UnableToCancel => b"UC",
            Self::Discontinue => b"DC",
            Self::StatusChange => b"SC",
        }
    }

    fn permitted(self, profile: Profile<'_>, direction: ControlDirection) -> bool {
        match self {
            Self::New | Self::Cancel => {
                direction == ControlDirection::Send
                    && !(profile.context == crate::law::profile::MessageContext::NegativeQuery)
            }
            Self::Discontinue => {
                direction == ControlDirection::Send
                    && (profile.context == crate::law::profile::MessageContext::NegativeQuery)
            }
            Self::Accepted | Self::UnableToAccept | Self::Cancelled | Self::UnableToCancel => {
                direction == ControlDirection::Receive
                    && !(profile.context == crate::law::profile::MessageContext::Results)
            }
            Self::StatusChange => {
                direction == ControlDirection::Receive
                    && (profile.context == crate::law::profile::MessageContext::Results)
            }
        }
    }
}

/// Analyzer AWOS status from C.8.5; no transition or retry is inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    /// Accepted and scheduled, before the first run begins.
    Scheduled,
    /// Work has begun; observations may be absent.
    InProgress,
    /// Work is complete; subsequent corrections are still possible.
    Complete,
    /// Analyzer considers this AWOS cancelled or rejected.
    Cancelled,
}

impl OrderStatus {
    /// Parses the four profiled status codes.
    /// # Errors
    /// Returns ORC-5 for an unknown value.
    pub fn parse(value: &[u8]) -> Result<Self, Error> {
        match value {
            b"SC" => Ok(Self::Scheduled),
            b"IP" => Ok(Self::InProgress),
            b"CM" => Ok(Self::Complete),
            b"CA" => Ok(Self::Cancelled),
            _ => Err(content("ORC", 5)),
        }
    }

    /// Returns the exact profiled status code.
    #[must_use]
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Scheduled => b"SC",
            Self::InProgress => b"IP",
            Self::Complete => b"CM",
            Self::Cancelled => b"CA",
        }
    }
}

/// Enforces the encoded ceiling on send; ignored receive components do not count.
pub(super) fn limit(
    value: &[u8],
    name: &'static str,
    number: usize,
    maximum: usize,
    direction: ControlDirection,
) -> Result<(), Error> {
    if direction == ControlDirection::Send && characters(value) > maximum {
        return Err(content(name, number));
    }
    Ok(())
}

/// Checks the three required test identity components used by OBR and TCD.
pub(super) fn test_code(
    value: &[u8],
    name: &'static str,
    number: usize,
    direction: ControlDirection,
) -> Result<(), Error> {
    for (position, maximum) in [(1, 20), (2, 199), (3, 12)] {
        primitive(
            part(value, position),
            true,
            maximum,
            component_error(name, number, position),
        )?;
    }
    let system = part(value, 3);
    let vendor = system.len() == 5
        && system.starts_with(b"99")
        && system
            .get(2..)
            .is_some_and(|tail| tail.iter().all(u8::is_ascii_alphanumeric));
    if !matches!(system, b"LN" | b"JC10") && !vendor {
        return Err(component_error(name, number, 3));
    }
    extras(value, b'^', &[1, 2, 3], direction, |position| {
        component_error(name, number, position)
    })?;
    limit(value, name, number, 250, direction)
}

fn ei(
    value: &[u8],
    name: &'static str,
    number: usize,
    direction: ControlDirection,
) -> Result<(), Error> {
    primitive(part(value, 1), true, 50, component_error(name, number, 1))?;
    extras(value, b'^', &[1], direction, |position| {
        component_error(name, number, position)
    })?;
    limit(value, name, number, 50, direction)
}

fn eip(value: &[u8], number: usize, direction: ControlDirection) -> Result<(), Error> {
    let placer = part(value, 1);
    let identifier = placer.split(|b| *b == b'&').next().unwrap_or_default();
    primitive(
        identifier,
        true,
        50,
        Error::Subcomponent {
            segment: "ORC",
            field: number,
            component: 1,
            subcomponent: 1,
        },
    )?;
    extras(placer, b'&', &[1], direction, |subcomponent| {
        Error::Subcomponent {
            segment: "ORC",
            field: number,
            component: 1,
            subcomponent,
        }
    })?;
    extras(value, b'^', &[1], direction, |position| {
        component_error("ORC", number, position)
    })?;
    limit(
        value,
        "ORC",
        number,
        if number == 4 { 50 } else { 200 },
        direction,
    )
}

fn time(
    value: &[u8],
    name: &'static str,
    number: usize,
    date_allowed: bool,
    direction: ControlDirection,
) -> Result<(), Error> {
    let stamp = part(value, 1);
    primitive(stamp, true, 14, component_error(name, number, 1))?;
    let valid = if date_allowed && stamp.len() == 8 {
        let mut full = [b'0'; 14];
        full.get_mut(..8)
            .ok_or(content(name, number))?
            .copy_from_slice(stamp);
        super::profile::timestamp(&full, false)
    } else {
        super::profile::timestamp(stamp, false)
    };
    if !valid {
        return Err(component_error(name, number, 1));
    }
    extras(value, b'^', &[1], direction, |position| {
        component_error(name, number, position)
    })?;
    limit(value, name, number, 26, direction)
}

fn provider(value: &[u8], profile: Profile<'_>, direction: ControlDirection) -> Result<(), Error> {
    primitive(part(value, 1), true, 15, component_error("OBR", 16, 1))?;
    for position in [3, 4, 5] {
        primitive(
            part(value, position),
            false,
            usize::MAX,
            component_error("OBR", 16, position),
        )?;
    }
    let family = part(value, 2);
    primitive(
        family.split(|b| *b == b'&').next().unwrap_or_default(),
        false,
        usize::MAX,
        Error::Subcomponent {
            segment: "OBR",
            field: 16,
            component: 2,
            subcomponent: 1,
        },
    )?;
    extras(family, b'&', &[1], direction, |subcomponent| {
        Error::Subcomponent {
            segment: "OBR",
            field: 16,
            component: 2,
            subcomponent,
        }
    })?;
    authority_value(part(value, 9), "OBR", 16, 9, profile, direction)?;
    // Name primitives marked # remain complete even beyond the nominal XCN size.
    extras(value, b'^', &[1, 2, 3, 4, 5, 9], direction, |position| {
        component_error("OBR", 16, position)
    })
}

fn orc(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(segment, "ORC", &[1, 2, 4, 5, 8, 9, 21, 27], direction)?;
    scalar(segment, "ORC", 1, true, 2)?;
    if !OrderControl::parse(raw(segment, 1))?.permitted(profile, direction) {
        return Err(content("ORC", 1));
    }
    if direction == ControlDirection::Receive {
        if !(profile.context == crate::law::profile::MessageContext::Results) {
            field(segment, "ORC", 2, true, false)?;
            ei(raw(segment, 2), "ORC", 2, direction)?;
        }
        scalar(segment, "ORC", 5, true, 2)?;
        OrderStatus::parse(raw(segment, 5))?;
        if profile.workflow.analyzer_reflex {
            field(segment, "ORC", 8, false, true)?;
            if !raw(segment, 8).is_empty() {
                for value in raw(segment, 8).split(|b| *b == b'~') {
                    eip(value, 8, direction)?;
                }
            }
        }
        field(segment, "ORC", 27, false, false)?;
        if !raw(segment, 27).is_empty() {
            time(raw(segment, 27), "ORC", 27, true, direction)?;
        }
    } else {
        for number in [2, 5, 8, 27] {
            excluded(raw(segment, number), direction, content("ORC", number))?;
        }
        field(segment, "ORC", 9, true, false)?;
        time(raw(segment, 9), "ORC", 9, false, direction)?;
    }
    field(segment, "ORC", 4, false, false)?;
    if !raw(segment, 4).is_empty() {
        eip(raw(segment, 4), 4, direction)?;
    }
    field(segment, "ORC", 21, false, false)?;
    if !raw(segment, 21).is_empty() {
        primitive(
            part(raw(segment, 21), 1),
            true,
            usize::MAX,
            component_error("ORC", 21, 1),
        )?;
        extras(raw(segment, 21), b'^', &[1], direction, |position| {
            component_error("ORC", 21, position)
        })?;
    }
    Ok(())
}

fn obr(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(segment, "OBR", &[2, 3, 4, 11, 16, 49], direction)?;
    let awos = raw(segment, 2);
    if !(direction == ControlDirection::Receive && awos == b"\"\"") {
        field(segment, "OBR", 2, true, false)?;
        ei(awos, "OBR", 2, direction)?;
    }
    field(segment, "OBR", 4, true, false)?;
    test_code(raw(segment, 4), "OBR", 4, direction)?;
    if direction == ControlDirection::Send {
        for number in [3, 11] {
            excluded(raw(segment, number), direction, content("OBR", number))?;
        }
        field(segment, "OBR", 16, false, false)?;
        if !raw(segment, 16).is_empty() {
            provider(raw(segment, 16), profile, direction)?;
        }
    } else {
        field(segment, "OBR", 3, false, false)?;
        if !raw(segment, 3).is_empty() {
            ei(raw(segment, 3), "OBR", 3, direction)?;
        }
        if profile.workflow.analyzer_reflex {
            scalar(segment, "OBR", 11, false, 1)?;
            if !raw(segment, 11).is_empty() && raw(segment, 11) != b"G" {
                return Err(content("OBR", 11));
            }
        }
    }
    // C.6.1's AM column is RE (the following prose says X). Follow the
    // requested manager-column schema and make that discrepancy explicit in docs.
    if direction == ControlDirection::Send || profile.results.external_results {
        scalar(segment, "OBR", 49, false, 2)?;
        if !raw(segment, 49).is_empty() && raw(segment, 49) != b"RE" {
            return Err(content("OBR", 49));
        }
    }
    Ok(())
}

/// Checks order/timing content with direction-specific option predicates.
pub(super) fn validate(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    match segment.id() {
        b"ORC" => orc(segment, profile, direction),
        b"OBR" => obr(segment, profile, direction),
        b"TQ1" => {
            if !profile.priority.supported() {
                return excluded(segment.raw(), direction, content("TQ1", 0));
            }
            excluded_fields(segment, "TQ1", &[9], direction)?;
            field(segment, "TQ1", 9, true, false)?;
            coded(raw(segment, 9), "TQ1", 9, b"HL70485", direction)?;
            if part(raw(segment, 9), 3) == b"HL70485"
                && !matches!(part(raw(segment, 9), 1), b"R" | b"S")
            {
                return Err(component_error("TQ1", 9, 1));
            }
            Ok(())
        }
        _ => Err(content("order profile", 0)),
    }
}

/// Compares every supported test component without equating unknown extension data.
pub(super) fn same_test(left: &[u8], right: &[u8]) -> bool {
    [1, 2, 3]
        .into_iter()
        .all(|position| part(left, position) == part(right, position))
}

/// Checks the TCD reference and option-required request details within existing groups.
pub(super) fn validate_links(
    message: &crate::Message,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    let required = match direction {
        ControlDirection::Send => {
            profile.dilution.required()
                || profile.repeat.manager_repeat
                || profile.repeat.repeat_control
                || profile.workflow.pooling_analyzer
        }
        ControlDirection::Receive => {
            profile.dilution.required() || profile.workflow.pooling_analyzer
        }
    };
    let mut reference = None;
    let mut awaiting = false;
    let mut in_order = false;
    let mut needs_priority = false;
    let mut tcd_occurrence = 0_usize;
    for segment in message.segments() {
        match segment.id() {
            b"OBR" if direction == ControlDirection::Send => {
                if awaiting {
                    return Err(content("TCD", 0));
                }
                reference = Some(raw(segment, 4));
                awaiting = required;
            }
            b"OBR" => {
                if needs_priority {
                    return Err(content("TQ1", 0));
                }
                needs_priority = profile.priority.required()
                    && (profile.context == crate::law::profile::MessageContext::Results);
                if awaiting {
                    return Err(content("TCD", 0));
                }
                in_order = true;
                reference = None;
            }
            b"OBX" if direction == ControlDirection::Receive => {
                if needs_priority {
                    return Err(content("TQ1", 0));
                }
                if awaiting {
                    return Err(content("TCD", 0));
                }
                reference = Some(raw(segment, 3));
                awaiting = required && in_order;
            }
            b"TQ1" => needs_priority = false,
            b"SPM" | b"SAC" => {
                if needs_priority {
                    return Err(content("TQ1", 0));
                }
                if awaiting {
                    return Err(content("TCD", 0));
                }
                reference = None;
                in_order = false;
            }
            b"ORC" => {
                if awaiting {
                    return Err(content("TCD", 0));
                }
                reference = None;
            }
            b"TCD" => {
                tcd_occurrence = tcd_occurrence.checked_add(1).ok_or(content("TCD", 0))?;
                let reference = reference.ok_or(content("TCD", 0))?;
                if !same_test(raw(segment, 1), reference) {
                    return Err(Error::Inconsistent {
                        segment: "TCD",
                        field: 1,
                    }
                    .at_segment(segment, tcd_occurrence));
                }
                awaiting = false;
            }
            _ => {}
        }
    }
    if needs_priority {
        return Err(content("TQ1", 0));
    }
    if awaiting {
        return Err(content("TCD", 0));
    }
    Ok(())
}
