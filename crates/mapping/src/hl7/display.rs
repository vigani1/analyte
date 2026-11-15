use crate::orders::{AppOrder, OrderMappingError};
use crate::outbound_codes::{
    map_priority as outbound_priority, map_specimen as outbound_specimen, map_test as outbound_test,
};
use analyte_config::{DsrLine, Validated};

/// Maps explicitly configured DSP text, preserving values and leaving absent context empty.
/// Line order repeats for every test; no machine-specific positions are implicit.
/// # Errors
/// Refuses unmapped codes/priorities, namespace mismatches, delimiter injection and bounds.
pub fn map(
    order: &AppOrder,
    order_id: &str,
    config: &Validated,
) -> Result<Vec<Vec<u8>>, OrderMappingError> {
    use analyte_config::Stack;
    let orders = match config.effective().stack() {
        Stack::Hl7(s) => s.orders.as_ref(),
        Stack::FileHl7(s) => s.orders.as_ref(),
        Stack::Astm(_)
        | Stack::FileAstm(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::VidasLiteral(_)
        | Stack::Autolyser(_) => None,
    }
    .ok_or(OrderMappingError::Disabled)?;
    let layout = &orders
        .query
        .as_ref()
        .ok_or(OrderMappingError::Disabled)?
        .layout;
    let settings = &orders.tables;
    let mut lines = Vec::new();
    for test in &order.tests {
        let mapped = outbound_test(&settings.tests, test)?;
        let Some(code) = mapped.machine_components.first() else {
            return Err(OrderMappingError::Representation);
        };
        for line in &layout.lines {
            let value = match line {
                DsrLine::SampleId => order.sample.as_str(),
                DsrLine::OrderId => order_id,
                DsrLine::TestCode => code,
                DsrLine::Priority => outbound_priority(&settings.priorities, order.priority)?
                    .machine
                    .as_str(),
                DsrLine::Position => order.position.as_ref().map_or("", |p| p.slot.as_str()),
                DsrLine::PatientId => match &order.patient {
                    None => "",
                    Some(patient)
                        if orders.patient_namespace.as_ref() == Some(&patient.namespace) =>
                    {
                        patient.value.as_str()
                    }
                    Some(_) => return Err(OrderMappingError::PatientNamespace),
                },
                DsrLine::SpecimenType => match &order.specimen_type {
                    None => "",
                    Some(source) => outbound_specimen(settings.specimen_types.as_deref(), source)?
                        .machine
                        .as_str(),
                },
                DsrLine::Literal { value } => value,
            };
            if value.len() > 300
                || !value.is_ascii()
                || value
                    .bytes()
                    .any(|b| b.is_ascii_control() || b"|^~\\&".contains(&b))
                || lines.len() >= config.effective().stack().limits().records
            {
                return Err(OrderMappingError::Representation);
            }
            lines.push(value.as_bytes().to_vec());
        }
    }
    if lines.is_empty() {
        return Err(OrderMappingError::Representation);
    }
    Ok(lines)
}
