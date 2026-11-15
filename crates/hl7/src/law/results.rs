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
                if !decimal(value) {
                    return Err(content("OBX", 5));
                }
                Value::Number(value)
            }
            b"SN" => Value::StructuredNumber(value),
            b"ST" | b"TX" | b"FT" => Value::Text(value),
            b"CE" | b"CWE" => Value::Coded(value),
            b"EI" => Value::Identity(value),
            b"ED" => Value::Encapsulated(value),
            b"RP" => Value::Reference(value),
            b"NA" => Value::Array(value),
            b"DT" | b"TM" | b"TS" => Value::Time(value),
            _ => return Err(content("OBX", 2)),
        });
    }
    Ok(values)
}

/// Parent of a retained supplemental segment, identified by source ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
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
        let mut attached = Vec::new();
        let mut order_status = None;
        let mut order_count = 0usize;
        let mut observation_count = 0usize;
        let mut has_patient = false;
        let mut has_visit = false;
        let mut has_timing = false;
        let mut has_tcd = false;
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
                    has_tcd = false;
                    if !matches!(order_status, Some(b"IP" | b"CM")) {
                        return Err(content("OBX", 0));
                    }
                    for field in [3, 4, 11] {
                        if raw(segment, field).is_empty() {
                            return Err(content("OBX", field));
                        }
                    }
                    let observation = Observation {
                        segment,
                        order: order.ok_or(content("OBR", 0))?,
                        specimen: specimen.ok_or(content("SPM", 0))?,
                        container: container.ok_or(content("SAC", 0))?,
                        status: status(raw(segment, 11))?,
                        values: values(segment)?,
                    };
                    observations.push(observation);
                    observation_count =
                        observation_count.checked_add(1).ok_or(content("OBX", 0))?;
                    parent = Some(Parent::Observation(ordinal));
                }
                b"TCD" => {
                    if has_tcd {
                        return Err(content("TCD", 0));
                    }
                    has_tcd = true;
                    if !matches!(parent, Some(Parent::Observation(_))) {
                        return Err(content("TCD", 0));
                    }
                    let observation = observations.last().ok_or(content("TCD", 0))?;
                    if raw(segment, 1) != observation.code() {
                        return Err(content("TCD", 1));
                    }
                    attached.push(Attached {
                        parent: parent.ok_or(content("TCD", 0))?,
                        segment,
                    });
                }
                b"NTE" | b"INV" => {
                    attached.push(Attached {
                        parent: parent.ok_or(content("NTE", 0))?,
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
        if order_count == 0 {
            return Err(content("OBR", 0));
        }
        Ok(Self {
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
    for segment in ack.segments().skip(1) {
        wire.append(segment.raw())?;
        wire.append(b"\r")?;
    }
    Ok(Some(Message::parse(
        &wire.bytes,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )?))
}
