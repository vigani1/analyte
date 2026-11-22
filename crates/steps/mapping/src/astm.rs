//! Maps received ASTM messages to model results and app orders to ASTM fields without I/O (LIS02-A2).
use crate::inbound::{grouping, source};
use crate::orders::codes::{map_priority, map_test};
use crate::orders::{AppOrder, map_specimen};
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
    source.charset = syntax.encoding();
    source.attach_comments()?;
    let mut output = grouping::message(
        &source,
        &crate::RecordSettings {
            mappings,
            limits: settings,
        },
        input,
        None,
    )?;
    note_control_characters(message, &mut output)?;
    Ok(output)
}

/// Tolerance tag, shown to operators beside the receipt's departure evidence.
const CONTROL_CHARACTER_TOLERANCE: &str = "astm_control_character_in_text";

/// Adds one visible note per field that kept a literal BEL, HT, VT or FF.
/// The tolerance is always on: LIS02-A2 §5.1 allows these bytes in a record, values
/// stay byte for byte, and identifiers still refuse controls at their own boundary.
fn note_control_characters(
    message: &analyte_astm::Message,
    output: &mut Output,
) -> Result<(), Error> {
    let mut start = 0usize;
    for (index, record) in message.records().iter().enumerate() {
        let number = index.saturating_add(1);
        let end = start.checked_add(record.raw().len()).ok_or(Error::Limit {
            position: Position {
                record: number,
                field: 0,
            },
            resource: "source byte span",
        })?;
        for (field_index, field) in record.fields().iter().enumerate() {
            if !field.has_control_character() {
                continue;
            }
            let position = Position {
                record: number,
                field: field_index.saturating_add(1),
            };
            if output.applied_departures.len()
                == analyte_model::EvidenceList::<analyte_model::AppliedDeparture>::MAX_ENTRIES
            {
                return Err(Error::Limit {
                    position,
                    resource: "applied departures",
                });
            }
            output
                .applied_departures
                .push(analyte_model::AppliedDeparture {
                    rule: crate::identifier(CONTROL_CHARACTER_TOLERANCE, position)?,
                    declaration: crate::text(
                        "always on: deliberate tolerance (docs/protocols/astm/decisions.md)",
                        position,
                    )?,
                    standard: crate::identifier("LIS02-A2 §5.1", position)?,
                    record: position.record,
                    field: position.field,
                    source: crate::text(
                        "Literal BEL, HT, VT or FF in a text field; value kept byte for byte",
                        position,
                    )?,
                    meaning: analyte_model::DepartureMeaning::ControlCharacterKept,
                    coordinates: crate::identifier("syntax_bytes", position)?,
                    spans: crate::list(
                        vec![
                            analyte_model::DepartureSpan::try_from(start..end)
                                .map_err(|reason| Error::Structure { position, reason })?,
                        ],
                        position,
                    )?,
                });
        }
        // Each record is followed by its CR in the message's syntax bytes.
        start = end.saturating_add(1);
    }
    Ok(())
}

/// Maps explicit ASTM order fields without guessing missing codes or context.
/// Test codes go out exactly as the order wrote them, in the machine's code system.
/// # Errors
/// Preserves the code-system, context, repertoire and bounds failures.
pub fn map_order(
    order: &AppOrder,
    limits: &analyte_config::SyntaxLimits,
    settings: &analyte_config::AstmOrders,
) -> Result<analyte_astm::OrderRecord, OrderError> {
    let astm = &settings.layout();
    let settings = &settings.tables();
    if order.tests.is_empty() || order.tests.len() > limits.repetitions() {
        return Err(OrderError::TestCount);
    }
    let mut tests = Vec::new();
    for requested in &order.tests {
        tests.push(map_test(settings.test_system(), requested)?);
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
            .ok_or(crate::OrderMappingError::MissingPosition)?;
        if value.slot.as_str().is_empty() {
            return Err(crate::OrderMappingError::MissingPosition.into());
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
    // LIS02 section 5.6.1: the code goes in its component of the universal
    // test ID; the components before it, and any the machine expects after it,
    // stay empty.
    let code_at = usize::from(astm.test.component);
    let width = astm.test_components.map_or(code_at, usize::from);
    if code_at == 0 || width < code_at || width > limits.components() {
        return Err(OrderError::TestComponents);
    }
    fields[4] = tests
        .into_iter()
        .map(|code| {
            let mut parts = vec![String::new(); width];
            if let Some(part) = parts.get_mut(code_at.saturating_sub(1)) {
                code.clone_into(part);
            }
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
        .ok_or(OrderError::RecordLayout)?
        .to_vec();
    analyte_astm::OrderRecord::new(values, limits.record_bytes(), limits.bytes())
        .map_err(order_error)
}

fn set(
    fields: &mut [Vec<Vec<String>>],
    selected: &analyte_config::Field,
    value: &str,
    limit: usize,
) -> Result<(), OrderError> {
    let component = usize::from(selected.component);
    if component == 0 || component > limit {
        return Err(OrderError::SelectorComponent);
    }
    let field = fields
        .get_mut(usize::from(selected.field).saturating_sub(1))
        .ok_or(OrderError::SelectorField)?;
    let mut parts = vec![String::new(); component];
    let part = parts.last_mut().ok_or(OrderError::SelectorComponent)?;
    *part = value.into();
    *field = vec![parts];
    Ok(())
}

fn order_error(error: analyte_astm::OrderError) -> OrderError {
    match error {
        analyte_astm::OrderError::Text => OrderError::TextRepertoire,
        analyte_astm::OrderError::Field => OrderError::SelectorField,
        analyte_astm::OrderError::Limit => OrderError::RecordLimit,
    }
}

/// Specific outgoing ASTM mapping refusals.
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// ASTM test count is empty or exceeds the repetition limit.
    #[error("ASTM test count is empty or exceeds the repetition limit")]
    TestCount,
    /// ASTM test components exceed the configured limit.
    #[error("ASTM test components exceed the configured limit")]
    TestComponents,
    /// ASTM order fields do not cover the selected record layout.
    #[error("ASTM order fields do not cover the selected record layout")]
    RecordLayout,
    /// ASTM order exceeds record or byte limits.
    #[error("ASTM order exceeds record or byte limits")]
    RecordLimit,
    /// ASTM selector component is zero or exceeds the configured limit.
    #[error("ASTM selector component is zero or exceeds the configured limit")]
    SelectorComponent,
    /// ASTM selector field is outside the order record.
    #[error("ASTM selector field is outside the order record")]
    SelectorField,
    /// ASTM text is empty or contains unsupported bytes or delimiters.
    #[error("ASTM text is empty or contains unsupported bytes or delimiters")]
    TextRepertoire,
    /// Shared order evidence is unavailable or unmapped.
    #[error(transparent)]
    Shared(#[from] crate::OrderMappingError),
}
