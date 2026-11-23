//! Maps patient values, interference panels and physical specimen evidence.
use super::{array, components, numeric, present, scalar, structure};
use crate::inbound::{
    source::{Record, Source},
    value,
};
use crate::{Error, Output, Position, Problem, finding, identifier, list, text};
use analyte_hl7::{Field, Segment};
use analyte_model::{
    CarrierKind, Decimal, Dilution, DilutionApplied, EvidenceList, InterferenceComponent, QcClass,
    ResultPart, ResultStatus, ResultValue, SampleIdentity, SamplePosition, SourceText,
};

pub(crate) enum PatientValues {
    Combined(EvidenceList<ResultValue>),
    Panel(Box<[ResultValue; 3]>, [SourceText; 3]),
}

impl PatientValues {
    pub(crate) fn values(&self, position: Position) -> Result<EvidenceList<ResultValue>, Error> {
        match self {
            Self::Combined(values) => Ok(values.clone()),
            Self::Panel(values, _) => list(values.to_vec(), position),
        }
    }
}

pub(crate) fn patient_values(
    source: &Source<'_>,
    index: usize,
    output: &mut Output,
) -> Result<Option<PatientValues>, Error> {
    let Some(Record::Hl7(segment)) = source.nodes.get(index).map(|node| node.record) else {
        return Ok(None);
    };
    let datatype = segment.field(2).map_or(&[][..], Field::raw);
    let primary = segment.field(5);
    let raw = segment.field(13);
    let composite = |field: Option<Field<'_>>| {
        field.is_some_and(|field| field.repetitions().any(|r| r.components().nth(1).is_some()))
    };
    // LIS V1.1 section 2.3.5 defines the two arrays in the same component order.
    if composite(raw) || (matches!(datatype, b"NM" | b"BOTH" | b"ST") && composite(primary)) {
        if datatype != b"NM" || present(segment.field(9)) || present(segment.field(10)) {
            return Err(structure(
                source,
                index,
                5,
                "interference panel has conflicting value forms",
            ));
        }
        for field in [6, 7, 8] {
            if present(segment.field(field)) {
                return Err(structure(
                    source,
                    index,
                    field,
                    "interference panel does not establish component units, ranges or flags",
                ));
            }
        }
        let values = components(source, index, segment, 5, 3)?;
        let originals = components(source, index, segment, 13, 3)?;
        let [first, second, third] = array(values, source.position(index, 5))?;
        return Ok(Some(PatientValues::Panel(
            Box::new([
                numeric(first, source.position(index, 5))?,
                numeric(second, source.position(index, 5))?,
                numeric(third, source.position(index, 5))?,
            ]),
            array(originals, source.position(index, 13))?,
        )));
    }

    if datatype != b"BOTH" && !(datatype == b"ST" && present(segment.field(9))) {
        if present(segment.field(9)) || present(segment.field(10)) {
            return Err(structure(
                source,
                index,
                9,
                "qualitative fields conflict with the declared value type",
            ));
        }
        return Ok(None);
    }

    let position = source.position(index, 5);
    let mut values = if datatype == b"BOTH" && present(primary) {
        vec![numeric(scalar(source, index, segment, 5)?, position)?]
    } else if present(primary) {
        value::values(source, index, 5, output)?.as_slice().to_vec()
    } else {
        Vec::new()
    };
    let original = scalar(source, index, segment, 9)?;
    let decoded = source
        .decoded_repetition(index, 9, 0)?
        .map(|value| text(&value, source.position(index, 9)))
        .transpose()?;
    let mut qualitative = ResultValue::text(original, decoded);
    if present(segment.field(10)) {
        qualitative.reference = Some(scalar(source, index, segment, 10)?);
    }
    values.push(qualitative);
    Ok(Some(PatientValues::Combined(list(values, position)?)))
}

pub(crate) fn patient_part(
    source: &Source<'_>,
    index: usize,
    part: &mut ResultPart,
    sample: Option<&mut SampleIdentity>,
    readings: Option<PatientValues>,
    output: &mut Output,
) -> Result<Option<[ResultPart; 2]>, Error> {
    if !matches!(part.qc_class, QcClass::Patient | QcClass::Unknown) {
        finding(
            output,
            source.position(0, 16),
            Problem::ConflictingClassification,
        )?;
    }
    part.qc_class = QcClass::Patient;
    // The upload setting can trigger incomplete reports. Only F is documented.
    if part.source_status.as_str() != "F" {
        part.status = ResultStatus::Unknown;
        finding(
            output,
            source.position(index, 11),
            Problem::UnqualifiedHl7Report,
        )?;
    }
    for order in source.ancestors(index)?.into_iter().rev() {
        let Some(node) = source.nodes.get(order).filter(|node| node.name == "OBR") else {
            continue;
        };
        if let Record::Hl7(segment) = node.record {
            patient_order(source, order, segment, part, sample, output)?;
        }
        break;
    }
    if let Some(PatientValues::Panel(values, originals)) = readings {
        crate::reserve_evidence(output, source, index)?;
        crate::reserve_evidence(output, source, index)?;
        let [first, second, third] = *values;
        let [first_raw, second_raw, third_raw] = originals;
        let position = source.position(index, 5);
        let mut hemolysis = part.clone();
        let mut jaundice = part.clone();
        for (part, component, value, raw) in [
            (part, InterferenceComponent::Turbidity, first, first_raw),
            (
                &mut hemolysis,
                InterferenceComponent::Hemolysis,
                second,
                second_raw,
            ),
            (
                &mut jaundice,
                InterferenceComponent::Jaundice,
                third,
                third_raw,
            ),
        ] {
            part.interference = Some(component);
            part.values = list(vec![value], position)?;
            part.raw_measurement = Some(raw);
        }
        return Ok(Some([hemolysis, jaundice]));
    }
    part.raw_measurement = source
        .present(source.raw(index, 13)?)
        .map(|value| text(&value, source.position(index, 13)))
        .transpose()?;
    Ok(None)
}

fn patient_order(
    source: &Source<'_>,
    order: usize,
    segment: Segment<'_>,
    part: &mut ResultPart,
    sample: Option<&mut SampleIdentity>,
    output: &mut Output,
) -> Result<(), Error> {
    if segment
        .field(25)
        .is_some_and(|status| !status.raw().is_empty() && status.raw() != b"F")
    {
        finding(
            output,
            source.position(order, 25),
            Problem::UnqualifiedHl7Report,
        )?;
    }
    // LIS V1.1 section 2.3.4.1 names a dilution factor and a rack/slot pair.
    // It does not say whether the factor has already been applied to the result.
    if present(segment.field(9)) {
        let original = scalar(source, order, segment, 9)?;
        part.dilution = Some(Dilution {
            factor: Decimal::new(original.as_str()).ok(),
            original: Some(original),
            applied: DilutionApplied::Unknown,
        });
    }
    if let Some(sample) = sample.filter(|_| present(segment.field(10))) {
        let position = source.position(order, 10);
        let [rack, slot] = array(components(source, order, segment, 10, 2)?, position)?;
        sample.position = Some(SamplePosition {
            kind: CarrierKind::Rack,
            carrier: Some(identifier(rack.as_str(), position)?),
            slot: identifier(slot.as_str(), position)?,
        });
    }
    Ok(())
}
