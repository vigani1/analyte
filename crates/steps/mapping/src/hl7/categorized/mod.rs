//! Applies the declared layout to categorized ORU result uploads.
use crate::inbound::{source::Source, value};
use crate::{Error, Output, Position, identifier, list, text};
use analyte_config::MachineDeparture;
use analyte_hl7::{Field, Message, Segment};
use analyte_model::{
    AppliedDeparture, DepartureMeaning, DepartureSpan, ResultValue, SourceText, ValueInterpretation,
};

mod control;
mod patient;

pub(super) use control::controls;
pub(crate) use patient::{PatientValues, patient_part, patient_values};

fn present(field: Option<Field<'_>>) -> bool {
    field.is_some_and(|field| !field.raw().is_empty() && !field.is_null())
}

fn scalar(
    source: &Source<'_>,
    index: usize,
    segment: Segment<'_>,
    field: usize,
) -> Result<SourceText, Error> {
    let [value] = array(
        components(source, index, segment, field, 1)?,
        source.position(index, field),
    )?;
    Ok(value)
}

fn components(
    source: &Source<'_>,
    index: usize,
    segment: Segment<'_>,
    field: usize,
    count: usize,
) -> Result<Vec<SourceText>, Error> {
    let position = source.position(index, field);
    let invalid = || {
        structure(
            source,
            index,
            field,
            "missing or misaligned result components",
        )
    };
    let mut repetitions = segment
        .field(field)
        .filter(|f| present(Some(*f)))
        .ok_or_else(invalid)?
        .repetitions();
    let repetition = repetitions.next().ok_or_else(invalid)?;
    if repetitions.next().is_some() {
        return Err(invalid());
    }
    let mut parts = Vec::with_capacity(count);
    for component in repetition.components() {
        if parts.len() == count
            || matches!(component.raw(), b"" | b"\"\"")
            || component.subcomponents().nth(1).is_some()
        {
            return Err(invalid());
        }
        let original = source.original_text(component.raw(), index, field)?;
        if source.text_problem(&original).is_some() {
            return Err(structure(
                source,
                index,
                field,
                "result component uses unsupported text escapes",
            ));
        }
        parts.push(text(&original, position)?);
    }
    if parts.len() != count {
        return Err(invalid());
    }
    Ok(parts)
}

fn array<const N: usize>(
    parts: Vec<SourceText>,
    position: Position,
) -> Result<[SourceText; N], Error> {
    parts.try_into().map_err(|_| Error::Structure {
        position,
        reason: "unexpected component count",
    })
}

fn numeric(original: SourceText, position: Position) -> Result<ResultValue, Error> {
    let parsed = value::decimal(original.as_str(), true);
    if !matches!(parsed, ValueInterpretation::Numeric(_)) {
        return Err(Error::Structure {
            position,
            reason: "documented numeric result cannot be interpreted",
        });
    }
    Ok(ResultValue::new(original, parsed))
}

fn structure(source: &Source<'_>, index: usize, field: usize, reason: &'static str) -> Error {
    Error::Structure {
        position: source.position(index, field),
        reason,
    }
}

pub(super) fn record_application(
    message: &Message,
    declaration: &MachineDeparture,
    output: &mut Output,
) -> Result<(), Error> {
    let position = Position {
        record: 1,
        field: 16,
    };
    let span = DepartureSpan::try_from(0..message.raw().len())
        .map_err(|reason| Error::Structure { position, reason })?;
    output.applied_departures.push(AppliedDeparture {
        rule: identifier(declaration.tag(), position)?,
        declaration: declaration.evidence_snapshot()?,
        standard: identifier("HL7 2.3.1 section 7.3.2", position)?,
        record: 1,
        field: 16,
        source: text("categorized ORU result layout; source unchanged", position)?,
        meaning: DepartureMeaning::WireCompatibility,
        coordinates: identifier("syntax_bytes", position)?,
        spans: list(vec![span], position)?,
    });
    Ok(())
}
