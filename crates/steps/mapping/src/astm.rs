//! Maps received ASTM messages to model results and app orders to ASTM fields without I/O (LIS02-A2).
use crate::inbound::{grouping, source};
use crate::orders::codes::{map_priority, map_test};
use crate::orders::{AppOrder, OrderMappingError, map_specimen};
use crate::{Error, Input, Output, Position};

/// Selected ASTM syntax and incoming code mappings.
pub struct Settings<'a> {
    /// Validated syntax and bounds.
    pub syntax: &'a analyte_config::AstmSyntax,
    /// Explicit incoming translations.
    pub mappings: &'a analyte_config::Mappings,
}

/// Maps one parsed LIS02 message, retaining hierarchy and original value text.
/// # Errors
/// Rejects a mismatched config, invalid ownership, encoding or model resource bounds.
pub fn map(
    settings: &Settings<'_>,
    message: &analyte_astm::Message,
    input: Input<'_>,
) -> Result<Output, Error> {
    let Settings { syntax, mappings } = settings;
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
) -> Result<analyte_astm::OrderRecord, OrderMappingError> {
    let astm = &settings.layout();
    let settings = &settings.tables();
    if order.tests.is_empty() || order.tests.len() > limits.repetitions() {
        return Err(OrderMappingError::AstmTestCount);
    }
    let mut tests = Vec::new();
    for requested in &order.tests {
        let mapped = map_test(settings.tests(), requested)?;
        tests.push(mapped.machine_components.clone());
    }
    let priority = map_priority(settings.priorities(), order.priority)?;
    let mut fields: [Vec<Vec<String>>; 16] = std::array::from_fn(|_| Vec::new());
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
    let prefix = usize::from(astm.test.component).saturating_sub(1);
    if settings.tests().iter().any(|t| {
        t.machine_components
            .len()
            .checked_add(prefix)
            .is_none_or(|n| n > limits.components())
    }) {
        return Err(OrderMappingError::AstmTestComponents);
    }
    fields[4] = tests
        .into_iter()
        .map(|test| {
            let mut parts = vec![String::new(); prefix];
            parts.extend(test);
            parts
        })
        .collect();
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
    let values = fields
        .get(2..last_field)
        .ok_or(OrderMappingError::AstmRecordLayout)?
        .to_vec();
    analyte_astm::OrderRecord::new(values, limits.record_bytes(), limits.bytes())
        .map_err(order_error)
}

fn set(
    fields: &mut [Vec<Vec<String>>],
    selected: &analyte_config::Field,
    value: &str,
    limit: usize,
) -> Result<(), OrderMappingError> {
    let component = usize::from(selected.component);
    if component == 0 || component > limit {
        return Err(OrderMappingError::AstmSelectorComponent);
    }
    let field = fields
        .get_mut(usize::from(selected.field).saturating_sub(1))
        .ok_or(OrderMappingError::AstmSelectorField)?;
    let mut parts = vec![String::new(); component];
    let part = parts
        .last_mut()
        .ok_or(OrderMappingError::AstmSelectorComponent)?;
    *part = value.into();
    *field = vec![parts];
    Ok(())
}

fn order_error(error: analyte_astm::OrderError) -> OrderMappingError {
    match error {
        analyte_astm::OrderError::Text => OrderMappingError::AstmTextRepertoire,
        analyte_astm::OrderError::Field => OrderMappingError::AstmSelectorField,
        analyte_astm::OrderError::Limit => OrderMappingError::AstmRecordLimit,
    }
}
