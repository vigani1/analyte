//! Validates fields interpreted by the bounded result profile before clinical mapping.
use crate::{Error, Message, Msh, Repetition, datatypes, version::FieldKind};

/// Checks the declared edition's interpreted result fields without changing raw bytes.
/// Unselected fields remain raw; unsupported value types are parked by mapping.
/// # Errors
/// Returns the segment and field for withdrawn types, malformed composites or
/// unselected result moods. Does not qualify arbitrary HL7 structures.
pub fn validate_result_fields(message: &Message) -> Result<(), Error> {
    let version = Msh::new(message)?.version();
    let rules = version.require_rules()?;
    for segment in message.segments() {
        validate_context(segment, rules)?;
        // Enhanced OG identity needs a full grouping contract, which this profile does not select.
        if version.enhanced_result_identity()
            && segment.id() == b"OBX"
            && segment.field(4).is_some_and(|field| {
                field.repetitions().any(|value| {
                    value
                        .components()
                        .skip(1)
                        .any(|component| !component.raw().is_empty())
                })
            })
        {
            return Err(Error::Control {
                segment: "OBX",
                field: 4,
            });
        }

        for row in rules.result_fields {
            if segment.id() != row.segment {
                continue;
            }
            let Some(field) = segment.field(row.field) else {
                continue;
            };
            if field.raw().is_empty() || field.is_null() {
                continue;
            }
            let scalar_time = |value: Repetition<'_>| {
                value.raw().len() <= 24
                    && datatypes::edition_timestamp(
                        &value.components().collect::<Vec<_>>(),
                        version,
                    )
            };
            let cwe = |value: Repetition<'_>| {
                value.raw().len() <= rules.coded_field_bytes
                    && datatypes::coded(&value.components().collect::<Vec<_>>(), version)
            };
            let valid = match row.kind {
                FieldKind::Dtm => {
                    field.repetitions().count() == 1 && field.repetitions().all(scalar_time)
                }
                FieldKind::Cwe => field.repetitions().count() == 1 && field.repetitions().all(cwe),
                FieldKind::ValueType => !matches!(field.raw(), b"CE" | b"TS"),
                FieldKind::Value => match segment.field(2).map_or(&[][..], crate::Field::raw) {
                    b"CWE" => field.repetitions().all(cwe),
                    b"DTM" => field.repetitions().all(scalar_time),
                    // No fallback conversion: mapping parks any other unsupported type.
                    _ => true,
                },
                FieldKind::Refuse => false,
            };
            if !valid {
                return Err(Error::Control {
                    segment: if segment.id() == b"OBR" { "OBR" } else { "OBX" },
                    field: row.field,
                });
            }
        }
    }
    Ok(())
}

// Reject contexts that would let mapping assign a meaning the selected profile cannot establish.
fn validate_context(
    segment: crate::Segment<'_>,
    rules: &crate::version::Rules,
) -> Result<(), Error> {
    if segment.id() == b"OBR" {
        for number in rules.withdrawn_obr {
            if segment
                .field(*number)
                .is_some_and(|field| !field.raw().is_empty() && !field.is_null())
            {
                return Err(Error::Control {
                    segment: "OBR",
                    field: *number,
                });
            }
        }
    }
    if segment.id() == b"OBX" {
        if let Some(component) = rules.observation_authority
            && let Some(field) = segment
                .field(3)
                .filter(|field| !field.raw().is_empty() && !field.is_null())
        {
            for value in field.repetitions() {
                if value
                    .components()
                    .next()
                    .is_some_and(|code| !code.raw().is_empty())
                    && value
                        .components()
                        .nth(component.saturating_sub(1))
                        .is_none_or(|system| system.raw().is_empty())
                {
                    return Err(Error::Control {
                        segment: "OBX",
                        field: 3,
                    });
                }
            }
        }
        for (dependent, required) in rules.observation_dependencies {
            if segment
                .field(*dependent)
                .is_some_and(|field| !field.raw().is_empty() && !field.is_null())
                && segment
                    .field(*required)
                    .is_none_or(|field| field.raw().is_empty() || field.is_null())
            {
                return Err(Error::Control {
                    segment: "OBX",
                    field: *required,
                });
            }
        }
    }
    Ok(())
}
