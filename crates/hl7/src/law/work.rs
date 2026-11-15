use super::content::{ContainerDetails, OrderDetails, PatientInfo, SpecimenDetails, WorkNotes};
use super::responses::Wire;
use super::{Error, Identity, Query, Scope, content};
use crate::{Message, Msa, Options, Response};
use std::collections::{BTreeMap, BTreeSet};

/// Negotiated outgoing options. Absence never authorizes clinical decisions.
#[derive(Debug, Clone, Copy, Default)]
pub struct Capabilities {
    /// Peer accepts patient/encounter demographics.
    pub patient: bool,
    /// Peer accepts extended container characteristics.
    pub container: bool,
    /// Peer accepts supplied specimen identity and characteristics.
    pub specimen: bool,
    /// Peer accepts explicit requested dilution.
    pub dilution: bool,
    /// Peer accepts app-authorized manager rerun/reflex status.
    pub manager_repeat: bool,
    /// Peer accepts app-authorized automatic repeat/reflex permissions.
    pub repeat_control: bool,
    /// Peer accepts explicit priority.
    pub priority: bool,
}

/// New work versus cancellation of a previously sent AWOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkAction {
    /// New AWOS; replicates require separate unique IDs.
    New,
    /// Cancel request; does not itself establish cancellation.
    Cancel,
}

/// App-supplied optional instructions; no defaults are synthesized.
#[derive(Debug, Clone, Copy, Default)]
pub struct Instructions<'a> {
    /// App-authorized dilution to use if the analyzer later initiates a rerun.
    pub rerun_dilution: Option<&'a [u8]>,
    /// Known endogenous content of a pre-dilution diluent.
    pub predilution_content: Option<&'a [u8]>,
    /// Requested specimen consumption, with its supplied units.
    pub consumption: Option<&'a [u8]>,
    /// Explicit coded auto-dilution type supplied by the app.
    pub dilution_type: Option<&'a [u8]>,
    /// Requested auto-dilution factor as an HL7 SN composite.
    pub dilution: Option<&'a [u8]>,
    /// App-authorized repeat status as a coded HL7 0389 value.
    pub repeat: Option<&'a [u8]>,
    /// Explicit app permission for automatic repeat (Y/N).
    pub automatic_repeat: Option<bool>,
    /// Explicit app permission for reflex testing (Y/N).
    pub reflex: Option<bool>,
    /// App-supplied coded priority.
    pub priority: Option<&'a [u8]>,
}

/// One app-authorized battery/test under one unique AWOS ID.
#[derive(Debug, Clone, Copy)]
pub struct Awos<'a> {
    notes: Option<&'a WorkNotes<'a>>,
    specimen_details: Option<&'a SpecimenDetails<'a>>,
    container_details: Option<&'a ContainerDetails<'a>>,
    order_details: Option<&'a OrderDetails<'a>>,
    id: &'a [u8],
    specimen: &'a [u8],
    specimen_type: &'a [u8],
    role: &'a [u8],
    container: &'a [u8],
    test: &'a [u8],
    action: WorkAction,
    instructions: Instructions<'a>,
    transaction_time: Option<&'a [u8]>,
}

impl<'a> Awos<'a> {
    /// Validates the required identifiers and coded specimen/test fields.
    /// An AWOS orders one test or battery; separate replicates need separate IDs.
    /// # Errors
    /// Rejects empty, null, oversized or separator-injected fields.
    pub fn new(
        id: &'a [u8],
        specimen: &'a [u8],
        specimen_type: &'a [u8],
        role: &'a [u8],
        container: &'a [u8],
        test: &'a [u8],
        action: WorkAction,
    ) -> Result<Self, Error> {
        for (value, segment, field, maximum) in [
            (id, "OBR", 2, 50),
            (specimen, "SPM", 2, 427),
            (container, "SAC", 3, 80),
            (test, "OBR", 4, 250),
        ] {
            safe(value, segment, field, maximum)?;
        }
        match role {
            b"P" => safe(specimen_type, "SPM", 4, 250)?,
            b"Q" => {
                if specimen_type != b"\"\"" {
                    return Err(content("SPM", 4));
                }
            }
            _ => return Err(content("SPM", 11)),
        }
        Ok(Self {
            notes: None,
            specimen_details: None,
            container_details: None,
            order_details: None,
            id,
            specimen,
            specimen_type,
            role,
            container,
            test,
            action,
            instructions: Instructions::default(),
            transaction_time: None,
        })
    }

    /// Attaches explicit app instructions, validated against peer capabilities at rendering.
    #[must_use]
    pub fn with_instructions(mut self, instructions: Instructions<'a>) -> Self {
        self.instructions = instructions;
        self
    }

    /// Supplies the actual initiating-event timestamp for ORC-9, without a timezone.
    /// If omitted, the caller asserts that the supplied response time is also the
    /// initiating-event time. Earlier order events must supply their own timestamp.
    #[must_use]
    pub fn with_transaction_time(mut self, timestamp: &'a [u8]) -> Self {
        self.transaction_time = Some(timestamp);
        self
    }

    /// Supplies comments with explicit group ownership; text is encoded FT data.
    #[must_use]
    pub fn with_notes(mut self, notes: &'a WorkNotes<'a>) -> Self {
        self.notes = Some(notes);
        self
    }

    /// Supplies available enhanced specimen fields; rendering checks the peer profile.
    #[must_use]
    pub fn with_specimen_details(mut self, details: &'a SpecimenDetails<'a>) -> Self {
        self.specimen_details = Some(details);
        self
    }

    /// Supplies available container characteristics without converting volume or dilution.
    #[must_use]
    pub fn with_container_details(mut self, details: &'a ContainerDetails<'a>) -> Self {
        self.container_details = Some(details);
        self
    }

    /// Supplies order grouping/provider context without changing AWOS identity.
    #[must_use]
    pub fn with_order_details(mut self, details: &'a OrderDetails<'a>) -> Self {
        self.order_details = Some(details);
        self
    }

    /// Returns the stable AWOS identity; it is never generated by rendering.
    #[must_use]
    pub fn id(self) -> &'a [u8] {
        self.id
    }
}

fn safe(value: &[u8], segment: &'static str, field: usize, maximum: usize) -> Result<(), Error> {
    if value.is_empty()
        || value == b"\"\""
        || value.len() > maximum
        || std::str::from_utf8(value).is_err()
        || value
            .iter()
            .any(|b| b.is_ascii_control() || matches!(b, b'|' | b'~'))
    {
        return Err(content(segment, field));
    }
    Ok(())
}

fn segment(wire: &mut Wire, id: &[u8], fields: &[&[u8]]) -> Result<(), Error> {
    wire.append(id)?;
    for field in fields {
        wire.field(field)?;
    }
    wire.append(b"\r")
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
    super::control_id(response.control_id.raw())?;
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
        (capabilities.patient, b"LAW_PAT_DEM".as_slice()),
        (capabilities.container, b"LAW_CONTAINER".as_slice()),
        (capabilities.specimen, b"LAW_SPECIMEN".as_slice()),
        (capabilities.dilution, b"LAW_DILUTIONS".as_slice()),
        (capabilities.manager_repeat, b"LAW_AM_RR"),
        (capabilities.repeat_control, b"LAW_AM_RR_CONTROL"),
        (capabilities.priority, b"LAW_AWOS_PRIORITY"),
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
        super::Profile::for_peer(capabilities),
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
    mut profile: super::Profile<'_>,
) -> Result<Message, Error> {
    // A validation context may narrow, but cannot enlarge, the peer's capabilities.
    profile.patient &= capabilities.patient;
    profile.specimen &= capabilities.specimen;
    profile.container &= capabilities.container;
    profile.dilution &= capabilities.dilution;
    profile.dilution_required &= capabilities.dilution;
    profile.manager_repeat &= capabilities.manager_repeat;
    profile.repeat_control &= capabilities.repeat_control;
    profile.priority &= capabilities.priority;
    profile.priority_required &= capabilities.priority;
    if work.is_empty() || work.len() > options.limits.segments {
        return Err(content("OBR", 0));
    }
    let mut ids = BTreeSet::new();
    for awos in work {
        if !ids.insert(awos.id) {
            return Err(content("OBR", 2));
        }
    }
    let mut wire = Wire::new(options.limits.message_bytes, b'|');
    header(&mut wire, address, response, capabilities)?;
    if let Some(patient) = patient {
        if !capabilities.patient {
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
        segment(&mut wire, b"PID", &fields)?;
        notes(&mut wire, patient.notes, &mut 0)?;
        if let Some(visit) = patient.visit {
            let mut fields: [&[u8]; 3] = [b""; 3];
            set_field(&mut fields, 2, Some(visit.class), "PV1")?;
            set_field(&mut fields, 3, visit.location, "PV1")?;
            segment(&mut wire, b"PV1", &fields)?;
        }
    }
    let mut groups: BTreeMap<(&[u8], &[u8]), Vec<&Awos<'_>>> = BTreeMap::new();
    for awos in work {
        groups
            .entry((awos.specimen, awos.container))
            .or_default()
            .push(awos);
    }
    for (ordinal, group) in groups.values().enumerate() {
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
            (2, capabilities.specimen.then_some(awos.specimen)),
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
        segment(&mut wire, b"SPM", &specimen)?;
        let mut note_count = 0usize;
        for awos in group {
            if let Some(content) = awos.notes {
                notes(&mut wire, content.specimen, &mut note_count)?;
            }
        }
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
                set_field(&mut container, position, value, "SAC")?;
            }
        }
        segment(&mut wire, b"SAC", &container)?;
        let mut note_count = 0usize;
        for awos in group {
            if let Some(content) = awos.notes {
                notes(&mut wire, content.container, &mut note_count)?;
            }
        }
        for awos in group {
            let control = match awos.action {
                WorkAction::New => super::OrderControl::New,
                WorkAction::Cancel => super::OrderControl::Cancel,
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
            segment(&mut wire, b"ORC", &order)?;
            if let Some(content) = awos.notes {
                notes(&mut wire, content.order, &mut 0)?;
            }
            let instruction = awos.instructions;
            if let Some(priority) = instruction.priority {
                if !capabilities.priority {
                    return Err(content("TQ1", 9));
                }
                safe(priority, "TQ1", 9, 250)?;
                segment(
                    &mut wire,
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
            segment(&mut wire, b"OBR", &request)?;
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
                    && !capabilities.dilution
                {
                    return Err(content("TCD", 2));
                }
                if instruction.repeat.is_some() && !capabilities.manager_repeat {
                    return Err(content("TCD", 8));
                }
                if (instruction.automatic_repeat.is_some() || instruction.reflex.is_some())
                    && !capabilities.repeat_control
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
                segment(&mut wire, b"TCD", &details)?;
            }
            if let Some(content) = awos.notes {
                notes(&mut wire, content.request, &mut 0)?;
            }
        }
    }
    let message = super::parse_message(&wire.bytes, options)?;
    profile.validate_message(&message, super::ControlDirection::Send)?;
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
        let value = super::raw(query.qpd, source);
        // C.11 manager-authored EI fields support EI.1 only. The RSP still
        // echoes the original QPD; ignored receive components stay in that original.
        *slot = if matches!(target, 2 | 3 | 9 | 12) {
            super::control::part(value, 1)
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
            super::OrderControl::Discontinue.as_bytes(),
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
    let message = super::parse_message(&wire.bytes, options)?;
    super::Profile::for_peer(Capabilities::default())
        .validate_message(&message, super::ControlDirection::Send)?;
    Ok(message)
}

/// Analyzer's per-request AWOS decision; never a message acknowledgment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwosOutcome {
    /// New work accepted.
    Accepted,
    /// This request rejected, including a duplicate of previously accepted work.
    UnableToAccept,
    /// Cancellation confirmed.
    CancelAccepted,
    /// Cancellation refused; work may still complete.
    UnableToCancel,
}

/// A scoped decision from an ORL; the prior work lifecycle is not mutated.
#[derive(Debug, Clone, Copy)]
pub struct AwosDecision<'a> {
    /// AWOS identity, including its namespace.
    pub id: Identity<'a>,
    /// Request-specific outcome.
    pub outcome: AwosOutcome,
    /// Typed analyzer work status; original bytes remain in the message.
    pub status: super::OrderStatus,
}

/// Validated ORL response with message acceptance separate from AWOS decisions.
#[derive(Debug)]
pub struct WorkResponse<'a> {
    message: &'a Message,
    acceptance: crate::AckCode,
    decisions: Vec<AwosDecision<'a>>,
}

impl<'a> WorkResponse<'a> {
    /// Validates correlation and one response for every requested AWOS.
    /// # Errors
    /// Rejects inconsistent/missing/duplicate/unknown AWOS or mismatched control/status pairs.
    pub fn new(message: &'a Message, sent: &'a Message) -> Result<Self, Error> {
        Self::with_profile(message, sent, super::Profile::receive())
    }

    /// Validates a work response using negotiated options and trusted authority bindings.
    /// # Errors
    /// Returns located profile failures or an inconsistent work-response error.
    pub fn with_profile(
        message: &'a Message,
        sent: &'a Message,
        mut profile: super::Profile<'_>,
    ) -> Result<Self, Error> {
        profile.results_message = false;
        profile.validate_message(message, super::ControlDirection::Receive)?;
        super::profile::header(message, b"LAB-28", false)?;
        let msh = super::msh(message)?;
        let d = message.delimiters();
        if !super::raw(msh.segment(), 9)
            .split(|b| *b == d.component)
            .take(3)
            .eq([b"ORL".as_slice(), b"O34", b"ORL_O42"])
        {
            return Err(content("MSH", 9));
        }
        let mut segments = message.segments().skip(1);
        let msa_segment = segments
            .next()
            .filter(|s| s.id() == b"MSA")
            .ok_or(content("MSA", 0))?;
        let msa = Msa::from_law(msa_segment)?;
        if msa.control_id() != super::msh(sent)?.control_id() {
            return Err(content("MSA", 2));
        }
        let expected: Vec<_> = sent
            .segments()
            .filter(|s| s.id() == b"OBR")
            .map(|s| super::raw(s, 2))
            .collect();
        let mut requests = BTreeMap::new();
        let mut action = None;
        let mut source_specimen = b"".as_slice();
        let mut source_container = b"".as_slice();
        for segment in sent.segments() {
            match segment.id() {
                b"SPM" => {
                    source_specimen = super::raw(segment, 2);
                    source_container = b"";
                    action = None;
                }
                b"SAC" => {
                    source_container = super::raw(segment, 3);
                }
                b"ORC" => {
                    action = match super::OrderControl::parse(super::raw(segment, 1))? {
                        super::OrderControl::New => Some(WorkAction::New),
                        super::OrderControl::Cancel => Some(WorkAction::Cancel),
                        super::OrderControl::Discontinue => None,
                        super::OrderControl::Accepted
                        | super::OrderControl::UnableToAccept
                        | super::OrderControl::Cancelled
                        | super::OrderControl::UnableToCancel
                        | super::OrderControl::StatusChange => return Err(content("ORC", 1)),
                    };
                }
                b"OBR" => {
                    let action = action.ok_or(content("ORC", 1))?;
                    if requests
                        .insert(
                            super::raw(segment, 2),
                            (action, source_specimen, source_container),
                        )
                        .is_some()
                    {
                        return Err(content("OBR", 2));
                    }
                }
                _ => {}
            }
        }
        let mut decisions = Vec::new();
        let mut has_specimen = false;
        let mut response_specimen = b"".as_slice();
        let mut response_container = b"".as_slice();
        let mut error_count = 0usize;
        let mut seen = BTreeSet::new();
        profile.validate_segment(msa_segment, super::ControlDirection::Receive)?;
        for s in segments {
            profile.validate_segment(s, super::ControlDirection::Receive)?;
            match s.id() {
                b"ERR" => {
                    if has_specimen {
                        return Err(content("ERR", 0));
                    }
                    error_count = error_count.checked_add(1).ok_or(content("ERR", 0))?;
                    continue;
                }
                b"PID" => {
                    if has_specimen || !seen.is_empty() {
                        return Err(content("PID", 0));
                    }
                    continue;
                }
                b"SPM" => {
                    has_specimen = true;
                    response_specimen = super::raw(s, 2);
                    response_container = b"";
                    continue;
                }
                b"SAC" => {
                    if !has_specimen {
                        return Err(content("SAC", 0));
                    }
                    response_container = super::raw(s, 3);
                    continue;
                }
                b"ORC" => {
                    if !has_specimen || error_count != 0 {
                        return Err(content("ORC", 0));
                    }
                }
                _ => return Err(content("ORL", 0)),
            }

            let id = super::raw(s, 2);
            if !expected.contains(&id) || !seen.insert(id) {
                return Err(content("ORC", 2));
            }
            let (_, expected_specimen, expected_container) =
                requests.get(id).ok_or(content("ORC", 2))?;
            if !response_specimen.is_empty() && response_specimen != *expected_specimen {
                return Err(Error::Inconsistent {
                    segment: "SPM",
                    field: 2,
                });
            }
            if !response_container.is_empty() && response_container != *expected_container {
                return Err(Error::Inconsistent {
                    segment: "SAC",
                    field: 3,
                });
            }
            let status = super::OrderStatus::parse(super::raw(s, 5))?;
            let outcome = response_outcome(super::OrderControl::parse(super::raw(s, 1))?, status)?;
            match (requests.get(id).map(|request| &request.0), outcome) {
                (Some(WorkAction::New), AwosOutcome::Accepted | AwosOutcome::UnableToAccept)
                | (
                    Some(WorkAction::Cancel),
                    AwosOutcome::CancelAccepted | AwosOutcome::UnableToCancel,
                ) => {}
                (
                    Some(WorkAction::New),
                    AwosOutcome::CancelAccepted | AwosOutcome::UnableToCancel,
                )
                | (Some(WorkAction::Cancel), AwosOutcome::Accepted | AwosOutcome::UnableToAccept)
                | (
                    None,
                    AwosOutcome::Accepted
                    | AwosOutcome::UnableToAccept
                    | AwosOutcome::CancelAccepted
                    | AwosOutcome::UnableToCancel,
                ) => return Err(content("ORC", 1)),
            }
            decisions.push(AwosDecision {
                id: Identity(id),
                outcome,
                status,
            });
        }
        if (msa.code() == crate::AckCode::Aa) == (error_count != 0) {
            return Err(content("ERR", 0));
        }
        if msa.code() == crate::AckCode::Aa && seen.len() != expected.len() {
            return Err(content("ORC", 0));
        }
        if msa.code() != crate::AckCode::Aa && !decisions.is_empty() {
            return Err(content("ORC", 0));
        }
        Ok(Self {
            message,
            acceptance: msa.code(),
            decisions,
        })
    }

    /// Returns message-level acceptance, independently of AWOS outcomes.
    #[must_use]
    pub fn acceptance(&self) -> crate::AckCode {
        self.acceptance
    }

    /// Returns request-specific decisions; an unable-to-accept never retracts prior acceptance.
    #[must_use]
    pub fn decisions(&self) -> &[AwosDecision<'a>] {
        &self.decisions
    }

    /// Returns the untouched input for durable retention and reconciliation.
    #[must_use]
    pub fn message(&self) -> &'a Message {
        self.message
    }
}

fn response_outcome(
    control: super::OrderControl,
    status: super::OrderStatus,
) -> Result<AwosOutcome, Error> {
    use super::{OrderControl, OrderStatus};
    match control {
        OrderControl::Accepted => match status {
            OrderStatus::Scheduled | OrderStatus::InProgress | OrderStatus::Complete => {
                Ok(AwosOutcome::Accepted)
            }
            OrderStatus::Cancelled => Err(content("ORC", 5)),
        },
        OrderControl::UnableToAccept => match status {
            OrderStatus::Cancelled => Ok(AwosOutcome::UnableToAccept),
            OrderStatus::Scheduled | OrderStatus::InProgress | OrderStatus::Complete => {
                Err(content("ORC", 5))
            }
        },
        OrderControl::Cancelled => match status {
            OrderStatus::Cancelled => Ok(AwosOutcome::CancelAccepted),
            OrderStatus::Scheduled | OrderStatus::InProgress | OrderStatus::Complete => {
                Err(content("ORC", 5))
            }
        },
        OrderControl::UnableToCancel => match status {
            OrderStatus::InProgress | OrderStatus::Complete => Ok(AwosOutcome::UnableToCancel),
            OrderStatus::Scheduled | OrderStatus::Cancelled => Err(content("ORC", 5)),
        },
        OrderControl::New
        | OrderControl::Cancel
        | OrderControl::Discontinue
        | OrderControl::StatusChange => Err(content("ORC", 1)),
    }
}
