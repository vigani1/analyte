//! Validates fields interpreted by the bounded result profile before clinical mapping.
use crate::{Error, Message, Msh, Repetition, datatypes, version::FieldKind};

/// Checks the declared edition's interpreted result fields without changing raw bytes.
/// Unselected fields remain raw; unsupported value types are parked by mapping.
/// # Errors
/// Returns the segment and field for withdrawn types, malformed composites or
/// unselected result moods. Does not qualify arbitrary HL7 structures.
pub fn validate_result_fields(message: &Message) -> Result<(), Error> {
    let version = Msh::new(message)?.version();
    for segment in message.segments() {
        for row in version.require_rules()?.result_fields {
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
                value.raw().len() <= 705
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
