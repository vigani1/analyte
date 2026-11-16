//! LAW result grouping belongs to the HL7 profile syntax boundary.
use super::responses::Wire;
use super::{Error, Identity, content, raw};
use crate::{AckCode, AckError, Field, Message, Options, Response, Responsibility, Segment};

/// Source observation state; revisions remain a host responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultStatus {
    /// Final observation; never inferred from a value or earlier message.
    Final,
    /// Preliminary observation.
    Preliminary,
    /// Explicit C status: correction of a previously final observation.
    Corrected,
    /// Result entered but not verified.
    Unverified,
    /// This run could not obtain a result.
    Unobtainable,
}

impl ResultStatus {
    /// Parses exactly the C.7-18 LAW subset, retaining correction only for C.
    /// # Errors
    /// Returns OBX-11 for any other status spelling.
    pub fn parse(value: &[u8]) -> Result<Self, Error> {
        match value {
            b"F" => Ok(Self::Final),
            b"P" => Ok(Self::Preliminary),
            b"C" => Ok(Self::Corrected),
            b"R" => Ok(Self::Unverified),
            b"X" => Ok(Self::Unobtainable),
            _ => Err(content("OBX", 11)),
        }
    }

    /// Returns the exact declared status code without a lifecycle inference.
    #[must_use]
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Final => b"F",
            Self::Preliminary => b"P",
            Self::Corrected => b"C",
            Self::Unverified => b"R",
            Self::Unobtainable => b"X",
        }
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
    /// ST or TX text; escapes remain available for lossless decoding.
    Text(&'a [u8]),
    /// Coded CE value.
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

impl<'a> Value<'a> {
    /// Returns the original encoded spelling, without numeric or unit conversion.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        match self {
            Self::Null => b"\"\"",
            Self::Identity(value)
            | Self::Number(value)
            | Self::StructuredNumber(value)
            | Self::Text(value)
            | Self::Coded(value)
            | Self::Encapsulated(value)
            | Self::Reference(value)
            | Self::Array(value)
            | Self::Time(value) => value,
        }
    }
}

// Field validation happens once at the message boundary; these views only classify it.
fn values(segment: Segment<'_>) -> Result<Vec<Value<'_>>, Error> {
    let field = segment.field(5).ok_or(content("OBX", 5))?;
    if field.is_null() {
        return Ok(vec![Value::Null]);
    }
    field
        .repetitions()
        .map(|repetition| {
            let value = repetition.raw();
            Ok(match raw(segment, 2) {
                b"NM" => Value::Number(value),
                b"SN" => Value::StructuredNumber(value),
                b"ST" | b"TX" => Value::Text(value),
                b"CE" => Value::Coded(value),
                b"EI" => Value::Identity(value),
                b"ED" => Value::Encapsulated(value),
                b"RP" => Value::Reference(value),
                b"NA" => Value::Array(value),
                _ => return Err(content("OBX", 2)),
            })
        })
        .collect()
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
    detail: Option<super::TestDetail<'a>>,
    values: Vec<Value<'a>>,
}

impl<'a> Observation<'a> {
    /// Returns validated TCD data with original dilution spelling and adjustment knowledge.
    #[must_use]
    pub fn test_detail(&self) -> Option<super::TestDetail<'a>> {
        self.detail
    }

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
        Self::with_profile(message, super::Profile::receive())
    }

    /// Validates results with explicit negotiated options and trusted authority bindings.
    /// # Errors
    /// Rejects invalid content or unresolved mixed authority names without partial results.
    pub fn with_profile(message: &'a Message, profile: super::Profile<'_>) -> Result<Self, Error> {
        profile.validate_message(message, super::ControlDirection::Receive)?;
        super::profile::header(message, b"LAB-29", true)?;
        let msh = super::msh(message)?;
        let d = message.delimiters();
        if !raw(msh.segment(), 9)
            .split(|b| *b == d.component)
            .take(3)
            .eq([b"OUL".as_slice(), b"R22", b"OUL_R22"])
        {
            return Err(content("MSH", 9));
        }
        let mut groups = ResultGroups::new();
        let mut occurrences = std::collections::BTreeMap::<&[u8], usize>::new();
        for (ordinal, segment) in message.segments().enumerate().skip(1) {
            let occurrence = occurrences.entry(segment.id()).or_default();
            *occurrence = occurrence.checked_add(1).ok_or(content("OUL", 0))?;
            let checked = groups.parse_segment(segment, ordinal, profile);
            checked.map_err(|error| error.at_segment(segment, *occurrence))?;
        }
        check_complete(groups.order_status, groups.observation_count)?;
        if groups.order.is_some() && groups.order_status.is_none() {
            return Err(content("ORC", 0));
        }
        if groups.order_count == 0 || groups.order.is_none() {
            return Err(content("OBR", 0));
        }
        let mut code_counts = std::collections::BTreeMap::new();
        for observation in &groups.observations {
            let count = code_counts
                .entry((
                    observation.order_ordinal,
                    super::control::part(observation.code(), 1),
                    super::control::part(observation.code(), 3),
                ))
                .or_insert(0usize);
            *count = count.checked_add(1).ok_or(content("OBX", 4))?;
        }
        for observation in &groups.observations {
            let repeated_code = code_counts
                .get(&(
                    observation.order_ordinal,
                    super::control::part(observation.code(), 1),
                    super::control::part(observation.code(), 3),
                ))
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
            specimen_observations: groups.specimen_observations,
            message,
            observations: groups.observations,
            attached: groups.attached,
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

fn check_complete(order_status: Option<super::OrderStatus>, count: usize) -> Result<(), Error> {
    if order_status == Some(super::OrderStatus::Complete) && count == 0 {
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
    let Some(ack) = crate::ack::acknowledgment_with_msh(
        super::msh(message)?,
        response,
        code,
        responsibility,
        error,
        options,
        crate::ack::AckProfile::Law,
    )?
    else {
        return Ok(None);
    };
    let mut wire = Wire::new(options.limits.message_bytes, message.delimiters().field);
    let msh = ack.segments().next().ok_or(content("MSH", 0))?;
    wire.append(b"MSH")?;
    let mut profile = b"LAB-29".to_vec();
    profile.push(message.delimiters().component);
    profile.extend_from_slice(b"IHE");
    for field in 2..=21 {
        wire.field(if field == 21 {
            &profile
        } else {
            super::control::reply_field(msh, field)
        })?;
    }
    wire.append(b"\r")?;
    super::responses::append_ack_segments(&mut wire, &ack)?;
    let output = super::parse_message(
        &wire.bytes,
        Options {
            terminator: crate::Terminator::Cr,
            ..options
        },
    )?;
    super::validate_control(&output, super::ControlDirection::Send)?;
    Ok(Some(output))
}

mod groups;
use groups::ResultGroups;
