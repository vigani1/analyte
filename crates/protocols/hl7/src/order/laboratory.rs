//! Groups HL7 O21 and O22 segments into borrowed views.
use super::{Error, invalid};
use crate::law::{FieldRule, Usage};

// Base SPM-2 is 80 bytes; its rule is independent of LAW's profile predicates.
pub(super) const SPECIMEN_ID_BYTES: usize = 80;
use crate::{Component, Field, Message, Segment, Version};

/// One peer-reported specimen and its own container group, never flattened.
#[derive(Debug)]
pub struct SpecimenResponse<'a> {
    segment: Segment<'a>,
    containers: Vec<Segment<'a>>,
}

impl<'a> SpecimenResponse<'a> {
    /// Returns the original SPM view, including assigning namespaces and filler ID.
    #[must_use]
    pub fn segment(&self) -> Segment<'a> {
        self.segment
    }

    /// Returns only the SAC segments belonging to this specimen.
    #[must_use]
    pub fn containers(&self) -> &[Segment<'a>] {
        &self.containers
    }
}

#[derive(Clone, Copy)]
pub(super) struct ReplyContext<'a, 's> {
    pub message: &'a Message,
    pub sent_message: &'a Message,
    pub sent_specimens: &'s [Option<Segment<'a>>],
}

fn rule(
    segment: Segment<'_>,
    name: &'static str,
    field: usize,
    required: bool,
    maximum: usize,
) -> Result<(), Error> {
    // FieldRule is the existing datatype-neutral check used by LAW. Its profile
    // tables are deliberately not called for a non-LAW transaction.
    FieldRule {
        segment: name,
        field,
        usage: if required {
            Usage::Required
        } else {
            Usage::RequiredIfAvailable
        },
        minimum: 1,
        maximum: 1,
        length: maximum,
    }
    .check(segment)
    .map_err(|error| match error {
        crate::law::Error::Syntax(error) => Error::Syntax(error),
        crate::law::Error::Content { segment, field } => invalid(segment, field),
        crate::law::Error::Inconsistent { segment, field } => Error::Correlation { segment, field },
        error @ (crate::law::Error::AtSegment { .. }
        | crate::law::Error::DilutionDefinition { .. }
        | crate::law::Error::Subcomponent { .. }
        | crate::law::Error::Authority { .. }
        | crate::law::Error::Component { .. }) => Error::FieldRule(error),
    })
}

fn value(segment: Segment<'_>, field: usize) -> Option<Field<'_>> {
    segment.field(field).filter(|f| !f.raw().is_empty())
}

fn sequence(segment: Segment<'_>, name: &'static str, first_only: bool) -> Result<(), Error> {
    rule(segment, name, 1, false, 4)?;
    if let Some(field) = value(segment, 1) {
        let ordinal = field.raw().iter().try_fold(0_u16, |n, b| {
            if !b.is_ascii_digit() {
                return None;
            }
            n.checked_mul(10)?
                .checked_add(u16::from(b.checked_sub(b'0')?))
        });
        if ordinal.is_none_or(|n| n == 0 || (first_only && n != 1)) {
            return Err(invalid(name, 1));
        }
    }
    Ok(())
}

fn entity(component: Component<'_>, name: &'static str, field: usize) -> Result<(), Error> {
    if component.raw().is_empty() {
        return Ok(());
    }
    let parts: Vec<_> = component.subcomponents().collect();
    entity_parts(&parts, name, field)
}

fn entity_parts(parts: &[&[u8]], name: &'static str, field: usize) -> Result<(), Error> {
    let id = parts.first().copied().unwrap_or_default();
    let universal = parts.get(2).copied().unwrap_or_default();
    let kind = parts.get(3).copied().unwrap_or_default();
    if parts.len() > 4
        || id.is_empty()
        || parts.iter().any(|p| *p == b"\"\"")
        || parts
            .iter()
            .zip([199, 20, 199, 6])
            .any(|(p, max)| p.len() > max)
        || universal.is_empty() != kind.is_empty()
        || !crate::datatypes::uid(kind, Version::V251)
    {
        return Err(invalid(name, field));
    }
    Ok(())
}

fn specimen(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    sequence(segment, "SPM", false)?;
    rule(segment, "SPM", 2, false, SPECIMEN_ID_BYTES)?;
    if let Some(field) = value(segment, 2)
        && !field.is_null()
    {
        let repetition = field
            .repetitions()
            .next()
            .ok_or_else(|| invalid("SPM", 2))?;
        let parts: Vec<_> = repetition.components().collect();
        if parts.len() > 2 || parts.iter().all(|p| p.raw().is_empty()) {
            return Err(invalid("SPM", 2));
        }
        for part in parts {
            entity(part, "SPM", 2)?;
        }
    }
    rule(segment, "SPM", 4, true, 250)?;
    let source = value(segment, 4).ok_or_else(|| invalid("SPM", 4))?;
    if !source.is_null() {
        let repetition = source
            .repetitions()
            .next()
            .ok_or_else(|| invalid("SPM", 4))?;
        if !if version == Version::V251 {
            crate::datatypes::cwe(&repetition.components().collect::<Vec<_>>())
        } else {
            crate::datatypes::coded(&repetition.components().collect::<Vec<_>>(), version)
        } {
            return Err(invalid("SPM", 4));
        }
    }
    Ok(())
}

fn container(segment: Segment<'_>) -> Result<(), Error> {
    let mut identified = false;
    for number in [3, 4] {
        rule(segment, "SAC", number, false, 80)?;
        if let Some(field) = value(segment, number)
            && !field.is_null()
        {
            let repetition = field
                .repetitions()
                .next()
                .ok_or_else(|| invalid("SAC", number))?;
            let parts: Vec<_> = repetition.components().collect();
            if parts.iter().any(|p| p.subcomponents().count() != 1) {
                return Err(invalid("SAC", number));
            }
            entity_parts(
                &parts.iter().map(|p| p.raw()).collect::<Vec<_>>(),
                "SAC",
                number,
            )?;
            identified = true;
        }
    }
    if !identified {
        return Err(invalid("SAC", 3));
    }
    Ok(())
}

pub(super) fn timing(segment: Segment<'_>, version: Version) -> Result<(), Error> {
    sequence(segment, "TQ1", true)?;
    for (index, field) in segment.fields().enumerate() {
        let number = index.saturating_add(1);
        if !matches!(number, 1 | 7 | 8 | 9) && !field.raw().is_empty() {
            return Err(invalid("TQ1", number));
        }
    }
    for number in [7, 8] {
        rule(
            segment,
            "TQ1",
            number,
            false,
            version
                .order_rules()
                .ok_or_else(|| invalid("MSH", 12))?
                .time_bytes,
        )?;
        if let Some(field) = value(segment, number) {
            let repetition = field
                .repetitions()
                .next()
                .ok_or_else(|| invalid("TQ1", number))?;
            if field.is_null()
                || !crate::datatypes::edition_timestamp(
                    &repetition.components().collect::<Vec<_>>(),
                    version,
                )
            {
                return Err(invalid("TQ1", number));
            }
        }
    }
    rule(segment, "TQ1", 9, false, 250)?;
    if let Some(field) = value(segment, 9) {
        let repetition = field
            .repetitions()
            .next()
            .ok_or_else(|| invalid("TQ1", 9))?;
        if field.is_null()
            || !if version == Version::V251 {
                crate::datatypes::cwe(&repetition.components().collect::<Vec<_>>())
            } else {
                crate::datatypes::coded(&repetition.components().collect::<Vec<_>>(), version)
            }
        {
            return Err(invalid("TQ1", 9));
        }
    }
    Ok(())
}

pub(super) fn sent_specimens(
    message: &Message,
    count: usize,
) -> Result<Vec<Option<Segment<'_>>>, Error> {
    let mut groups = vec![None; count];
    let mut next = 0_usize;
    let mut current = None;
    for segment in message.segments() {
        match segment.id() {
            b"ORC" => {
                current = Some(next);
                next = next.checked_add(1).ok_or(Error::Limit)?;
            }
            b"SPM" => {
                let slot = current
                    .and_then(|index| groups.get_mut(index))
                    .ok_or_else(|| invalid("SPM", 0))?;
                if slot.replace(segment).is_some() {
                    return Err(invalid("SPM", 0));
                }
            }
            _ => {}
        }
    }
    Ok(groups)
}

fn placer_key(segment: Segment<'_>, message: &Message) -> Result<Vec<String>, Error> {
    let field = value(segment, 2)
        .filter(|f| !f.is_null())
        .ok_or(Error::Correlation {
            segment: "SPM",
            field: 2,
        })?;
    let placer = field
        .repetitions()
        .next()
        .and_then(|r| r.components().next())
        .filter(|c| !c.raw().is_empty())
        .ok_or(Error::Correlation {
            segment: "SPM",
            field: 2,
        })?;
    let mut key = Vec::new();
    for part in placer.subcomponents() {
        key.push(crate::decode_text(part, message)?);
    }
    while key.last().is_some_and(String::is_empty) {
        key.pop();
    }
    Ok(key)
}

pub(super) fn read_specimens<'a>(
    segments: &mut std::iter::Peekable<impl Iterator<Item = Segment<'a>>>,
    context: ReplyContext<'a, '_>,
    index: usize,
) -> Result<Vec<SpecimenResponse<'a>>, Error> {
    let mut specimens = Vec::new();
    while segments.peek().is_some_and(|s| s.id() == b"SPM") {
        let spm = segments.next().ok_or_else(|| invalid("SPM", 0))?;
        specimen(spm, crate::Msh::new(context.message)?.version())?;
        if !specimens.is_empty() {
            return Err(invalid("SPM", 2));
        }
        if let Some(Some(sent)) = context.sent_specimens.get(index)
            && placer_key(spm, context.message)? != placer_key(*sent, context.sent_message)?
        {
            return Err(Error::Correlation {
                segment: "SPM",
                field: 2,
            });
        }
        let mut containers = Vec::new();
        while segments.peek().is_some_and(|s| s.id() == b"SAC") {
            let sac = segments.next().ok_or_else(|| invalid("SAC", 0))?;
            container(sac)?;
            containers.push(sac);
        }
        specimens.push(SpecimenResponse {
            segment: spm,
            containers,
        });
    }
    Ok(specimens)
}
