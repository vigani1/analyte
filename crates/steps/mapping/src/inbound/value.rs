//! Typed value interpretation never replaces original wire text.
use crate::inbound::source::Source;
use crate::{Error, Output, Position, Problem, finding, identifier, list, text};
use analyte_model::*;

pub(crate) fn values(
    source: &Source<'_>,
    index: usize,
    field: usize,
    output: &mut Output,
) -> Result<EvidenceList<ResultValue>, Error> {
    let position = source.position(index, field);
    let astm = source.syntax == crate::FieldSyntax::AstmLis02;
    let data_type = if astm {
        String::new()
    } else {
        source.raw(index, 2)?.unwrap_or_default()
    };
    let mut originals = source.repetitions(index, field)?;
    // A status-only observation still has evidence: its value was absent.
    if originals.is_empty() {
        originals.push(String::new());
    }
    let mut values = Vec::new();
    for (ordinal, original) in originals.into_iter().enumerate() {
        let repetition = ordinal.saturating_add(1);
        let text_problem = source.text_problem(&original);
        let parsed = if let Some(problem) = &text_problem {
            let reason = if *problem == Problem::UnsupportedFormatting {
                "HL7 formatting or highlighting is not interpreted"
            } else {
                "HL7 escape requires an unsupported interpretation agreement"
            };
            unsupported(&data_type, position, reason)?
        } else if astm {
            decimal(&original, true)
        } else {
            match data_type.as_str() {
                "NM" => decimal(&original, false),
                "ST" | "TX" | "FT" | "DT" | "TM" | "TS" | "ID" | "IS" => ValueInterpretation::Text,
                "CE" | "CWE" => coded(source, index, field, repetition)?.unwrap_or(unsupported(
                    &data_type,
                    position,
                    "coded value lacks code or assigning system",
                )?),
                "SN" => structured(source, index, field, repetition)?.unwrap_or(unsupported(
                    &data_type,
                    position,
                    "malformed structured numeric value",
                )?),
                "" if original.is_empty() => ValueInterpretation::Text,
                "" => {
                    return Err(Error::Structure {
                        position: source.position(index, 2),
                        reason: "value type absent for a populated observation",
                    });
                }
                _ => unsupported(&data_type, position, "unsupported HL7 value type")?,
            }
        };
        if parsed.is_unsupported() {
            finding(
                output,
                position,
                text_problem.unwrap_or(Problem::UnsupportedValue),
            )?;
        }
        values.push(ResultValue {
            decoded_text: if matches!(parsed, ValueInterpretation::Text) {
                source
                    .decoded_repetition(index, field, ordinal)?
                    .map(|value| text(&value, position))
                    .transpose()?
            } else {
                None
            },
            original: text(&original, position)?,
            parsed,
        });
    }
    list(values, position)
}

fn unsupported(
    data_type: &str,
    position: Position,
    reason: &str,
) -> Result<ValueInterpretation, Error> {
    Ok(ValueInterpretation::Unsupported {
        source_type: identifier(data_type, position)?,
        reason: Reason::new(reason).map_err(|source| Error::Text { position, source })?,
    })
}

fn comparator(value: &str) -> Option<Comparator> {
    match value {
        "<" => Some(Comparator::Less),
        "<=" => Some(Comparator::LessOrEqual),
        ">" => Some(Comparator::Greater),
        ">=" => Some(Comparator::GreaterOrEqual),
        _ => None,
    }
}

fn decimal(raw: &str, allow_comparator: bool) -> ValueInterpretation {
    let mut number = raw;
    let mut bound = None;
    if allow_comparator {
        for prefix in ["<=", ">=", "<", ">"] {
            if let Some(rest) = raw.strip_prefix(prefix) {
                number = rest;
                bound = comparator(prefix);
                break;
            }
        }
    }
    Decimal::new(number).map_or(ValueInterpretation::Text, |number| {
        ValueInterpretation::Numeric(NumericValue {
            number,
            comparator: bound,
        })
    })
}

fn coded(
    source: &Source<'_>,
    index: usize,
    field: usize,
    repetition: usize,
) -> Result<Option<ValueInterpretation>, Error> {
    let position = source.position(index, field);
    let code = source
        .component(index, field, repetition, 1)?
        .unwrap_or_default();
    let system = source
        .component(index, field, repetition, 3)?
        .unwrap_or_default();
    if code.is_empty() || system.is_empty() || code == "\"\"" || system == "\"\"" {
        return Ok(None);
    }
    Ok(Some(ValueInterpretation::Coded(CodedValue {
        code: NamespacedId {
            value: identifier(&code, position)?,
            namespace: identifier(&system, position)?,
        },
        text: text(
            &source
                .component(index, field, repetition, 2)?
                .unwrap_or_default(),
            position,
        )?,
    })))
}

fn structured(
    source: &Source<'_>,
    index: usize,
    field: usize,
    repetition: usize,
) -> Result<Option<ValueInterpretation>, Error> {
    let position = source.position(index, field);
    let bound = source
        .component(index, field, repetition, 1)?
        .unwrap_or_default();
    let first = source
        .component(index, field, repetition, 2)?
        .unwrap_or_default();
    let separator = source
        .component(index, field, repetition, 3)?
        .unwrap_or_default();
    let second = source
        .component(index, field, repetition, 4)?
        .unwrap_or_default();
    if (!bound.is_empty() && bound != "=" && comparator(&bound).is_none())
        || !matches!(separator.as_str(), "" | ":" | "-" | "+" | "/")
    {
        return Ok(None);
    }
    let Ok(first) = Decimal::new(&first) else {
        return Ok(None);
    };
    let second = if second.is_empty() {
        None
    } else {
        let Ok(value) = Decimal::new(&second) else {
            return Ok(None);
        };
        Some(value)
    };
    if separator.is_empty() != second.is_none() {
        return Ok(None);
    }
    Ok(Some(ValueInterpretation::StructuredNumeric(
        StructuredNumeric {
            comparator: comparator(&bound),
            first: Some(first),
            separator: text(&separator, position)?,
            second,
        },
    )))
}

/// Test expectations constrain interpretation without changing source evidence.
pub(crate) fn check_kind(
    values: &EvidenceList<ResultValue>,
    expected: analyte_config::ValueKind,
    position: Position,
    output: &mut Output,
) -> Result<(), Error> {
    use analyte_config::ValueKind;
    for value in values.as_slice() {
        let fits = match expected {
            ValueKind::Either => true,
            ValueKind::Numeric => matches!(value.parsed, ValueInterpretation::Numeric(_)),
            ValueKind::Coded => {
                matches!(value.parsed, ValueInterpretation::Coded(_))
                    || (matches!(value.parsed, ValueInterpretation::Text)
                        && !value.original.as_str().is_empty()
                        && matches!(
                            decimal(value.original.as_str(), true),
                            ValueInterpretation::Text
                        ))
            }
        };
        if !fits {
            finding(output, position, Problem::ValueKindMismatch { expected })?;
        }
    }
    Ok(())
}
