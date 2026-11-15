//! Conservative vendor adapters retain unresolved identities and exact values.
use crate::{Error, config::Machine};
use analyte_model::*;
use analyte_store::Reading;

fn part(code: &str, value: ResultValue, received_at: jiff::Timestamp) -> Result<ResultPart, Error> {
    Ok(ResultPart {
        test: NamespacedId {
            namespace: Identifier::new("vendor-source")?,
            value: Identifier::new(code)?,
        },
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

fn translate(machine: &Machine, part: &mut ResultPart) -> Result<(), Error> {
    let settings = machine.snapshot.effective().machine();
    if let Some(entry) =
        analyte_mapping::test_translation(settings.tests(), &[part.test.value.as_str()], None)
    {
        part.test = NamespacedId {
            namespace: entry.app_system.clone(),
            value: entry.app_code.clone(),
        };
    }
    if let Some(unit) = &part.units {
        let original = unit.original_text().clone();
        if let Some(entry) = analyte_mapping::unit_translation(settings.units(), original.as_str())
        {
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
        processing: ProcessingMode::Production,
        identity,
        parts: NonEmptyEvidenceList::new(EvidenceList::new(vec![part])?)?,
    })
}

pub(super) fn vidas(
    machine: &Machine,
    receipt: ReceiptId,
    message: &analyte_vidas::syntax::Message,
    time: jiff::Timestamp,
) -> Result<Reading, Error> {
    let evidence = message.result()?;
    let mut source = part(evidence.assay.as_str(), evidence.value, time)?;
    source.units = evidence.unit.map(Units::original);
    source.flags = EvidenceList::new(evidence.flag.into_iter().collect())?;
    source.comments = EvidenceList::new(vec![evidence.call])?;
    if evidence.dilution.is_some() {
        source.dilution = Some(Dilution {
            factor: None,
            applied: DilutionApplied::Unknown,
        });
    }
    translate(machine, &mut source)?;
    Ok(Reading::Results(vec![result(
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
    )?]))
}

pub(super) fn autolyser(
    machine: &Machine,
    receipt: ReceiptId,
    report: &analyte_autolyser::records::Report,
    time: jiff::Timestamp,
) -> Result<Reading, Error> {
    let mut results = Vec::new();
    for value in &report.values {
        let code = std::str::from_utf8(value.code.bytes())?;
        let text = std::str::from_utf8(value.text.bytes())?;
        let mut source = part(
            code,
            ResultValue {
                original: SourceText::new(text)?,
                parsed: ValueInterpretation::Text,
            },
            time,
        )?;
        if report.control.as_ref().is_some_and(|c| c.is_control) {
            source.qc_class = QcClass::Control;
        }
        translate(machine, &mut source)?;
        results.push(result(receipt, None, source, IdentityStatus::Unresolved(Reason::new("Autolyser Rev18 has no immutable run identity; patient code is not trusted specimen identity")?))?);
    }
    if results.is_empty() {
        return Ok(Reading::Failed(Reason::new(
            "Autolyser report contains no results",
        )?));
    }
    Ok(Reading::Results(results))
}

pub(super) fn poct(
    machine: &Machine,
    receipt: ReceiptId,
    message: &analyte_poct::Message,
    time: jiff::Timestamp,
) -> Result<Reading, Error> {
    let mut results = Vec::new();
    for observation in analyte_poct::map_observations(message, time)? {
        let mut part = observation.part;
        translate(machine, &mut part)?;
        // The protocol crate's identity decision is authoritative, including Unresolved.
        results.push(result(
            receipt,
            observation.sample,
            part,
            observation.identity,
        )?);
    }
    if results.is_empty() {
        return Ok(Reading::Failed(Reason::new(
            "POCT observations transfer contains no observations",
        )?));
    }
    Ok(Reading::Results(results))
}
