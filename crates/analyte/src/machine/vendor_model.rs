//! Conservative vendor adapters retain unresolved identities and exact values.
use crate::{Error, config::Machine};
use analyte_mapping::{MappedResult, Output, Position};
use analyte_model::*;

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

fn translate_units(machine: &Machine, part: &mut ResultPart) -> Result<(), Error> {
    let (_, units) = machine.snapshot.effective().stack().code_tables();
    if let Some(unit) = &part.units {
        let original = unit.original_text().clone();
        if let Some(entry) = analyte_mapping::unit_translation(units, original.as_str()) {
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

fn translated_test(machine: &Machine, code: &str, system: Option<&str>) -> Option<NamespacedId> {
    analyte_mapping::test_translation(
        machine.snapshot.effective().stack().code_tables().0,
        &[code],
        system,
    )
    .map(|entry| NamespacedId {
        namespace: entry.app_system.clone(),
        value: entry.app_code.clone(),
    })
}

fn push_result(output: &mut Output, content: ResultContent) -> Result<(), Error> {
    output.results.push(MappedResult {
        content,
        corrections: EvidenceList::new(vec![false])?,
    });
    Ok(())
}

pub(super) fn vidas(
    machine: &Machine,
    receipt: ReceiptId,
    message: &analyte_vidas::syntax::Message,
    time: jiff::Timestamp,
) -> Result<Output, Error> {
    let evidence = message.result()?;
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

pub(super) fn autolyser(
    machine: &Machine,
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
        return Err(crate::invalid("Autolyser report contains no results"));
    }
    Ok(output)
}

pub(super) fn poct(
    machine: &Machine,
    receipt: ReceiptId,
    message: &analyte_poct::Message,
    time: jiff::Timestamp,
    device: Option<&str>,
) -> Result<Output, Error> {
    let observations = analyte_poct::map_observations(message, time)?;
    if observations.is_empty() {
        return Err(crate::invalid(
            "POCT observations transfer contains no observations",
        ));
    }
    let mut output = Output::default();
    for observation in observations {
        let mut part = observation.part;
        let mut identity = observation.identity;
        let mut processing = ProcessingMode::Unknown;
        let mut correction = false;
        if let analyte_config::Stack::Poct(settings) = machine.snapshot.effective().stack()
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
                            .ok_or_else(|| crate::invalid("POCT analyte namespace missing"))?,
                    )?,
                    value: Identifier::new(&observation.source_code.value)?,
                };
                identity = IdentityStatus::Resolved(ResultIdentity {
                    observation: ObservationIdentity {
                        code,
                        sub_id: SourceText::new("")?,
                        run: analyte_mapping::identity_run(&run)?,
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
