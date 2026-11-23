//! Converts lab-app orders into outbound mapping inputs.
use analyte_model::{Identifier, NamespacedId, SampleId, SourceText};
/// Adapts validated app fields without inventing a code system or position.
/// # Errors
/// Preserves the exact common-model text constraint failure.
pub fn map(order: &analyte_contract::Order) -> Result<super::AppOrder, analyte_model::TextError> {
    let fields = order.fields();
    let tests = fields
        .tests
        .iter()
        .map(|test| {
            Ok(NamespacedId {
                namespace: Identifier::new(test.namespace.as_str())?,
                value: Identifier::new(test.value.as_str())?,
            })
        })
        .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
    let test_names = fields
        .tests
        .iter()
        .map(|test| {
            test.name
                .as_ref()
                .map(|n| SourceText::new(n.as_str()))
                .transpose()
        })
        .collect::<Result<Vec<_>, analyte_model::TextError>>()?;
    Ok(super::AppOrder {
        sample: SampleId::new(fields.sample_id.as_str())?,
        tests,
        test_names,
        test_dilutions: fields
            .tests
            .iter()
            .map(|t| t.dilution_ratio.map(std::num::NonZeroU16::get))
            .collect(),
        patient: fields
            .patient
            .as_ref()
            .map(|p| namespaced(&p.id))
            .transpose()?,
        specimen_type: fields.specimen_type.as_ref().map(namespaced).transpose()?,
        priority: match fields.priority {
            analyte_contract::Priority::Routine => super::AppOrderPriority::Routine,
            analyte_contract::Priority::Urgent => super::AppOrderPriority::Urgent,
            analyte_contract::Priority::Stat => super::AppOrderPriority::Stat,
        },
        position: fields.position.as_ref().map(position).transpose()?,
    })
}

fn namespaced(
    id: &analyte_contract::document::NamespacedId,
) -> Result<NamespacedId, analyte_model::TextError> {
    Ok(NamespacedId {
        namespace: Identifier::new(id.namespace.as_str())?,
        value: Identifier::new(id.value.as_str())?,
    })
}

fn position(
    position: &analyte_contract::document::SamplePosition,
) -> Result<analyte_model::SamplePosition, analyte_model::TextError> {
    use analyte_contract::document::CarrierKind as Wire;
    use analyte_model::CarrierKind;
    Ok(analyte_model::SamplePosition {
        kind: match &position.kind {
            Wire::Rack => CarrierKind::Rack,
            Wire::Disk => CarrierKind::Disk,
            Wire::Tray => CarrierKind::Tray,
            Wire::Other(id) => CarrierKind::Other(Identifier::new(id.as_str())?),
        },
        carrier: position
            .carrier
            .as_ref()
            .map(|id| Identifier::new(id.as_str()))
            .transpose()?,
        slot: Identifier::new(position.slot.as_str())?,
    })
}
