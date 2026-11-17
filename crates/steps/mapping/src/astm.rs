//! Maps received ASTM messages to model results and app orders to ASTM fields without I/O (LIS02-A2).
use crate::inbound::{grouping, source};
use crate::orders::codes::{map_priority as outbound_priority, map_test as outbound_test};
use crate::orders::{AppOrder, OrderMappingError, map_specimen};
use crate::{Error, Input, Output, Position};

/// Maps one parsed LIS02 message, retaining hierarchy and original value text.
/// # Errors
/// Rejects a mismatched config, invalid ownership, encoding or model resource bounds.
pub fn map(
    message: &analyte_astm::Message,
    syntax: &analyte_config::AstmSyntax,
    mappings: &analyte_config::Mappings,
    input: Input,
) -> Result<Output, Error> {
    let settings = syntax.limits();
    if message.records().len() > settings.records() {
        return Err(Error::Limit {
            position: Position {
                record: 0,
                field: 0,
            },
            resource: "records",
        });
    }
    let mut bytes = 0_usize;
    for (index, record) in message.records().iter().enumerate() {
        bytes = bytes
            .checked_add(record.raw().len())
            .and_then(|b| b.checked_add(1))
            .ok_or(Error::Limit {
                position: Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                resource: "message bytes",
            })?;
        if bytes > settings.bytes() || record.raw().len() > settings.record_bytes() {
            return Err(Error::Limit {
                position: Position {
                    record: index.saturating_add(1),
                    field: 0,
                },
                resource: "message or record bytes",
            });
        }
    }
    let mut source = source::astm(message);
    source.charset = match syntax.encoding() {
        analyte_config::Encoding::Ascii => analyte_hl7::Charset::Ascii,
        analyte_config::Encoding::Latin1 => analyte_hl7::Charset::Latin1,
        analyte_config::Encoding::Utf8 => analyte_hl7::Charset::Utf8,
    };
    source.attach_comments()?;
    grouping::message(
        &source,
        &crate::RecordSettings {
            mappings,
            limits: settings,
        },
        input,
        None,
    )
}

/// Maps explicit ASTM order fields without guessing missing codes or context.
/// # Errors
/// Preserves the existing code-table, context, repertoire and bounds failures.
pub fn map_order(
    order: &AppOrder,
    limits: &analyte_config::SyntaxLimits,
    settings: &analyte_config::AstmOrders,
) -> Result<Vec<u8>, OrderMappingError> {
    let astm = &settings.layout();
    let settings = &settings.tables();
    if order.tests.is_empty() || order.tests.len() > limits.repetitions() {
        return Err(OrderMappingError::AstmTestCount);
    }
    let mut tests = Vec::new();
    for requested in &order.tests {
        let mapped = outbound_test(settings.tests(), requested)?;
        for part in &mapped.machine_components {
            validate_text(part)?;
        }
        tests.push(mapped.machine_components.join("^"));
    }
    let priority = outbound_priority(settings.priorities(), order.priority)?;
    let mut fields: [String; 16] = std::array::from_fn(|_| String::new());
    fields[0] = "O".into();
    fields[1] = "1".into();
    set(
        &mut fields,
        &astm.specimen,
        order.sample.as_str(),
        limits.components(),
    )?;
    if let Some(position) = &astm.position {
        let value = order
            .position
            .as_ref()
            .ok_or(OrderMappingError::MissingPosition)?;
        if value.slot.as_str().is_empty() {
            return Err(OrderMappingError::MissingPosition);
        }
        set(
            &mut fields,
            position,
            value.slot.as_str(),
            limits.components(),
        )?;
    }
    if let Some(destination) = &astm.specimen_type {
        let code = map_specimen(order, settings)?;
        set(&mut fields, destination, code, limits.components())?;
    }
    let prefix = "^".repeat(usize::from(astm.test.component).saturating_sub(1));
    if settings.tests().iter().any(|t| {
        t.machine_components
            .len()
            .checked_add(prefix.len())
            .is_none_or(|n| n > limits.components())
    }) {
        return Err(OrderMappingError::AstmTestComponents);
    }
    fields[4] = tests
        .iter()
        .map(|t| format!("{prefix}{t}"))
        .collect::<Vec<_>>()
        .join("\\");
    set(
        &mut fields,
        &astm.priority,
        priority.machine.as_str(),
        limits.components(),
    )?;
    set(
        &mut fields,
        &astm.action,
        astm.action_code.as_str(),
        limits.components(),
    )?;
    let last_field = if astm.specimen_type.is_some() { 16 } else { 12 };
    let mut record = fields
        .get(..last_field)
        .ok_or(OrderMappingError::AstmRecordLayout)?
        .join("|")
        .into_bytes();
    if record.len() > limits.record_bytes()
        || record
            .len()
            .checked_add(1)
            .is_none_or(|n| n > limits.bytes())
    {
        return Err(OrderMappingError::AstmRecordLimit);
    }
    record.push(b'\r');
    Ok(record)
}

fn set(
    fields: &mut [String],
    selected: &analyte_config::Field,
    value: &str,
    limit: usize,
) -> Result<(), OrderMappingError> {
    validate_text(value)?;
    let component = usize::from(selected.component);
    if component == 0 || component > limit {
        return Err(OrderMappingError::AstmSelectorComponent);
    }
    let field = fields
        .get_mut(usize::from(selected.field).saturating_sub(1))
        .ok_or(OrderMappingError::AstmSelectorField)?;
    *field = format!("{}{value}", "^".repeat(component.saturating_sub(1)));
    Ok(())
}

fn validate_text(value: &str) -> Result<(), OrderMappingError> {
    // This initial outbound repertoire refuses reserved characters rather than
    // changing clinical identifiers or guessing a vendor escape convention.
    if value.len() > 4096
        || !value.is_ascii()
        || value
            .bytes()
            .any(|b| !(32..127).contains(&b) || b"|\\^&".contains(&b))
    {
        return Err(OrderMappingError::AstmTextRepertoire);
    }
    Ok(())
}
