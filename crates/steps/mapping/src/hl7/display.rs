//! Maps app orders to legacy HL7 display responses.
use super::OrderError;
use crate::orders::AppOrder;
use crate::orders::codes::{map_priority, map_specimen, map_test};
use analyte_config::{DsrLine, Hl7Orders, SyntaxLimits};

/// Maps explicitly configured DSP text, preserving values and leaving absent context empty.
/// Line order repeats for every test; no machine-specific positions are implicit.
/// # Errors
/// Refuses foreign test and specimen codes, unmapped priorities, namespace
/// mismatches, delimiter injection and bounds.
pub fn map(
    order: &AppOrder,
    order_id: &str,
    orders: &Hl7Orders,
    limits: &SyntaxLimits,
) -> Result<Vec<Vec<u8>>, OrderError> {
    let layout = &orders
        .query()
        .ok_or(crate::OrderMappingError::Disabled)?
        .layout();
    let settings = &orders.tables();
    let mut lines = Vec::new();
    for test in &order.tests {
        let code = map_test(settings.test_system(), test)?;
        for line in &layout.lines {
            let value = match line {
                DsrLine::SampleId => order.sample.as_str(),
                DsrLine::OrderId => order_id,
                DsrLine::TestCode => code,
                DsrLine::Priority => map_priority(settings.priorities(), order.priority)?
                    .machine
                    .as_str(),
                DsrLine::Position => order.position.as_ref().map_or("", |p| p.slot.as_str()),
                DsrLine::PatientId => match &order.patient {
                    None => "",
                    Some(patient) if orders.patient_namespace() == Some(&patient.namespace) => {
                        patient.value.as_str()
                    }
                    Some(_) => return Err(crate::OrderMappingError::PatientNamespace.into()),
                },
                DsrLine::SpecimenType => match &order.specimen_type {
                    None => "",
                    Some(source) => map_specimen(settings.specimen_system(), source)?,
                },
                DsrLine::Literal { value } => value,
            };
            if value.len() > 300
                || !value.is_ascii()
                || value
                    .bytes()
                    .any(|b| b.is_ascii_control() || b"|^~\\&".contains(&b))
                || lines.len() >= limits.records()
            {
                return Err(OrderError::DisplayLineBounds);
            }
            lines.push(value.as_bytes().to_vec());
        }
    }
    if lines.is_empty() {
        return Err(OrderError::DisplayEmpty);
    }
    Ok(lines)
}
