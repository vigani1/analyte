//! Maps Autolyser results and outgoing work lists.
use crate::inbound::evidence::{Error, map_part, map_result, map_test, map_units, push_result};
use crate::orders::codes::{self, map_priority};
use crate::orders::{AppOrder, map_patient, map_specimen};
use crate::{Output, Position};

use analyte_model::{
    EvidenceList, IdentityStatus, QcClass, Reason, ResultValue, SourceText, ValueInterpretation,
};
/// Maps Autolyser results without inferring a specimen or immutable run.
/// # Errors
/// Preserves source encoding, model and mapping failures.
pub fn map(
    machine: &analyte_config::AutolyserStack,
    report: &analyte_autolyser::syntax::Report,
    input: crate::Input<'_>,
) -> Result<Output, Error> {
    let receipt = input.receipt;
    let time = input.received_at;
    let mut output = Output::default();
    for (index, value) in report.values.iter().enumerate() {
        let code = std::str::from_utf8(value.code.bytes())?;
        let value = ResultValue::new(
            SourceText::new(std::str::from_utf8(value.text.bytes())?)?,
            ValueInterpretation::Text,
        );
        let Some(test) = map_test(machine.settings().codes(), code, None) else {
            output.unmapped_test(
                Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                EvidenceList::new(vec![value])?,
            )?;
            continue;
        };
        let mut source = map_part(test, value, time)?;
        if report.control.as_ref().is_some_and(|c| c.is_control) {
            source.qc_class = QcClass::Control;
        }
        map_units(machine.settings().codes(), &mut source)?;
        push_result(
            &mut output,
            map_result(
                receipt,
                None,
                source,
                IdentityStatus::Unresolved(Reason::new(
                    &crate::Unidentified::AutolyserRun.to_string(),
                )?),
            )?,
        )?;
    }
    if report.values.is_empty() {
        return Err(Error::Context("Autolyser report contains no results"));
    }
    Ok(output)
}

/// Maps explicit app work to Autolyser codes, without inferred medium or cup.
/// # Errors
/// Preserves missing context, code mapping and fixed-width refusals.
pub fn map_order(
    order: &AppOrder,
    machine: &analyte_config::AutolyserStack,
    settings: &analyte_config::AutolyserOrders,
) -> Result<analyte_autolyser::syntax::Worklist, OrderError> {
    use analyte_autolyser::syntax::{Field, List, PatientCode, Sample, Worklist};
    if !machine.policy().download {
        return Err(crate::OrderMappingError::Disabled.into());
    }
    let patient = map_patient(order, settings.patient_namespace())?;
    let clone = settings.clone_enabled();
    let settings = &settings.tables();
    if patient.len() > 15 {
        return Err(crate::OrderMappingError::PatientIdTooLong.into());
    }
    let mut padded = [b' '; 15];
    padded
        .get_mut(..patient.len())
        .ok_or(crate::OrderMappingError::PatientIdTooLong)?
        .copy_from_slice(patient.as_bytes());
    let patient = PatientCode::new(&padded)?;
    let sample = match map_specimen(order, settings)? {
        "S" => Sample::Serum,
        "U" => Sample::Urine,
        _ => return Err(crate::OrderMappingError::UnmappedSpecimenType.into()),
    };
    let priority = map_priority(settings.priorities(), order.priority)?;
    let list = match &priority.machine {
        analyte_config::MachinePriority::Code(analyte_config::PriorityCode::T) => List::Routine,
        analyte_config::MachinePriority::Code(analyte_config::PriorityCode::R) => List::Stat,
        analyte_config::MachinePriority::Code(
            analyte_config::PriorityCode::S
            | analyte_config::PriorityCode::A
            | analyte_config::PriorityCode::P
            | analyte_config::PriorityCode::C
            | analyte_config::PriorityCode::Prn,
        )
        | analyte_config::MachinePriority::Custom(_) => {
            return Err(crate::OrderMappingError::UnmappedPriority.into());
        }
    };
    let position = order
        .position
        .as_ref()
        .ok_or(OrderError::MissingCupPosition)?;
    let slot = position.slot.as_str();
    if slot.is_empty() || slot.len() > 2 || !slot.bytes().all(|b| b.is_ascii_digit()) {
        return Err(OrderError::InvalidCupPosition);
    }
    let position = slot
        .parse::<u8>()
        .map_err(|_| OrderError::InvalidCupPosition)?;
    if !(1..=99).contains(&position) {
        return Err(OrderError::InvalidCupPosition);
    }
    if order.tests.is_empty()
        || order.tests.len() > 99
        || order.tests.len() > machine.settings().limits().repetitions()
    {
        return Err(OrderError::TestCount);
    }
    let mut tests = Vec::new();
    for test in &order.tests {
        let mapped = codes::map_test(settings.tests(), test)?;
        let [code] = mapped.machine_components.as_slice() else {
            return Err(OrderError::AssayComponents);
        };
        tests.push(Field::<4>::new(code.as_bytes())?);
    }
    let worklist = Worklist::new(patient, list, sample, clone, position, tests)?;
    if worklist.encode().len() > machine.settings().limits().bytes() {
        return Err(OrderError::MessageLimit);
    }
    Ok(worklist)
}

/// Specific outgoing AUTOLYSER mapping refusals.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// No explicitly addressed cup can be encoded; zero is never invented.
    #[error("invalid-cup-position")]
    InvalidCupPosition,
    /// Autolyser requires an explicit addressed cup.
    #[error("missing-cup-position")]
    MissingCupPosition,
    /// An Autolyser field fails the documented fixed-width rules.
    #[error("Autolyser request is not representable: {0}")]
    Syntax(#[from] analyte_autolyser::syntax::Error),
    /// Autolyser test count is empty or exceeds protocol or configured bounds.
    #[error("Autolyser test count is empty or exceeds protocol or configured bounds")]
    TestCount,
    /// Autolyser assay mapping must contain exactly one component.
    #[error("Autolyser assay mapping must contain exactly one component")]
    AssayComponents,
    /// Autolyser worklist exceeds the message byte limit.
    #[error("Autolyser worklist exceeds the message byte limit")]
    MessageLimit,
    /// Shared order evidence is unavailable or unmapped.
    #[error(transparent)]
    Shared(#[from] crate::OrderMappingError),
}
