//! Adapts vendor evidence while retaining unresolved identities and exact values.
use crate::{MappedResult, Output};

use analyte_model::{
    EmptyEvidence, EvidenceList, IdentityStatus, ListError, NamespacedId, NonEmptyEvidenceList,
    ProcessingMode, QcClass, ReceiptId, ResultContent, ResultPart, ResultStatus, ResultValue,
    SampleIdentity, SourceText, TextError, UnitMapping, UnitMappingError, Units,
};

/// Specific vendor-to-model boundary failures; original receipts must remain protected.
#[derive(Debug, thiserror::Error)]
pub enum ResultMappingError {
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
pub(crate) use ResultMappingError as Error;

pub(crate) fn map_part(
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

pub(crate) fn map_units(
    machine: &analyte_config::CodeTables,
    part: &mut ResultPart,
) -> Result<(), Error> {
    let units = &machine.units;
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

pub(crate) fn map_result(
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

/// The machine's own code exactly as sent, in the code system the message
/// names or else the machine's declared one (ADR 0029). An empty code is
/// `None`: there is no test to name, and none is invented.
pub(crate) fn map_test(
    machine: &analyte_config::CodeTables,
    code: &str,
    system: Option<&str>,
) -> Result<Option<NamespacedId>, Error> {
    if code.is_empty() {
        return Ok(None);
    }
    Ok(Some(NamespacedId {
        namespace: match system.filter(|s| !s.is_empty()) {
            Some(system) => analyte_model::Identifier::new(system)?,
            None => machine.test_system.clone(),
        },
        value: analyte_model::Identifier::new(code)?,
    }))
}

pub(crate) fn push_result(output: &mut Output, content: ResultContent) -> Result<(), Error> {
    output.results.push(MappedResult {
        content,
        corrections: EvidenceList::new(vec![false])?,
    });
    Ok(())
}
