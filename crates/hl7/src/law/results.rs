use super::responses::Wire;
use super::{Error, Identity, content, raw};
use crate::{AckCode, AckError, Field, Message, Options, Response, Responsibility, Segment};

/// Source observation state; revisions remain a host responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultStatus {
    /// Final observation.
    Final,
    /// Preliminary observation.
    Preliminary,
    /// Corrected observation.
    Corrected,
    /// Result removed.
    Deleted,
    /// Previously transmitted result was wrong.
    Wrong,
    /// Status change to final without retransmitting the value.
    Finalize,
    /// Partial result.
    Partial,
    /// Not verified.
    Unverified,
    /// Could not obtain a result.
    Unobtainable,
    /// Results pending.
    Pending,
    /// No result available yet.
    NoResult,
    /// Order detail rather than a result.
    OrderDetail,
}

fn status(value: &[u8]) -> Result<ResultStatus, Error> {
    match value {
        b"F" => Ok(ResultStatus::Final),
        b"P" => Ok(ResultStatus::Preliminary),
        b"C" => Ok(ResultStatus::Corrected),
        b"D" => Ok(ResultStatus::Deleted),
        b"W" => Ok(ResultStatus::Wrong),
        b"U" => Ok(ResultStatus::Finalize),
        b"S" => Ok(ResultStatus::Partial),
        b"R" => Ok(ResultStatus::Unverified),
        b"X" => Ok(ResultStatus::Unobtainable),
        b"I" => Ok(ResultStatus::Pending),
        b"N" => Ok(ResultStatus::NoResult),
        b"O" => Ok(ResultStatus::OrderDetail),
        _ => Err(content("OBX", 11)),
    }
}

/// Typed value with exact encoded spelling, including repeated values.
/// No floating point conversion or external-pointer dereference is performed.
#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    /// Explicit null value, distinct from an absent value.
    Null,
    /// Reference to a source observation identity.
    Identity(&'a [u8]),
    /// Plain decimal NM.
    Number(&'a [u8]),
    /// Structured numeric SN, including comparator/ratio.
    StructuredNumber(&'a [u8]),
    /// ST, TX or FT text; escapes remain available for lossless decoding.
    Text(&'a [u8]),
    /// Coded CE or CWE value.
    Coded(&'a [u8]),
    /// Embedded data, retained without interpretation.
    Encapsulated(&'a [u8]),
    /// External data reference, not proof that referenced data is retained.
    Reference(&'a [u8]),
    /// Numeric array with original axis/repetition boundaries.
    Array(&'a [u8]),
    /// Original date/time with its precision and offset.
    Time(&'a [u8]),
}

fn decimal(value: &[u8]) -> bool {
    let unsigned = value
        .strip_prefix(b"+")
        .or_else(|| value.strip_prefix(b"-"))
        .unwrap_or(value);
    let mut point = false;
    let mut digit = false;
    for byte in unsigned {
        if byte.is_ascii_digit() {
            digit = true;
        } else if *byte == b'.' && !point {
            point = true;
        } else {
            return false;
        }
    }
    digit
}

fn values(segment: Segment<'_>) -> Result<Vec<Value<'_>>, Error> {
    let field = segment.field(5).ok_or(content("OBX", 5))?;
    if field.is_null() {
        return Ok(vec![Value::Null]);
    }
    let kind = raw(segment, 2);
    let mut values = Vec::new();
    for repetition in field.repetitions() {
        let value = repetition.raw();
        if value.is_empty() {
            return Err(content("OBX", 5));
        }
        values.push(match kind {
            b"NM" => {
                if value.len() > 16 || !decimal(value) {
                    return Err(content("OBX", 5));
                }
                Value::Number(value)
            }
            b"SN" => {
                let parts: Vec<_> = value
                    .split(|b| *b == segment.delimiters.component)
                    .collect();
                if parts.len() > 4
                    || !matches!(
                        parts.first().copied(),
                        Some(b"" | b"<" | b">" | b"<=" | b">=" | b"=" | b"<>")
                    )
                    || parts.get(1).is_none_or(|v| !decimal(v))
                {
                    return Err(content("OBX", 5));
                }
                let separator = parts.get(2).copied().unwrap_or_default();
                if !matches!(separator, b"" | b":" | b"/" | b"-" | b"+")
                    || (!separator.is_empty() && parts.get(3).is_none_or(|v| !decimal(v)))
                {
                    return Err(content("OBX", 5));
                }
                Value::StructuredNumber(value)
            }
            b"ST" | b"TX" | b"FT" => {
                let maximum = if kind == b"ST" { 200 } else { 65536 };
                if value.len() > maximum {
                    return Err(content("OBX", 5));
                }
                Value::Text(value)
            }
            b"CE" | b"CWE" => Value::Coded(value),
            b"EI" => Value::Identity(value),
            b"ED" => {
                let parts: Vec<_> = value
                    .split(|b| *b == segment.delimiters.component)
                    .collect();
                if parts.len() != 5
                    || parts.get(1).is_none_or(|p| p.is_empty() || p.len() > 9)
                    || parts
                        .get(3)
                        .is_none_or(|p| !matches!(*p, b"A" | b"Hex" | b"Base64"))
                    || parts.get(4).is_none_or(|p| p.is_empty())
                {
                    return Err(content("OBX", 5));
                }
                Value::Encapsulated(value)
            }
            b"RP" => {
                let parts: Vec<_> = value
                    .split(|b| *b == segment.delimiters.component)
                    .collect();
                if parts.len() > 4 || parts.first().is_none_or(|p| p.is_empty() || p.len() > 999) {
                    return Err(content("OBX", 5));
                }
                Value::Reference(value)
            }
            b"NA" => {
                for (index, axis) in value
                    .split(|b| *b == segment.delimiters.component)
                    .enumerate()
                {
                    if (index == 0 || !axis.is_empty()) && (axis.len() > 16 || !decimal(axis)) {
                        return Err(content("OBX", 5));
                    }
                }
                Value::Array(value)
            }
            b"DT" | b"TM" | b"TS" => Value::Time(value),
            _ => return Err(content("OBX", 2)),
        });
    }
    Ok(values)
}

/// Parent of a retained supplemental segment, identified by source ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    /// Observation about the specimen itself, before the container/order groups.
    SpecimenObservation(usize),
    /// Patient segment ordinal.
    Patient(usize),
    /// Specimen segment ordinal.
    Specimen(usize),
    /// Container segment ordinal.
    Container(usize),
    /// Order segment ordinal.
    Order(usize),
    /// Result segment ordinal.
    Observation(usize),
}

/// A typed specimen-level observation, distinct from an AWOS result.
#[derive(Debug)]
pub struct SpecimenObservation<'a> {
    specimen: Segment<'a>,
    segment: Segment<'a>,
    values: Vec<Value<'a>>,
}

impl<'a> SpecimenObservation<'a> {
    /// Returns the source specimen, including its original identities.
    #[must_use]
    pub fn specimen(&self) -> Segment<'a> {
        self.specimen
    }

    /// Returns the original OBX with identity, status, units and equipment.
    #[must_use]
    pub fn segment(&self) -> Segment<'a> {
        self.segment
    }

    /// Returns validated value repetitions without converting their spelling.
    #[must_use]
    pub fn values(&self) -> &[Value<'a>] {
        &self.values
    }
}

#[derive(Clone, Copy)]
enum ResultPhase {
    NoResult,
    Observation,
    Detail,
    Inventory,
    Notes,
}

/// Note, inventory or detail segment with explicit source parent.
#[derive(Debug, Clone, Copy)]
pub struct Attached<'a> {
    /// Parent ownership; a note is never executable clinical logic.
    pub parent: Parent,
    /// Original segment and all fields, including unknown content.
    pub segment: Segment<'a>,
}

/// Observation with AWOS/specimen/container context, without invented identities.
#[derive(Debug)]
pub struct Observation<'a> {
    order_ordinal: usize,
    segment: Segment<'a>,
    order: Segment<'a>,
    specimen: Segment<'a>,
    container: Segment<'a>,
    status: ResultStatus,
    values: Vec<Value<'a>>,
}

impl<'a> Observation<'a> {
    /// Returns the source AWOS, or none for an analyzer-created unsolicited order.
    #[must_use]
    pub fn awos(&self) -> Option<Identity<'a>> {
        let value = raw(self.order, 2);
        (value != b"\"\"").then_some(Identity(value))
    }

    /// Returns the full specimen segment, retaining parent and collection data.
    #[must_use]
    pub fn specimen(&self) -> Segment<'a> {
        self.specimen
    }

    /// Returns full container data, including carrier, volume and dilution fields.
    #[must_use]
    pub fn container(&self) -> Segment<'a> {
        self.container
    }

    /// Returns full order data, including analyzer-assigned order identity.
    #[must_use]
    pub fn order(&self) -> Segment<'a> {
        self.order
    }

    /// Returns the exact coded observation identity (OBX-3).
    #[must_use]
    pub fn code(&self) -> &'a [u8] {
        raw(self.segment, 3)
    }

    /// Returns the original run/sub-ID composite (OBX-4), never a synthetic key.
    #[must_use]
    pub fn sub_id(&self) -> &'a [u8] {
        raw(self.segment, 4)
    }

    /// Returns the source status; it is not an Analyte revision counter.
    #[must_use]
    pub fn status(&self) -> ResultStatus {
        self.status
    }

    /// Returns each value repetition with original spelling.
    #[must_use]
    pub fn values(&self) -> &[Value<'a>] {
        &self.values
    }

    /// Returns source units without conversion or inferred UCUM coding.
    #[must_use]
    pub fn units(&self) -> Option<Field<'a>> {
        self.segment.field(6)
    }

    /// Returns the source reference range.
    #[must_use]
    pub fn range(&self) -> Option<Field<'a>> {
        self.segment.field(7)
    }

    /// Returns all source interpretation flags.
    #[must_use]
    pub fn flags(&self) -> Option<Field<'a>> {
        self.segment.field(8)
    }

    /// Returns the equipment hierarchy without flattening its repetitions.
    #[must_use]
    pub fn equipment(&self) -> Option<Field<'a>> {
        self.segment.field(18)
    }

    /// Returns every source field, including probability, instance ID and timing.
    #[must_use]
    pub fn segment(&self) -> Segment<'a> {
        self.segment
    }
}

/// Atomically validated LAB-29 interpretation borrowing the complete original message.
#[derive(Debug)]
pub struct Results<'a> {
    specimen_observations: Vec<SpecimenObservation<'a>>,
    message: &'a Message,
    observations: Vec<Observation<'a>>,
    attached: Vec<Attached<'a>>,
}

impl<'a> Results<'a> {
    /// Parses LAB-29 without requiring prior downloaded AWOS or emitting partial results.
    /// # Errors
    /// Rejects missing context, unsupported types/statuses and inconsistent TCD identity.
    /// The input remains owned by the host even when no view can be returned.
    pub fn new(message: &'a Message) -> Result<Self, Error> {
        super::profile::header(message, b"LAB-29", true)?;
        let msh = super::msh(message)?;
        let d = message.delimiters();
        if !raw(msh.segment(), 9).split(|b| *b == d.component).eq([
            b"OUL".as_slice(),
            b"R22",
            b"OUL_R22",
        ]) {
            return Err(content("MSH", 9));
        }
        let mut specimen = None;
        let mut container = None;
        let mut order = None;
        let mut parent = None;
        let mut observations: Vec<Observation<'a>> = Vec::new();
        let mut specimen_observations = Vec::new();
        let mut result_phase = ResultPhase::NoResult;
        let mut has_container_inventory = false;
        let mut attached = Vec::new();
        let mut order_status = None;
        let mut order_count = 0usize;
        let mut specimen_count = 0usize;
        let mut observation_count = 0usize;
        let mut has_patient = false;
        let mut has_visit = false;
        let mut has_timing = false;
        for (ordinal, segment) in message.segments().enumerate().skip(1) {
            super::profile::validate_segment(segment)?;
            match segment.id() {
                b"PID" => {
                    if has_patient || specimen.is_some() {
                        return Err(content("PID", 0));
                    }
                    has_patient = true;
                    parent = Some(Parent::Patient(ordinal));
                }
                b"PV1" => {
                    if !has_patient || has_visit || specimen.is_some() {
                        return Err(content("PV1", 0));
                    }
                    has_visit = true;
                    attached.push(Attached {
                        parent: parent.ok_or(content("PV1", 0))?,
                        segment,
                    });
                }
                b"SPM" => {
                    if specimen.is_some() && order.is_none() {
                        return Err(content("OBR", 0));
                    }
                    specimen_count = specimen_count.checked_add(1).ok_or(content("SPM", 1))?;
                    if raw(segment, 1) != specimen_count.to_string().as_bytes() {
                        return Err(content("SPM", 1));
                    }
                    check_complete(order_status, observation_count)?;
                    if order.is_some() && order_status.is_none() {
                        return Err(content("ORC", 0));
                    }
                    specimen = Some(segment);
                    container = None;
                    order = None;
                    order_status = None;
                    parent = Some(Parent::Specimen(ordinal));
                }
                b"SAC" => {
                    if specimen.is_none() || container.is_some() {
                        return Err(content("SAC", 0));
                    }
                    container = Some(segment);
                    has_container_inventory = false;
                    order = None;
                    parent = Some(Parent::Container(ordinal));
                }
                b"OBR" => {
                    check_complete(order_status, observation_count)?;
                    if order.is_some() && order_status.is_none() {
                        return Err(content("ORC", 0));
                    }
                    if container.is_none() {
                        return Err(content("OBR", 0));
                    }
                    for field in [2, 4] {
                        if raw(segment, field).is_empty() {
                            return Err(content("OBR", field));
                        }
                    }
                    order = Some(segment);
                    order_status = None;
                    observation_count = 0;
                    has_timing = false;
                    order_count = order_count.checked_add(1).ok_or(content("OBR", 0))?;
                    parent = Some(Parent::Order(ordinal));
                }
                b"ORC" => {
                    if order.is_none() || raw(segment, 1) != b"SC" || order_status.is_some() {
                        return Err(content("ORC", 1));
                    }
                    if !matches!(raw(segment, 5), b"SC" | b"IP" | b"CM" | b"CA")
                        || !raw(segment, 2).is_empty()
                        || !raw(segment, 9).is_empty()
                    {
                        return Err(content("ORC", 5));
                    }
                    order_status = Some(raw(segment, 5));
                    attached.push(Attached {
                        parent: parent.ok_or(content("ORC", 0))?,
                        segment,
                    });
                }
                b"TQ1" => {
                    if order_status.is_none() || has_timing || observation_count != 0 {
                        return Err(content("TQ1", 0));
                    }
                    has_timing = true;
                    attached.push(Attached {
                        parent: parent.ok_or(content("TQ1", 0))?,
                        segment,
                    });
                }
                b"OBX" => {
                    if order.is_none() && container.is_none() {
                        let specimen = specimen.ok_or(content("SPM", 0))?;
                        specimen_observations.push(SpecimenObservation {
                            specimen,
                            segment,
                            values: values(segment)?,
                        });
                        parent = Some(Parent::SpecimenObservation(ordinal));
                        continue;
                    }
                    if !matches!(order_status, Some(b"IP" | b"CM")) {
                        return Err(content("OBX", 0));
                    }
                    for field in [3, 4, 11] {
                        if raw(segment, field).is_empty() {
                            return Err(content("OBX", field));
                        }
                    }
                    let sequence = observation_count.checked_add(1).ok_or(content("OBX", 1))?;
                    if raw(segment, 1) != sequence.to_string().as_bytes() {
                        return Err(content("OBX", 1));
                    }
                    let observation = Observation {
                        order_ordinal: order_count,
                        segment,
                        order: order.ok_or(content("OBR", 0))?,
                        specimen: specimen.ok_or(content("SPM", 0))?,
                        container: container.ok_or(content("SAC", 0))?,
                        status: status(raw(segment, 11))?,
                        values: values(segment)?,
                    };
                    observations.push(observation);
                    result_phase = ResultPhase::Observation;
                    observation_count =
                        observation_count.checked_add(1).ok_or(content("OBX", 0))?;
                    parent = Some(Parent::Observation(ordinal));
                }
                b"TCD" => {
                    match result_phase {
                        ResultPhase::Observation => {}
                        ResultPhase::NoResult
                        | ResultPhase::Detail
                        | ResultPhase::Inventory
                        | ResultPhase::Notes => return Err(content("TCD", 0)),
                    }
                    result_phase = ResultPhase::Detail;
                    if !matches!(parent, Some(Parent::Observation(_))) {
                        return Err(content("TCD", 0));
                    }
                    let observation = observations.last().ok_or(content("TCD", 0))?;
                    if raw(segment, 1) != observation.code() {
                        return Err(Error::Inconsistent {
                            segment: "TCD",
                            field: 1,
                        });
                    }
                    attached.push(Attached {
                        parent: parent.ok_or(content("TCD", 0))?,
                        segment,
                    });
                }
                b"NTE" => {
                    match parent {
                        Some(Parent::Patient(_)) => {}
                        Some(Parent::Order(_)) => {
                            if order_status.is_none() {
                                return Err(content("ORC", 0));
                            }
                        }
                        Some(Parent::Observation(_)) => {
                            result_phase = ResultPhase::Notes;
                        }
                        Some(
                            Parent::Specimen(_)
                            | Parent::Container(_)
                            | Parent::SpecimenObservation(_),
                        )
                        | None => return Err(content("NTE", 0)),
                    }
                    attached.push(Attached {
                        parent: parent.ok_or(content("NTE", 0))?,
                        segment,
                    });
                }
                b"INV" => {
                    match parent {
                        Some(Parent::Container(_)) => {
                            if has_container_inventory
                                || specimen.is_none_or(|s| {
                                    raw(s, 11).split(|b| *b == d.component).next()
                                        != Some(b"Q".as_slice())
                                })
                            {
                                return Err(content("INV", 0));
                            }
                            has_container_inventory = true;
                        }
                        Some(Parent::Observation(_)) => {
                            match result_phase {
                                ResultPhase::Observation
                                | ResultPhase::Detail
                                | ResultPhase::Inventory => {}
                                ResultPhase::NoResult | ResultPhase::Notes => {
                                    return Err(content("INV", 0));
                                }
                            }
                            result_phase = ResultPhase::Inventory;
                        }
                        Some(
                            Parent::Patient(_)
                            | Parent::Specimen(_)
                            | Parent::Order(_)
                            | Parent::SpecimenObservation(_),
                        )
                        | None => return Err(content("INV", 0)),
                    }
                    attached.push(Attached {
                        parent: parent.ok_or(content("INV", 0))?,
                        segment,
                    });
                }
                _ => return Err(content("OUL", 0)),
            }
        }
        check_complete(order_status, observation_count)?;
        if order.is_some() && order_status.is_none() {
            return Err(content("ORC", 0));
        }
        if order_count == 0 || order.is_none() {
            return Err(content("OBR", 0));
        }
        let mut code_counts = std::collections::BTreeMap::new();
        for observation in &observations {
            let count = code_counts
                .entry((observation.order_ordinal, observation.code()))
                .or_insert(0usize);
            *count = count.checked_add(1).ok_or(content("OBX", 4))?;
        }
        for observation in &observations {
            let repeated_code = code_counts
                .get(&(observation.order_ordinal, observation.code()))
                .is_some_and(|count| *count > 1);
            if repeated_code {
                let parts: Vec<_> = observation.sub_id().split(|b| *b == d.component).collect();
                if parts.get(1).is_none_or(|v| v.is_empty())
                    || parts.get(2).is_none_or(|v| v.is_empty())
                {
                    return Err(content("OBX", 4));
                }
            }
        }
        Ok(Self {
            specimen_observations,
            message,
            observations,
            attached,
        })
    }

    /// Returns complete observations only after the entire message passed validation.
    #[must_use]
    pub fn observations(&self) -> &[Observation<'a>] {
        &self.observations
    }

    /// Returns specimen observations without falsely assigning an AWOS to them.
    #[must_use]
    pub fn specimen_observations(&self) -> &[SpecimenObservation<'a>] {
        &self.specimen_observations
    }

    /// Returns scoped notes, inventory, timing and order-control information.
    #[must_use]
    pub fn attached(&self) -> &[Attached<'a>] {
        &self.attached
    }

    /// Returns the complete original, including status-only orders and unknown fields.
    #[must_use]
    pub fn message(&self) -> &'a Message {
        self.message
    }
}

fn check_complete(order_status: Option<&[u8]>, count: usize) -> Result<(), Error> {
    if order_status == Some(b"CM") && count == 0 {
        return Err(content("OBX", 0));
    }
    Ok(())
}

/// Renders LAW ACK/R22 through the core acknowledgment function and adds its profile.
/// # Errors
/// Rejects unsafe positive responsibility, metadata, missing error details or output limits.
pub fn result_acknowledgment(
    message: &Message,
    response: Response<'_>,
    code: AckCode,
    responsibility: Responsibility,
    error: Option<AckError<'_>>,
    options: Options,
) -> Result<Option<Message>, Error> {
    if !super::profile::timestamp(response.timestamp, true) {
        return Err(content("MSH", 7));
    }
    if code == AckCode::Aa {
        Results::new(message)?;
    }
    if (code == AckCode::Aa) == error.is_some() {
        return Err(content("ERR", 0));
    }
    let Some(ack) = crate::ack::acknowledgment_with_limit(
        message,
        response,
        code,
        responsibility,
        error,
        options,
        50,
    )?
    else {
        return Ok(None);
    };
    let mut wire = Wire::new(options.limits.message_bytes, message.delimiters().field);
    let msh = super::msh(&ack)?.segment();
    wire.append(b"MSH")?;
    let mut profile = b"LAB-29".to_vec();
    profile.push(message.delimiters().component);
    profile.extend_from_slice(b"IHE");
    for field in 2..=21 {
        wire.field(if field == 21 {
            &profile
        } else {
            raw(msh, field)
        })?;
    }
    wire.append(b"\r")?;
    super::responses::append_ack_segments(&mut wire, &ack)?;
    Ok(Some(Message::parse(
        &wire.bytes,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )?))
}
