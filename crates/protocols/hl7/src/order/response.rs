//! Interprets HL7 order acknowledgments.
//! Kept whole: Validates one correlated order response and its bounded order-owned groups atomically.
use super::{Control, Error, Family, OrderMessage, SpecimenResponse, invalid, laboratory, raw};
use crate::{AckCode, Field, Message, Msa, Msh, Segment, Version};
use std::collections::{BTreeMap, BTreeSet};

/// Peer-reported request outcome, independent of MSA and the host lifecycle.
#[derive(Debug, Clone, Copy)]
pub enum Outcome<'a> {
    /// OK in response to NW.
    Accepted,
    /// UA in response to NW; no rejection reason is invented if it is absent.
    Rejected {
        /// Original ORC-16 CE, when supplied.
        reason: Option<Field<'a>>,
    },
    /// CR in response to CA; this is evidence for the host, not a state update.
    CancelAccepted,
    /// UC in response to CA.
    UnableToCancel {
        /// Original ORC-16 CE, when supplied.
        reason: Option<Field<'a>>,
    },
    /// This message provides no decision for the sent order, even if MSA is AA.
    /// It does not grant permission to resend or assume that nothing happened.
    NotReported,
}

/// A sent order and its matching response, or an explicitly missing decision.
#[derive(Debug)]
pub struct Decision<'a> {
    placer: Field<'a>,
    request: Control,
    outcome: Outcome<'a>,
    common: Option<Segment<'a>>,
    detail: Option<Segment<'a>>,
    notes: Vec<Segment<'a>>,
    filler: Option<Field<'a>>,
    timing: Vec<Segment<'a>>,
    specimens: Vec<SpecimenResponse<'a>>,
}

impl<'a> Decision<'a> {
    /// Returns the stable caller-supplied placer identity, including namespaces.
    #[must_use]
    pub fn placer(&self) -> Field<'a> {
        self.placer
    }

    /// Returns NW or CA from the sent message, never a reconstructed request.
    #[must_use]
    pub fn request(&self) -> Control {
        self.request
    }

    /// Returns the reported outcome; missing decisions never imply acceptance.
    #[must_use]
    pub fn outcome(&self) -> Outcome<'a> {
        self.outcome
    }

    /// Returns the original peer ORC, retaining additional un-interpreted fields.
    #[must_use]
    pub fn common(&self) -> Option<Segment<'a>> {
        self.common
    }

    /// Returns the original optional order detail, including placer/sample text.
    #[must_use]
    pub fn detail(&self) -> Option<Segment<'a>> {
        self.detail
    }

    /// Returns notes belonging to this response order, in original order.
    #[must_use]
    pub fn notes(&self) -> &[Segment<'a>] {
        &self.notes
    }

    /// Returns only the filler identity actually supplied by the peer.
    #[must_use]
    pub fn filler(&self) -> Option<Field<'a>> {
        self.filler
    }

    /// Returns O22 timing segments owned by this order; no schedule runs here.
    #[must_use]
    pub fn timing(&self) -> &[Segment<'a>] {
        &self.timing
    }

    /// Returns each O22 specimen with its own containers; missing data is not invented.
    #[must_use]
    pub fn specimens(&self) -> &[SpecimenResponse<'a>] {
        &self.specimens
    }
}

/// Correlated ORR or non-LAW ORL response, selected by the sent transaction.
/// Message acceptance never substitutes for an order decision.
#[derive(Debug)]
pub struct OrderResponse<'a> {
    message: &'a Message,
    acceptance: AckCode,
    decisions: Vec<Decision<'a>>,
}

fn field(segment: Segment<'_>, number: usize) -> Option<Field<'_>> {
    segment.field(number).filter(|v| !v.raw().is_empty())
}

fn identity_key<'a>(
    field: Field<'a>,
    segment: &str,
    number: usize,
    version: Version,
) -> Result<Vec<&'a [u8]>, Error> {
    if field.is_null()
        || field.raw().len() > super::EI_MAX_BYTES
        || field.repetitions().count() != 1
    {
        return Err(invalid(segment, number));
    }
    let value = field
        .repetitions()
        .next()
        .ok_or_else(|| invalid(segment, number))?;
    let mut parts = Vec::new();
    for component in value.components() {
        if component.subcomponents().count() != 1 || component.raw() == b"\"\"" {
            return Err(invalid(segment, number));
        }
        parts.push(component.raw());
    }
    if parts.len() > 4 || parts.first().is_none_or(|v| v.is_empty()) {
        return Err(invalid(segment, number));
    }
    let universal = parts.get(2).copied().unwrap_or_default();
    let kind = parts.get(3).copied().unwrap_or_default();
    if universal.is_empty() != kind.is_empty() || !crate::datatypes::uid(kind, version) {
        return Err(invalid(segment, number));
    }
    // EI trailing empty components change spelling but not its assigning namespace.
    while parts.last().is_some_and(|v| v.is_empty()) {
        parts.pop();
    }
    Ok(parts)
}

fn identities<'a>(
    orc: Segment<'a>,
    obr: Option<Segment<'a>>,
    number: usize,
    required: bool,
    version: Version,
) -> Result<Option<Field<'a>>, Error> {
    let primary = field(orc, number);
    let detail = obr.and_then(|s| field(s, number));
    let primary_key = primary
        .map(|f| identity_key(f, "ORC", number, version))
        .transpose()?;
    let detail_key = detail
        .map(|f| identity_key(f, "OBR", number, version))
        .transpose()?;
    if let (Some(primary), Some(detail)) = (&primary_key, &detail_key)
        && primary != detail
    {
        return Err(Error::Correlation {
            segment: "OBR",
            field: number,
        });
    }
    let result = primary.or(detail);
    if required && result.is_none() {
        return Err(invalid("ORC", number));
    }
    Ok(result)
}

fn reason(orc: Segment<'_>, version: Version) -> Result<Option<Field<'_>>, Error> {
    let value = field(orc, 16);
    if let Some(value) = value {
        let max = version
            .order_rules()
            .ok_or_else(|| invalid("MSH", 12))?
            .code_bytes;
        if value.raw().len() > max || value.is_null() || value.repetitions().count() != 1 {
            return Err(invalid("ORC", 16));
        }
        let parts = value
            .repetitions()
            .next()
            .ok_or_else(|| invalid("ORC", 16))?;
        if !crate::datatypes::coded(&parts.components().collect::<Vec<_>>(), version) {
            return Err(invalid("ORC", 16));
        }
    }
    Ok(value)
}

fn validate_order_status(orc: Segment<'_>) -> Result<(), Error> {
    if !matches!(
        raw(orc, 5),
        b"" | b"A" | b"CA" | b"CM" | b"DC" | b"ER" | b"HD" | b"IP" | b"RP" | b"SC"
    ) {
        return Err(invalid("ORC", 5));
    }
    Ok(())
}

fn outcome<'a>(
    request: Control,
    response: Control,
    reason: Option<Field<'a>>,
) -> Result<Outcome<'a>, Error> {
    match (request, response) {
        (Control::New, Control::Accepted) => Ok(Outcome::Accepted),
        (Control::New, Control::Rejected) => Ok(Outcome::Rejected { reason }),
        (Control::Cancel, Control::Cancelled) => Ok(Outcome::CancelAccepted),
        (Control::Cancel, Control::UnableToCancel) => Ok(Outcome::UnableToCancel { reason }),
        (
            Control::New,
            Control::New | Control::Cancel | Control::Cancelled | Control::UnableToCancel,
        )
        | (
            Control::Cancel,
            Control::New | Control::Cancel | Control::Accepted | Control::Rejected,
        )
        | (
            Control::Accepted | Control::Rejected | Control::Cancelled | Control::UnableToCancel,
            Control::New
            | Control::Cancel
            | Control::Accepted
            | Control::Rejected
            | Control::Cancelled
            | Control::UnableToCancel,
        ) => Err(Error::Control),
    }
}

impl<'a> OrderResponse<'a> {
    /// Reads ORR/O02 for ORM/O01 or ORL/O22 for OML/O21, without fallback between families.
    ///
    /// ORC/OBR duplicate identities must agree. Unknown, duplicate and mismatched
    /// order responses are rejected atomically. Missing order responses remain
    /// `NotReported`; MSA AA alone never turns them into accepted work. Original
    /// ERR, PID and header/patient notes remain in `message()`.
    ///
    /// O22 order decisions require its patient response group. Only the declared
    /// single-window TQ1 and one-SPM-per-order subset is interpreted. Returned
    /// specimens must match the sent specimen where one was supplied. Extra
    /// specimen data for cancellation is retained as peer-reported context;
    /// it does not acquire an inferred identity from the request. Containers stay
    /// with their SPM, and additional raw fields remain for host interpretation.
    ///
    /// # Errors
    /// Rejects wrong editions/events/control IDs, invalid MSA, unsupported grammar,
    /// malformed order identities and unknown or unexpected ORC-1 values.
    pub fn new(message: &'a Message, sent: &'a OrderMessage) -> Result<Self, Error> {
        let sent_header = Msh::new(sent.message())?;
        let version = validate_response_header(message, sent, sent_header)?;
        let mut segments = message.segments().skip(1).peekable();
        let msa_segment = segments.next().ok_or_else(|| invalid("MSA", 0))?;
        let msa = response_acceptance(msa_segment, version, sent_header)?;
        let mut decisions = Vec::new();
        let mut expected = BTreeMap::new();
        let mut sent_fillers = Vec::new();
        for common in sent.message().segments().filter(|s| s.id() == b"ORC") {
            let placer = field(common, 2).ok_or_else(|| invalid("ORC", 2))?;
            let request = Control::parse(raw(common, 1))?;
            match request {
                Control::New | Control::Cancel => {}
                Control::Accepted
                | Control::Rejected
                | Control::Cancelled
                | Control::UnableToCancel => return Err(Error::Control),
            }
            let index = decisions.len();
            if expected
                .insert(identity_key(placer, "ORC", 2, version)?, index)
                .is_some()
            {
                return Err(Error::Duplicate { field: 2 });
            }
            sent_fillers.push(
                field(common, 3)
                    .map(|f| identity_key(f, "ORC", 3, version))
                    .transpose()?,
            );
            decisions.push(Decision {
                placer,
                request,
                outcome: Outcome::NotReported,
                common: None,
                detail: None,
                notes: Vec::new(),
                filler: None,
                timing: Vec::new(),
                specimens: Vec::new(),
            });
        }
        let mut errors = 0_usize;
        while segments.peek().is_some_and(|s| s.id() == b"ERR") {
            let error = segments.next().ok_or_else(|| invalid("ERR", 0))?;
            errors = errors.checked_add(1).ok_or(Error::Limit)?;
            if version == Version::V231 && errors > 1 {
                return Err(invalid("ERR", 0));
            }
            crate::ErrView::new(error, version)?;
        }
        if sent.family == Family::Oml {
            while segments.peek().is_some_and(|s| s.id() == b"SFT") {
                segments.next();
            }
        }
        while segments.peek().is_some_and(|s| s.id() == b"NTE") {
            segments.next();
        }
        let has_patient = segments.peek().is_some_and(|s| s.id() == b"PID");
        if has_patient {
            validate_patient(segments.next().ok_or_else(|| invalid("PID", 0))?, version)?;
            while sent.family == Family::Orm && segments.peek().is_some_and(|s| s.id() == b"NTE") {
                segments.next();
            }
        }
        if sent.family == Family::Oml && segments.peek().is_some() && !has_patient {
            return Err(invalid("PID", 0));
        }
        let specimens = match sent.family {
            Family::Orm => Vec::new(),
            Family::Oml => laboratory::sent_specimens(sent.message(), decisions.len())?,
        };
        let lab = match sent.family {
            Family::Orm => None,
            Family::Oml => Some(laboratory::ReplyContext {
                message,
                sent_message: sent.message(),
                sent_specimens: &specimens,
            }),
        };
        read_orders(
            &mut segments,
            version,
            &expected,
            &sent_fillers,
            &mut decisions,
            lab,
        )?;
        Ok(Self {
            message,
            acceptance: msa.code(),
            decisions,
        })
    }

    /// Returns MSA application acceptance independently of individual order decisions.
    #[must_use]
    pub fn acceptance(&self) -> AckCode {
        self.acceptance
    }

    /// Returns one outcome per sent order in sent order, including missing decisions.
    #[must_use]
    pub fn decisions(&self) -> &[Decision<'a>] {
        &self.decisions
    }

    /// Returns every original response byte for protected retention and further interpretation.
    #[must_use]
    pub fn message(&self) -> &'a Message {
        self.message
    }
}

fn read_orders<'a>(
    segments: &mut std::iter::Peekable<impl Iterator<Item = Segment<'a>>>,
    version: Version,
    expected: &BTreeMap<Vec<&[u8]>, usize>,
    sent_fillers: &[Option<Vec<&[u8]>>],
    decisions: &mut [Decision<'a>],
    lab: Option<laboratory::ReplyContext<'a, '_>>,
) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    let mut seen_fillers = BTreeSet::new();
    let reserved_fillers: BTreeMap<_, _> = sent_fillers
        .iter()
        .enumerate()
        .filter_map(|(index, key)| key.as_ref().map(|key| (key.clone(), index)))
        .collect();
    while let Some(common) = segments.next() {
        if common.id() != b"ORC" {
            return Err(invalid(
                std::str::from_utf8(common.id()).map_err(|_| crate::Error::Encoding)?,
                0,
            ));
        }
        let control = Control::parse(raw(common, 1))?;
        validate_order_status(common)?;
        let rules = version.order_rules().ok_or_else(|| invalid("MSH", 12))?;
        refuse_fields(common, rules.withdrawn_orc)?;
        let mut timing = Vec::new();
        if lab.is_some() {
            while segments.peek().is_some_and(|s| s.id() == b"TQ1") {
                if !timing.is_empty() {
                    return Err(invalid("TQ1", 0));
                }
                let tq1 = segments.next().ok_or_else(|| invalid("TQ1", 0))?;
                laboratory::timing(tq1, version)?;
                timing.push(tq1);
            }
        }
        let detail = if segments.peek().is_some_and(|s| s.id() == b"OBR") {
            segments.next()
        } else {
            None
        };
        validate_order_detail(common, detail, version)?;
        let placer =
            identities(common, detail, 2, true, version)?.ok_or_else(|| invalid("ORC", 2))?;
        let filler = identities(common, detail, 3, false, version)?;
        let order_key = identity_key(placer, "ORC", 2, version)?;
        let index = *expected.get(&order_key).ok_or(Error::Correlation {
            segment: "ORC",
            field: 2,
        })?;
        let specimens = if let Some(context) = lab {
            if detail.is_some() {
                laboratory::read_specimens(segments, context, index)?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if !seen.insert(index) {
            return Err(Error::Duplicate { field: 2 });
        }
        if let Some(filler) = filler {
            let filler_key = identity_key(filler, "ORC", 3, version)?;
            if reserved_fillers
                .get(&filler_key)
                .is_some_and(|owner| *owner != index)
            {
                return Err(Error::Correlation {
                    segment: "ORC",
                    field: 3,
                });
            }
            if !seen_fillers.insert(filler_key) {
                return Err(Error::Duplicate { field: 3 });
            }
        }
        if let (Some(Some(expected)), Some(filler)) = (sent_fillers.get(index), filler)
            && *expected != identity_key(filler, "ORC", 3, version)?
        {
            return Err(Error::Correlation {
                segment: "ORC",
                field: 3,
            });
        }
        let decision = decisions.get_mut(index).ok_or(Error::Correlation {
            segment: "ORC",
            field: 2,
        })?;
        decision.outcome = outcome(decision.request, control, reason(common, version)?)?;
        decision.common = Some(common);
        decision.detail = detail;
        decision.filler = filler;
        decision.timing = timing;
        decision.specimens = specimens;
        while lab.is_none() && segments.peek().is_some_and(|s| s.id() == b"NTE") {
            decision
                .notes
                .push(segments.next().ok_or_else(|| invalid("NTE", 0))?);
        }
    }
    Ok(())
}

fn validate_order_detail(
    common: Segment<'_>,
    detail: Option<Segment<'_>>,
    version: Version,
) -> Result<(), Error> {
    if let Some(obr) = detail {
        refuse_fields(
            obr,
            version
                .order_rules()
                .ok_or_else(|| invalid("MSH", 12))?
                .withdrawn_obr,
        )?;
        let test = field(obr, 4).ok_or_else(|| invalid("OBR", 4))?;
        let max = version
            .order_rules()
            .ok_or_else(|| invalid("MSH", 12))?
            .code_bytes;
        if test.is_null() || test.raw().len() > max || test.repetitions().count() != 1 {
            return Err(invalid("OBR", 4));
        }
        let ce = test.repetitions().next().ok_or_else(|| invalid("OBR", 4))?;
        if !crate::datatypes::coded(&ce.components().collect::<Vec<_>>(), version) {
            return Err(invalid("OBR", 4));
        }
        if !raw(common, 7).is_empty() && !raw(obr, 27).is_empty() && raw(common, 7) != raw(obr, 27)
        {
            return Err(Error::Correlation {
                segment: "OBR",
                field: 27,
            });
        }
    }
    Ok(())
}

fn validate_response_header(
    message: &Message,
    sent: &OrderMessage,
    sent_header: Msh<'_>,
) -> Result<Version, Error> {
    let header = Msh::new(message)?;
    let version = header.version();
    if version != sent_header.version() {
        return Err(Error::Correlation {
            segment: "MSH",
            field: 12,
        });
    }
    let mut kind = header.message_type();
    let [code, event, structure] = sent.family.response();
    let valid_code = kind.next() == Some(code);
    let valid_event = kind.next() == Some(event);
    let valid_structure = match (sent.family, kind.next()) {
        (Family::Orm, None) => true,
        (Family::Orm | Family::Oml, Some(value)) => value == structure,
        (Family::Oml, None) => false,
    };
    if !valid_code || !valid_event || !valid_structure {
        return Err(invalid("MSH", 9));
    }
    if header.control_id() == sent_header.control_id() {
        return Err(Error::Correlation {
            segment: "MSH",
            field: 10,
        });
    }
    Ok(version)
}

fn validate_acceptance(msa: Msa<'_>, sent_header: Msh<'_>) -> Result<(), Error> {
    if msa.control_id() != sent_header.control_id() {
        return Err(Error::Correlation {
            segment: "MSA",
            field: 2,
        });
    }
    match msa.code() {
        AckCode::Aa | AckCode::Ae | AckCode::Ar => {}
        AckCode::Ca | AckCode::Ce | AckCode::Cr => return Err(invalid("MSA", 1)),
    }
    Ok(())
}

fn response_acceptance<'a>(
    msa_segment: Segment<'a>,
    version: Version,
    sent_header: Msh<'_>,
) -> Result<Msa<'a>, Error> {
    for field in version
        .order_rules()
        .ok_or_else(|| invalid("MSH", 12))?
        .withdrawn_msa
    {
        if !raw(msa_segment, *field).is_empty() {
            return Err(invalid("MSA", *field));
        }
    }
    let msa = Msa::for_version(msa_segment, version)?;
    validate_acceptance(msa, sent_header)?;
    Ok(msa)
}

fn refuse_fields(segment: Segment<'_>, fields: &[usize]) -> Result<(), Error> {
    for number in fields {
        if !raw(segment, *number).is_empty() {
            return Err(invalid(
                std::str::from_utf8(segment.id()).map_err(|_| crate::Error::Encoding)?,
                *number,
            ));
        }
    }
    Ok(())
}

fn validate_patient(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    if !version
        .order_rules()
        .ok_or_else(|| invalid("MSH", 12))?
        .patient_type_required
    {
        return Ok(());
    }
    let identifiers = field(segment, 3)
        .filter(|f| !f.is_null())
        .ok_or_else(|| invalid("PID", 3))?;
    for repetition in identifiers.repetitions() {
        let parts = repetition.components().collect::<Vec<_>>();
        for (number, maximum) in [(1_usize, 15), (5, 5)] {
            let part = number
                .checked_sub(1)
                .and_then(|index| parts.get(index))
                .ok_or_else(|| invalid("PID", 3))?;
            super::types::scalar(part.raw(), "CX", number, maximum, true)?;
        }
    }
    field(segment, 5)
        .filter(|f| !f.is_null())
        .ok_or_else(|| invalid("PID", 5))?;
    Ok(())
}
