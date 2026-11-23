//! Reads controls from matching columns in OBR-only quality control uploads.
use super::{components, numeric, record_application, scalar, structure};
use crate::inbound::{grouping, result, source::Source};
use crate::{Error, Input, MappedResult, Output, identifier, list, text};
use analyte_config::{MachineDeparture, Mappings};
use analyte_hl7::{Message, Segment};
use analyte_model::{
    ControlMaterial, EvidenceList, IdentityStatus, NamespacedId, NonEmptyEvidenceList, QcClass,
    Reason, ResultContent, ResultPart, ResultStatus, SourceText,
};

pub(in crate::hl7) fn controls(
    message: &Message,
    source: &Source<'_>,
    mappings: &Mappings,
    input: Input<'_>,
    declaration: &MachineDeparture,
) -> Result<Output, Error> {
    let mut output = Output::default();
    let mode = grouping::processing(source, &mut output, false)?;
    for (index, segment) in message.segments().enumerate().skip(1) {
        if segment.id() != b"OBR" {
            return Err(structure(
                source,
                index,
                0,
                "QC upload must contain only OBR segments after MSH",
            ));
        }
        let position = source.position(index, 20);
        let parts = control_parts(source, index, segment, mappings, input, &mut output)?;
        let corrections = list(vec![false; parts.len()], position)?;
        let parts = NonEmptyEvidenceList::new(list(parts, position)?)
            .map_err(|_| structure(source, index, 20, "QC report has no measurements"))?;
        output.results.push(MappedResult {
            content: ResultContent {
                receipt: input.receipt,
                sample: None,
                processing: mode,
                identity: IdentityStatus::Unresolved(
                    Reason::new("QC upload does not establish a measurement identity")
                        .map_err(|source| Error::Text { position, source })?,
                ),
                parts,
            },
            corrections,
        });
    }
    record_application(message, declaration, &mut output)?;
    Ok(output)
}

fn control_parts(
    source: &Source<'_>,
    index: usize,
    segment: Segment<'_>,
    mappings: &Mappings,
    input: Input<'_>,
    output: &mut Output,
) -> Result<Vec<ResultPart>, Error> {
    let count = scalar(source, index, segment, 11)?;
    let count = count
        .as_str()
        .parse::<usize>()
        .ok()
        .filter(|count| (1..=EvidenceList::<ResultPart>::MAX_ENTRIES).contains(count))
        .ok_or_else(|| structure(source, index, 11, "invalid QC control count"))?;
    // LIS V1.1 sections 2.3.4.2 and 2.4.1.2: columns all describe the same rows.
    // The examples send the channel in both OBR-2 and OBR-3. Require agreement.
    let code = scalar(source, index, segment, 2)?;
    if code != scalar(source, index, segment, 3)? {
        return Err(structure(
            source,
            index,
            3,
            "QC channel fields do not agree",
        ));
    }
    let test = NamespacedId {
        value: identifier(code.as_str(), source.position(index, 2))?,
        namespace: mappings.test_system.clone(),
    };
    let mut columns = control_columns(source, index, segment, count)?;
    let ancestors = source.ancestors(index)?;
    let position = source.position(index, 20);
    let times = list(
        result::source_times(source, &ancestors, position, output)?,
        position,
    )?;
    let path = result::source_path(source, index)?;
    let source_status = text(
        &source.raw(index, 25)?.unwrap_or_default(),
        source.position(index, 25),
    )?;
    let mut parts = Vec::with_capacity(count);
    for _ in 0..count {
        let [
            Some(id),
            Some(name),
            Some(lot),
            Some(expiry),
            Some(level),
            Some(mean),
            Some(standard_deviation),
            Some(measurement),
        ] = columns.each_mut().map(Iterator::next)
        else {
            return Err(structure(
                source,
                index,
                11,
                "QC columns ended before the declared count",
            ));
        };
        if !matches!(level.as_str(), "L" | "M" | "H") {
            return Err(structure(source, index, 17, "undocumented QC level"));
        }
        crate::reserve_evidence(output, source, index)?;
        parts.push(ResultPart {
            test: test.clone(),
            values: list(vec![numeric(measurement, position)?], position)?,
            control: Some(ControlMaterial {
                id,
                name,
                lot,
                expiry,
                level,
                mean,
                standard_deviation,
            }),
            interference: None,
            raw_measurement: None,
            units: None,
            dilution: None,
            flags: EvidenceList::default(),
            reference_range: None,
            comments: EvidenceList::default(),
            qc_class: QcClass::Control,
            source_status: source_status.clone(),
            status: ResultStatus::Unknown,
            source_path: path.clone(),
            times: times.clone(),
            received_at: input.received_at,
        });
    }
    Ok(parts)
}

fn control_columns(
    source: &Source<'_>,
    index: usize,
    segment: Segment<'_>,
    count: usize,
) -> Result<[std::vec::IntoIter<SourceText>; 8], Error> {
    let fields = [12, 13, 14, 15, 17, 18, 19, 20];
    let [
        ids,
        names,
        lots,
        expiry,
        levels,
        means,
        deviations,
        measurements,
    ] = fields.map(|field| components(source, index, segment, field, count));
    let columns = [
        ids?,
        names?,
        lots?,
        expiry?,
        levels?,
        means?,
        deviations?,
        measurements?,
    ]
    .map(Vec::into_iter);
    Ok(columns)
}
