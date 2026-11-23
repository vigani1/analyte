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
    if layout.record_location.is_some() {
        return positional(order, orders, limits);
    }
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

// YHLO LIS V1.1 section 2.3.10. No authoritative analyzer sample number
// is present in an app order. Leave DSP-22 empty as permitted by the prose.
fn positional(
    order: &AppOrder,
    orders: &Hl7Orders,
    limits: &SyntaxLimits,
) -> Result<Vec<Vec<u8>>, OrderError> {
    if order.tests.is_empty() {
        return Err(OrderError::DisplayEmpty);
    }
    if 28_usize.saturating_add(order.tests.len()) > limits.records() {
        return Err(OrderError::DisplayLineBounds);
    }
    let settings = orders.tables();
    let priority = map_priority(settings.priorities(), order.priority)?
        .machine
        .as_str();
    let specimen = order
        .specimen_type
        .as_ref()
        .map(|source| map_specimen(settings.specimen_system(), source))
        .transpose()?
        .unwrap_or("");
    let mut rows: [Vec<u8>; 28] = std::array::from_fn(|_| Vec::new());
    for value in [order.sample.as_str(), priority, specimen] {
        if !value.is_empty()
            && !analyte_hl7::is_safe_field(value.as_bytes(), 300, analyte_hl7::FieldSyntax::Plain)
        {
            return Err(OrderError::DisplayLineBounds);
        }
    }
    rows[20] = order.sample.as_str().as_bytes().to_vec();
    rows[23] = priority.as_bytes().to_vec();
    rows[25] = specimen.as_bytes().to_vec();
    let mut lines = Vec::from(rows);
    for test in &order.tests {
        let code = map_test(settings.test_system(), test)?.as_bytes();
        if !analyte_hl7::is_safe_field(code, 297, analyte_hl7::FieldSyntax::Plain) {
            return Err(OrderError::DisplayLineBounds);
        }
        lines.push(code.to_vec());
    }
    Ok(lines)
}
