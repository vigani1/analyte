//! Qualifies required report context independently of selected clinical field mappings.
use crate::{Error, Output, Position, Problem, finding};
use analyte_hl7::{Field, Message, Msh, Version};

pub(super) fn qualify(message: &Message, output: &mut Output) -> Result<(), Error> {
    let modern = matches!(
        Msh::new(message)
            .map_err(|source| Error::Hl7 {
                position: Position {
                    record: 1,
                    field: 12
                },
                source
            })?
            .version(),
        Version::V282 | Version::V291
    );
    for (index, segment) in message.segments().enumerate() {
        let required: &[usize] = match segment.id() {
            b"OBR" if modern => &[4, 22, 25],
            b"OBR" => &[4, 25],
            b"OBX" => &[3, 11],
            _ => &[],
        };
        for field in required {
            if segment
                .field(*field)
                .is_none_or(|value| value.raw().is_empty() || value.is_null())
            {
                finding(
                    output,
                    Position {
                        record: index.saturating_add(1),
                        field: *field,
                    },
                    Problem::UnqualifiedHl7Report,
                )?;
            }
        }
        if segment.id() == b"OBR"
            && !matches!(
                segment.field(25).map_or(&[][..], Field::raw),
                b"O" | b"I" | b"S" | b"A" | b"P" | b"C" | b"R" | b"F" | b"X" | b"Y" | b"Z"
            )
        {
            finding(
                output,
                Position {
                    record: index.saturating_add(1),
                    field: 25,
                },
                Problem::UnqualifiedHl7Report,
            )?;
        }
        if segment.id() == b"OBX" {
            let code = segment.field(11).map_or(&[][..], Field::raw);
            // Supported table 0085 states; other meanings remain evidence, never qualified output.
            if !matches!(
                code,
                b"C" | b"P" | b"S" | b"F" | b"X" | b"I" | b"R" | b"D" | b"W" | b"U"
            ) {
                finding(
                    output,
                    Position {
                        record: index.saturating_add(1),
                        field: 11,
                    },
                    Problem::UnqualifiedHl7Report,
                )?;
            }
            if segment
                .field(5)
                .is_some_and(|value| !value.raw().is_empty() && !value.is_null())
                && segment
                    .field(2)
                    .is_none_or(|value| value.raw().is_empty() || value.is_null())
            {
                finding(
                    output,
                    Position {
                        record: index.saturating_add(1),
                        field: 2,
                    },
                    Problem::UnqualifiedHl7Report,
                )?;
            }
        }
    }
    Ok(())
}
