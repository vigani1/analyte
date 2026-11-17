//! Maps VIDAS codes, original values and outgoing work.
use crate::inbound::evidence::{Error, map_part, map_result, map_test, map_units, push_result};
use crate::orders::codes::{map_priority as outbound_priority, map_test as outbound_test};
use crate::orders::{AppOrder, MachineOrder, OrderMappingError, map_patient};
use crate::{Output, Position};

use analyte_model::{
    Dilution, DilutionApplied, EvidenceList, IdentityStatus, Reason, ReceiptId, ResultValue,
    SampleId, SampleIdentity, SampleSource, SourceText, Units, ValueInterpretation,
};
/// Maps VIDAS values conservatively, preserving unresolved identity and exact text.
/// # Errors
/// Preserves syntax, model and mapping failures.
pub fn map(
    machine: &analyte_config::VidasStack,
    receipt: ReceiptId,
    message: &analyte_vidas::syntax::Message,
    time: jiff::Timestamp,
) -> Result<Output, Error> {
    let evidence = map_evidence(message)?;
    let mut output = Output::default();
    let Some(test) = map_test(machine.codes(), evidence.assay.as_str(), None) else {
        output.unmapped_test(
            Position {
                record: 1,
                field: 0,
            },
            EvidenceList::new(vec![evidence.value])?,
        )?;
        return Ok(output);
    };
    let mut source = map_part(test, evidence.value, time)?;
    source.units = evidence.unit.map(Units::original);
    source.flags = EvidenceList::new(evidence.flag.into_iter().collect())?;
    source.comments = EvidenceList::new(vec![evidence.call])?;
    if let Some(original) = evidence.dilution {
        source.dilution = Some(Dilution {
            original: Some(original),
            factor: None,
            applied: DilutionApplied::Unknown,
        });
    }
    map_units(machine.codes(), &mut source)?;
    push_result(
        &mut output,
        map_result(
            receipt,
            Some(SampleIdentity {
                id: evidence.sample,
                source: SampleSource::Unknown,
                position: None,
            }),
            source,
            IdentityStatus::Unresolved(Reason::new(
                "VIDAS Literal lacks trusted result/run identity and patient versus QC classification",
            )?),
        )?,
    )?;
    Ok(output)
}

/// Extracts VIDAS common-model evidence without parsing or changing written values.
/// # Errors
/// Rejects non-result messages or common-model text violations.
pub fn map_evidence(
    message: &analyte_vidas::syntax::Message,
) -> Result<ResultEvidence, analyte_vidas::syntax::Error> {
    if message.kind() != analyte_vidas::syntax::Kind::Result {
        return Err(analyte_vidas::syntax::Error::NotResult);
    }
    Ok(ResultEvidence {
        sample: message.sample().clone(),
        assay: SourceText::new(
            message
                .get("rt")
                .ok_or_else(|| analyte_vidas::syntax::Error::Field("rt".into()))?,
        )?,
        value: ResultValue::new(
            SourceText::new(
                message
                    .get("qn")
                    .ok_or_else(|| analyte_vidas::syntax::Error::Field("qn".into()))?,
            )?,
            ValueInterpretation::Text,
        ),
        call: SourceText::new(
            message
                .get("ql")
                .ok_or_else(|| analyte_vidas::syntax::Error::Field("ql".into()))?,
        )?,
        unit: message.get("y3").map(SourceText::new).transpose()?,
        flag: message.get("nc").map(SourceText::new).transpose()?,
        dilution: message.get("qd").map(SourceText::new).transpose()?,
    })
}

/// Uninterpreted result evidence, not a deliverable identified observation.
/// No physical position exists in the documented schema. The host must resolve
/// result/revision identity and patient versus QC classification before delivery.
#[derive(Debug, Clone)]
pub struct ResultEvidence {
    /// Exact source sample identifier.
    pub sample: SampleId,
    /// Unmapped assay code; no assay name guessing.
    pub assay: SourceText,
    /// Original measured text, including comparator, error token or embedded unit.
    pub value: ResultValue,
    /// Original qualitative call in the installed language.
    pub call: SourceText,
    /// Separately transmitted unit, if any; no inferred conversion.
    pub unit: Option<SourceText>,
    /// Calibration/control/lot flag; never a clinical judgement.
    pub flag: Option<SourceText>,
    /// Original ratio or factor; application status is unknown.
    pub dilution: Option<SourceText>,
}
/// Maps one explicitly authorized VIDAS request per configured assay.
/// # Errors
/// Preserves missing context, code mapping and wire-size refusals.
pub fn map_order(
    order: &AppOrder,
    machine: &analyte_config::VidasStack,
    settings: &analyte_config::VidasOrders,
) -> Result<MachineOrder, OrderMappingError> {
    let installed = &machine.policy();
    if !installed.download {
        return Err(OrderMappingError::Disabled);
    }
    let patient = map_patient(order, settings.patient_namespace())?;
    if patient.len() > 17 {
        return Err(OrderMappingError::PatientIdTooLong);
    }
    if order.sample.as_str().len() > usize::from(u8::from(installed.sample_limit)) {
        return Err(OrderMappingError::SampleIdTooLong);
    }
    // BCI A section 4 has no specimen-type destination; so is origin and si is empty.
    if order.specimen_type.is_some() {
        return Err(OrderMappingError::UnmappedSpecimenType);
    }
    if order.tests.is_empty()
        || order.tests.len() > 100
        || order.tests.len() > machine.limits().records()
        || order.tests.len() > machine.limits().repetitions()
    {
        return Err(OrderMappingError::VidasTestCount);
    }
    outbound_priority(settings.priorities(), order.priority)?;
    let limit = match installed.sample_limit {
        analyte_config::VidasSampleLimit::Twelve => analyte_vidas::syntax::SampleLimit::Twelve,
        analyte_config::VidasSampleLimit::Seventeen => {
            analyte_vidas::syntax::SampleLimit::Seventeen
        }
    };
    let mut requests = Vec::new();
    let mut total_bytes = 0usize;
    for test in &order.tests {
        let mapped = outbound_test(settings.tests(), test)?;
        let [code] = mapped.machine_components.as_slice() else {
            return Err(OrderMappingError::VidasAssayComponents);
        };
        let request = analyte_vidas::syntax::Message::new(
            &[
                ("mt", "mpr"),
                ("pi", patient),
                ("si", ""),
                ("ci", order.sample.as_str()),
                ("rt", code),
            ],
            limit,
        )?;
        if request.text().len() > machine.frame_limit() {
            return Err(OrderMappingError::VidasFrameLimit);
        }
        total_bytes = total_bytes
            .checked_add(request.text().len())
            .ok_or(OrderMappingError::VidasByteCountOverflow)?;
        if total_bytes > machine.limits().bytes() {
            return Err(OrderMappingError::VidasMessageLimit);
        }
        requests.push(request);
    }
    Ok(MachineOrder::Vidas(requests))
}
