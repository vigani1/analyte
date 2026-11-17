//! Encodes LAW work orders and specimen groups without lifecycle decisions.
use super::{Awos, Capabilities, Error, Instructions, PatientInfo, WorkAction, content, safe};
use crate::law;
use crate::{Message, Options, Response};
use law::responses::Wire;
use law::{Query, Scope};
use std::collections::{BTreeMap, BTreeSet};
fn segment(wire: &mut Wire, id: &[u8], fields: &[&[u8]]) -> Result<(), Error> {
    wire.segment(id, fields).map_err(Into::into)
}

fn set_field<'a>(
    fields: &mut [&'a [u8]],
    position: usize,
    value: Option<&'a [u8]>,
    id: &'static str,
) -> Result<(), Error> {
    if let Some(value) = value {
        if value.len() > 65536
            || std::str::from_utf8(value).is_err()
            || value
                .iter()
                .any(|byte| byte.is_ascii_control() || *byte == b'|')
        {
            return Err(content(id, position));
        }
        let index = position.checked_sub(1).ok_or(content(id, position))?;
        *fields.get_mut(index).ok_or(content(id, position))? = value;
    }
    Ok(())
}

fn notes(wire: &mut Wire, values: &[&[u8]], count: &mut usize) -> Result<(), Error> {
    for value in values {
        *count = count.checked_add(1).ok_or(content("NTE", 1))?;
        let sequence = count.to_string();
        let mut fields: [&[u8]; 3] = [sequence.as_bytes(), b"A", b""];
        set_field(&mut fields, 3, Some(value), "NTE")?;
        segment(wire, b"NTE", &fields)?;
    }
    Ok(())
}

/// Host-supplied addressing for an initiating exchange; no configured-name checks on input.
#[derive(Debug, Clone, Copy)]
pub struct Address<'a> {
    /// Sending application.
    pub application: &'a [u8],
    /// Sending facility.
    pub facility: &'a [u8],
    /// Receiving application.
    pub peer_application: &'a [u8],
    /// Receiving facility.
    pub peer_facility: &'a [u8],
}

fn header(
    wire: &mut Wire,
    address: Address<'_>,
    response: Response<'_>,
    capabilities: Capabilities,
) -> Result<(), Error> {
    for (value, field) in [
        (address.application, 3),
        (address.facility, 4),
        (address.peer_application, 5),
        (address.peer_facility, 6),
    ] {
        if !value.is_empty() {
            safe(value, "MSH", field, 227)?;
        }
    }
    law::control_id(response.control_id.raw())?;
    safe(response.timestamp, "MSH", 7, 26)?;
    if !response
        .timestamp
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-'))
    {
        return Err(content("MSH", 7));
    }
    let mut profiles = b"LAB-28^IHE".to_vec();
    for (enabled, name) in [
        (capabilities.content.patient, b"LAW_PAT_DEM".as_slice()),
        (capabilities.content.container, b"LAW_CONTAINER".as_slice()),
        (capabilities.content.specimen, b"LAW_SPECIMEN".as_slice()),
        (
            capabilities.instructions.dilution,
            b"LAW_DILUTIONS".as_slice(),
        ),
        (
            capabilities.instructions.repeat.manager_repeat,
            b"LAW_AM_RR",
        ),
        (
            capabilities.instructions.repeat.repeat_control,
            b"LAW_AM_RR_CONTROL",
        ),
        (capabilities.instructions.priority, b"LAW_AWOS_PRIORITY"),
    ] {
        if enabled {
            profiles.extend_from_slice(b"~");
            profiles.extend_from_slice(name);
            profiles.extend_from_slice(b"^IHE");
        }
    }
    segment(
        wire,
        b"MSH",
        &[
            b"^~\\&",
            address.application,
            address.facility,
            address.peer_application,
            address.peer_facility,
            response.timestamp,
            b"",
            b"OML^O33^OML_O33",
            response.control_id.raw(),
            b"P",
            b"2.5.1",
            b"",
            b"",
            b"NE",
            b"AL",
            b"",
            b"UNICODE UTF-8",
            b"",
            b"",
            &profiles,
        ],
    )
}

/// Builds one or more app-authorized AWOS, rejecting duplicate identities before rendering.
/// Grouping is specimen-local; each AWOS has its own specimen/container group.
/// # Errors
/// Rejects empty work, duplicate IDs, unsupported options or invalid/oversized output.
pub fn work_order(
    work: &[Awos<'_>],
    patient: Option<PatientInfo<'_>>,
    address: Address<'_>,
    response: Response<'_>,
    capabilities: Capabilities,
    options: Options,
) -> Result<Message, Error> {
    work_order_with_profile(
        work,
        patient,
        address,
        response,
        capabilities,
        options,
        law::Profile::for_peer(capabilities),
    )
}

/// Builds work with explicit content validation and host-supplied authority bindings.
/// # Errors
/// Rejects invalid fields, unavailable options or unresolved assigning authorities.
pub fn work_order_with_profile(
    work: &[Awos<'_>],
    patient: Option<PatientInfo<'_>>,
    address: Address<'_>,
    response: Response<'_>,
    capabilities: Capabilities,
    options: Options,
    mut profile: law::Profile<'_>,
) -> Result<Message, Error> {
    // A validation context may narrow, but cannot enlarge, the peer's capabilities.
    profile.content.patient &= capabilities.content.patient;
    profile.content.specimen &= capabilities.content.specimen;
    profile.content.container &= capabilities.content.container;
    profile.dilution = profile
        .dilution
        .restrict(capabilities.instructions.dilution);
    profile.repeat.manager_repeat &= capabilities.instructions.repeat.manager_repeat;
    profile.repeat.repeat_control &= capabilities.instructions.repeat.repeat_control;
    profile.priority = profile
        .priority
        .restrict(capabilities.instructions.priority);
    if work.is_empty() || work.len() > options.limits.segments {
        return Err(content("OBR", 0));
    }
    let mut ids = BTreeSet::new();
    for awos in work {
        if awos.container.is_empty()
            && awos
                .container_details
                .and_then(|details| details.parent)
                .is_none_or(<[u8]>::is_empty)
        {
            return Err(content("SAC", 4));
        }
        if !ids.insert(awos.id) {
            return Err(content("OBR", 2));
        }
    }
    let mut wire = Wire::new(options.limits.message_bytes, b'|');
    header(&mut wire, address, response, capabilities)?;
    encode_patient(&mut wire, patient, capabilities)?;
    let mut groups: BTreeMap<(&[u8], &[u8]), Vec<&Awos<'_>>> = BTreeMap::new();
    for awos in work {
        groups
            .entry((awos.specimen, awos.container))
            .or_default()
            .push(awos);
    }
    for (ordinal, group) in groups.values().enumerate() {
        encode_specimen_group(&mut wire, group, ordinal, capabilities)?;
        for awos in group {
            encode_awos(&mut wire, awos, response, capabilities)?;
        }
    }
    let message = law::parse_message(&wire.bytes, options)?;
    profile.validate_message(&message, law::ControlDirection::Send)?;
    Ok(message)
}

/// Builds the restricted negative LAB-28 only from a validated query scope.
/// The host must call this solely after a fresh, complete no-work lookup.
/// # Errors
/// Rejects invalid metadata, incompatible encoding or oversized output.
pub fn negative_work(
    query: Query<'_>,
    address: Address<'_>,
    response: Response<'_>,
    options: Options,
) -> Result<Message, Error> {
    let mut wire = Wire::new(options.limits.message_bytes, b'|');
    header(&mut wire, address, response, Capabilities::default())?;
    segment(
        &mut wire,
        b"SPM",
        &[
            b"1",
            b"",
            b"",
            b"\"\"",
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
            b"U^^IHELAW",
        ],
    )?;
    // These are borrowed encoded fields; only identical delimiters can be echoed.
    if query.message().delimiters().bytes() != *b"|^~\\&" {
        return Err(content("MSH", 2));
    }
    let mut sac: [&[u8]; 15] = [b""; 15];
    for (target, source) in [(2, 3), (3, 9), (9, 4), (10, 5), (12, 6), (13, 7), (14, 8)] {
        let slot = sac.get_mut(target).ok_or(content("SAC", 0))?;
        let value = law::raw(query.qpd, source);
        // C.11 manager-authored EI fields support EI.1 only. The RSP still
        // echoes the original QPD; ignored receive components stay in that original.
        *slot = if matches!(target, 2 | 3 | 9 | 12) {
            law::fields::part(value, 1)
        } else {
            value
        };
    }
    if matches!(query.scope(), Scope::AllPending) {
        *sac.get_mut(2).ok_or(content("SAC", 3))? = b"\"\"";
    }
    segment(&mut wire, b"SAC", &sac)?;
    segment(
        &mut wire,
        b"ORC",
        &[
            law::OrderControl::Discontinue.as_bytes(),
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
            response.timestamp.get(..14).ok_or(content("ORC", 9))?,
        ],
    )?;
    let message = law::parse_message(&wire.bytes, options)?;
    law::Profile::for_peer(Capabilities::default())
        .validate_message(&message, law::ControlDirection::Send)?;
    Ok(message)
}

fn encode_patient(
    wire: &mut Wire,
    patient: Option<PatientInfo<'_>>,
    capabilities: Capabilities,
) -> Result<(), Error> {
    if let Some(patient) = patient {
        if !capabilities.content.patient {
            return Err(content("PID", 0));
        }
        let mut fields: [&[u8]; 35] = [b""; 35];
        for (position, value) in [
            (3, Some(patient.identifiers)),
            (5, Some(patient.name)),
            (7, patient.birth),
            (8, patient.sex),
            (10, patient.race),
            (35, patient.taxonomy),
        ] {
            set_field(&mut fields, position, value, "PID")?;
        }
        segment(wire, b"PID", &fields)?;
        notes(wire, patient.notes, &mut 0)?;
        if let Some(visit) = patient.visit {
            let mut fields: [&[u8]; 3] = [b""; 3];
            set_field(&mut fields, 2, Some(visit.class), "PV1")?;
            set_field(&mut fields, 3, visit.location, "PV1")?;
            segment(wire, b"PV1", &fields)?;
        }
    }
    Ok(())
}

fn encode_awos(
    wire: &mut Wire,
    awos: &Awos<'_>,
    response: Response<'_>,
    capabilities: Capabilities,
) -> Result<(), Error> {
    let control = match awos.action {
        WorkAction::New => law::OrderControl::New,
        WorkAction::Cancel => law::OrderControl::Cancel,
    };
    let details = awos.order_details.copied().unwrap_or_default();
    let mut order: [&[u8]; 21] = [b""; 21];
    for (position, value) in [
        (1, Some(control.as_bytes())),
        (4, details.group),
        (9, awos.transaction_time.or(response.timestamp.get(..14))),
        (21, details.facility),
    ] {
        set_field(&mut order, position, value, "ORC")?;
    }
    segment(wire, b"ORC", &order)?;
    if let Some(content) = awos.notes {
        notes(wire, content.order, &mut 0)?;
    }
    let instruction = awos.instructions;
    if let Some(priority) = instruction.priority {
        if !capabilities.instructions.priority {
            return Err(content("TQ1", 9));
        }
        safe(priority, "TQ1", 9, 250)?;
        segment(
            wire,
            b"TQ1",
            &[b"", b"", b"", b"", b"", b"", b"", b"", priority],
        )?;
    }
    let mut request: [&[u8]; 49] = [b""; 49];
    for (position, value) in [
        (2, Some(awos.id)),
        (4, Some(awos.test)),
        (16, details.provider),
        (49, details.result_handling),
    ] {
        set_field(&mut request, position, value, "OBR")?;
    }
    segment(wire, b"OBR", &request)?;
    encode_instructions(wire, awos, instruction, capabilities)?;
    if let Some(content) = awos.notes {
        notes(wire, content.request, &mut 0)?;
    }
    Ok(())
}

fn encode_instructions(
    wire: &mut Wire,
    awos: &Awos<'_>,
    instruction: Instructions<'_>,
    capabilities: Capabilities,
) -> Result<(), Error> {
    if instruction.dilution.is_some()
        || instruction.rerun_dilution.is_some()
        || instruction.predilution_content.is_some()
        || instruction.consumption.is_some()
        || instruction.dilution_type.is_some()
        || instruction.repeat.is_some()
        || instruction.automatic_repeat.is_some()
        || instruction.reflex.is_some()
    {
        if (instruction.dilution.is_some()
            || instruction.rerun_dilution.is_some()
            || instruction.predilution_content.is_some()
            || instruction.dilution_type.is_some())
            && !capabilities.instructions.dilution
        {
            return Err(content("TCD", 2));
        }
        if instruction.repeat.is_some() && !capabilities.instructions.repeat.manager_repeat {
            return Err(content("TCD", 8));
        }
        if (instruction.automatic_repeat.is_some() || instruction.reflex.is_some())
            && !capabilities.instructions.repeat.repeat_control
        {
            return Err(content("TCD", 6));
        }
        for (value, field) in [(instruction.dilution, 2), (instruction.repeat, 8)] {
            if let Some(value) = value {
                safe(value, "TCD", field, 250)?;
            }
        }
        let permission = |value| match value {
            Some(true) => b"Y".as_slice(),
            Some(false) => b"N",
            None => b"",
        };
        let mut details: [&[u8]; 11] = [b""; 11];
        for (position, value) in [
            (1, Some(awos.test)),
            (2, instruction.dilution),
            (3, instruction.rerun_dilution),
            (5, instruction.predilution_content),
            (6, Some(permission(instruction.automatic_repeat))),
            (7, Some(permission(instruction.reflex))),
            (8, instruction.repeat),
            (9, instruction.consumption),
            (11, instruction.dilution_type),
        ] {
            set_field(&mut details, position, value, "TCD")?;
        }
        segment(wire, b"TCD", &details)?;
    }
    Ok(())
}

fn encode_specimen_group(
    wire: &mut Wire,
    group: &[&Awos<'_>],
    ordinal: usize,
    capabilities: Capabilities,
) -> Result<(), Error> {
    let awos = group.first().ok_or(content("SPM", 0))?;
    if group.iter().any(|other| {
        other.specimen_type != awos.specimen_type
            || other.role != awos.role
            || other.specimen_details != awos.specimen_details
            || other.container_details != awos.container_details
    }) {
        return Err(content("SPM", 4));
    }
    let sequence = ordinal.checked_add(1).ok_or(content("SPM", 1))?.to_string();
    let mut specimen: [&[u8]; 27] = [b""; 27];
    for (position, value) in [
        (1, Some(sequence.as_bytes())),
        (2, capabilities.content.specimen.then_some(awos.specimen)),
        (4, Some(awos.specimen_type)),
        (
            11,
            Some(match awos.role {
                b"P" => b"P^^HL70369".as_slice(),
                b"Q" => b"Q^^HL70369",
                _ => return Err(content("SPM", 11)),
            }),
        ),
    ] {
        set_field(&mut specimen, position, value, "SPM")?;
    }
    if let Some(details) = awos.specimen_details {
        for (position, value) in [
            (3, details.parents),
            (7, details.collection_method),
            (8, details.source_site),
            (9, details.source_modifier),
            (16, details.risk),
            (17, details.collection_time),
            (18, details.receipt_time),
            (27, details.container_type),
        ] {
            set_field(&mut specimen, position, value, "SPM")?;
        }
    }
    segment(wire, b"SPM", &specimen)?;
    let mut note_count = 0usize;
    for awos in group {
        if let Some(content) = awos.notes {
            notes(wire, content.specimen, &mut note_count)?;
        }
    }
    encode_container(wire, awos, group)?;
    Ok(())
}

fn encode_container(wire: &mut Wire, awos: &Awos<'_>, group: &[&Awos<'_>]) -> Result<(), Error> {
    let mut container: [&[u8]; 29] = [b""; 29];
    set_field(&mut container, 3, Some(awos.container), "SAC")?;
    if let Some(details) = awos.container_details {
        for (position, value) in [
            (4, details.parent),
            (9, details.carrier_type),
            (10, details.carrier),
            (11, details.carrier_position),
            (13, details.tray),
            (14, details.tray_position),
            (15, details.location),
            (21, details.container_volume),
            (22, details.specimen_volume),
            (24, details.volume_units),
            (29, details.dilution),
        ] {
            // C.11 excludes carrier/tray identification when SAC-3 is supplied.
            if awos.container.is_empty() || !matches!(position, 10 | 11 | 13 | 14 | 15) {
                set_field(&mut container, position, value, "SAC")?;
            }
        }
    }
    segment(wire, b"SAC", &container)?;
    let mut note_count = 0usize;
    for awos in group {
        if let Some(content) = awos.notes {
            notes(wire, content.container, &mut note_count)?;
        }
    }
    Ok(())
}
