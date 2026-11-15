//! Conservative vendor adapters retain unresolved identities and exact values.
use crate::{MappedResult, Output, Position};
use analyte_config::Validated;
use analyte_model::*;

/// Specific vendor-to-model boundary failures; original receipts must remain protected.
#[derive(Debug, thiserror::Error)]
pub enum VendorMappingError {
    /// Common-model text refused the source.
    #[error(transparent)]
    Text(#[from] TextError),
    /// Bounded evidence list refused the source.
    #[error(transparent)]
    List(#[from] ListError),
    /// Original unit mapping validation failure.
    #[error(transparent)]
    Unit(#[from] UnitMappingError),
    /// A result cannot contain an empty part list.
    #[error(transparent)]
    Empty(#[from] EmptyEvidence),
    /// Source bytes are not valid UTF-8.
    #[error(transparent)]
    Encoding(#[from] std::str::Utf8Error),
    /// VIDAS result syntax or model text refused the source.
    #[error(transparent)]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// POCT observation mapping refused the source.
    #[error(transparent)]
    Poct(#[from] crate::poct::MappingError),
    /// Shared identity or mapping rule refused the source.
    #[error(transparent)]
    Mapping(#[from] crate::Error),
    /// A specifically named required context is unavailable.
    #[error("{0}")]
    Context(&'static str),
}
use VendorMappingError as Error;

fn part(
    test: NamespacedId,
    value: ResultValue,
    received_at: jiff::Timestamp,
) -> Result<ResultPart, Error> {
    Ok(ResultPart {
        test,
        values: EvidenceList::new(vec![value])?,
        units: None,
        dilution: None,
        flags: EvidenceList::new(Vec::new())?,
        reference_range: None,
        comments: EvidenceList::new(Vec::new())?,
        qc_class: QcClass::Unknown,
        source_status: SourceText::new("")?,
        status: ResultStatus::Unknown,
        source_path: EvidenceList::new(Vec::new())?,
        times: EvidenceList::new(Vec::new())?,
        received_at,
    })
}

fn translate_units(machine: &Validated, part: &mut ResultPart) -> Result<(), Error> {
    let (_, units) = machine.effective().stack().code_tables();
    if let Some(unit) = &part.units {
        let original = unit.original_text().clone();
        if let Some(entry) = crate::unit_translation(units, original.as_str()) {
            part.units = Some(Units::mapped(
                original.clone(),
                UnitMapping {
                    source: original,
                    target: entry.ucum.clone(),
                },
            )?);
        }
    }
    Ok(())
}

fn result(
    receipt: ReceiptId,
    sample: Option<SampleIdentity>,
    part: ResultPart,
    identity: IdentityStatus,
) -> Result<ResultContent, Error> {
    Ok(ResultContent {
        receipt,
        sample,
        processing: ProcessingMode::Unknown,
        identity,
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![part])?)?,
    })
}

fn translated_test(machine: &Validated, code: &str, system: Option<&str>) -> Option<NamespacedId> {
    crate::test_translation(machine.effective().stack().code_tables().0, &[code], system).map(
        |entry| NamespacedId {
            namespace: entry.app_system.clone(),
            value: entry.app_code.clone(),
        },
    )
}

fn push_result(output: &mut Output, content: ResultContent) -> Result<(), Error> {
    output.results.push(MappedResult {
        content,
        corrections: EvidenceList::new(vec![false])?,
    });
    Ok(())
}

/// Maps VIDAS values conservatively, preserving unresolved identity and exact text.
/// # Errors
/// Preserves syntax, model and mapping failures.
pub fn map_vidas(
    machine: &Validated,
    receipt: ReceiptId,
    message: &analyte_vidas::syntax::Message,
    time: jiff::Timestamp,
) -> Result<Output, Error> {
    let evidence = vidas_evidence(message)?;
    let mut output = Output::default();
    let Some(test) = translated_test(machine, evidence.assay.as_str(), None) else {
        output.unmapped_test(
            Position {
                record: 1,
                field: 0,
            },
            EvidenceList::new(vec![evidence.value])?,
        )?;
        return Ok(output);
    };
    let mut source = part(test, evidence.value, time)?;
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
    translate_units(machine, &mut source)?;
    push_result(
        &mut output,
        result(
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

/// Maps Autolyser results without inferring a specimen or immutable run.
/// # Errors
/// Preserves source encoding, model and mapping failures.
pub fn map_autolyser(
    machine: &Validated,
    receipt: ReceiptId,
    report: &analyte_autolyser::records::Report,
    time: jiff::Timestamp,
) -> Result<Output, Error> {
    let mut output = Output::default();
    for (index, value) in report.values.iter().enumerate() {
        let code = std::str::from_utf8(value.code.bytes())?;
        let value = ResultValue {
            decoded_text: None,
            original: SourceText::new(std::str::from_utf8(value.text.bytes())?)?,
            parsed: ValueInterpretation::Text,
        };
        let Some(test) = translated_test(machine, code, None) else {
            output.unmapped_test(
                Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                EvidenceList::new(vec![value])?,
            )?;
            continue;
        };
        let mut source = part(test, value, time)?;
        if report.control.as_ref().is_some_and(|c| c.is_control) {
            source.qc_class = QcClass::Control;
        }
        translate_units(machine, &mut source)?;
        push_result(
            &mut output,
            result(
                receipt,
                None,
                source,
                IdentityStatus::Unresolved(Reason::new(
                    "Autolyser Rev18 has no immutable run identity; patient code is not trusted specimen identity",
                )?),
            )?,
        )?;
    }
    if report.values.is_empty() {
        return Err(Error::Context("Autolyser report contains no results"));
    }
    Ok(output)
}

/// Applies explicit POCT code tables and identity policy to faithfully mapped observations.
/// # Errors
/// Preserves syntax, model, identity and mapping failures.
pub fn map_poct(
    machine: &Validated,
    receipt: ReceiptId,
    message: &analyte_poct::Message,
    time: jiff::Timestamp,
    device: Option<&str>,
) -> Result<Output, Error> {
    let observations = crate::poct::map_observations(message, time)?;
    if observations.is_empty() {
        return Err(Error::Context(
            "POCT observations transfer contains no observations",
        ));
    }
    let mut output = Output::default();
    for observation in observations {
        let mut part = observation.part;
        let mut identity = observation.identity;
        let mut processing = ProcessingMode::Unknown;
        let mut correction = false;
        if let analyte_config::Stack::Poct(settings) = machine.effective().stack()
            && let Some(policy) = &settings.policy.identity
        {
            processing = policy.processing;
            let service_index = part
                .source_path
                .as_slice()
                .iter()
                .find(|node| node.kind.as_str() == "SVC")
                .map(|node| node.occurrence.saturating_sub(1));
            let field = |name: &str| -> Option<&str> {
                message
                    .document()
                    .elements()
                    .iter()
                    .find(|e| e.parent() == service_index && e.name() == name)
                    .and_then(|e| {
                        e.attributes()
                            .iter()
                            .find(|(k, _)| k == "V")
                            .map(|(_, v)| v.as_str())
                    })
            };
            correction = field("SVC.reason_cd") == Some("EDT");
            let observed = field("SVC.observation_dttm").filter(|v| !v.is_empty());
            let sequence = field("SVC.sequence_nbr").filter(|v| !v.is_empty());
            if let (Some(device), Some(observed)) = (device.filter(|v| !v.is_empty()), observed)
                && (!policy.sequence_number || sequence.is_some())
            {
                let mut run = vec![observed];
                if policy.sequence_number
                    && let Some(sequence) = sequence
                {
                    run.push(sequence);
                }
                let code = NamespacedId {
                    namespace: Identifier::new(
                        observation
                            .source_code
                            .system
                            .as_deref()
                            .or(observation.source_code.system_name.as_deref())
                            .ok_or(Error::Context("POCT analyte namespace missing"))?,
                    )?,
                    value: Identifier::new(&observation.source_code.value)?,
                };
                identity = IdentityStatus::Resolved(ResultIdentity {
                    observation: ObservationIdentity {
                        code,
                        sub_id: SourceText::new("")?,
                        run: crate::identity_run(&run)?,
                        order: NamespacedId {
                            namespace: Identifier::new("poct-device")?,
                            value: Identifier::new(device)?,
                        },
                    },
                    revision: Revision(0),
                });
            } else {
                identity = IdentityStatus::Unresolved(Reason::new(
                    "POCT configured device/time/sequence identity evidence missing",
                )?);
            }
        }
        let system = observation
            .source_code
            .system
            .as_deref()
            .or(observation.source_code.system_name.as_deref());
        let Some(test) = translated_test(machine, &observation.source_code.value, system) else {
            output.unmapped_test(
                Position {
                    record: observation.source_index.saturating_add(1),
                    field: 0,
                },
                part.values,
            )?;
            continue;
        };
        part.test = test;
        translate_units(machine, &mut part)?;
        let mut content = result(receipt, observation.sample, part, identity)?;
        content.processing = processing;
        output.results.push(MappedResult {
            content,
            corrections: EvidenceList::new(vec![correction])?,
        });
    }
    Ok(output)
}

/// Extracts VIDAS common-model evidence without parsing or changing written values.
/// # Errors
/// Rejects non-result messages or common-model text violations.
pub fn vidas_evidence(
    message: &analyte_vidas::syntax::Message,
) -> Result<VidasResultEvidence, analyte_vidas::syntax::Error> {
    if message.kind() != analyte_vidas::syntax::Kind::Result {
        return Err(analyte_vidas::syntax::Error::NotResult);
    }
    Ok(VidasResultEvidence {
        sample: message.sample().clone(),
        assay: SourceText::new(
            message
                .get("rt")
                .ok_or_else(|| analyte_vidas::syntax::Error::Field("rt".into()))?,
        )?,
        value: ResultValue {
            decoded_text: None,
            original: SourceText::new(
                message
                    .get("qn")
                    .ok_or_else(|| analyte_vidas::syntax::Error::Field("qn".into()))?,
            )?,
            parsed: ValueInterpretation::Text,
        },
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
pub struct VidasResultEvidence {
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
