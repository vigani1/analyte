//! Interprets configured ASTM upload layouts while retaining original source evidence.
use crate::inbound::{
    source::{Record, Source},
    value,
};
use crate::{Error, Output, Position, Problem, finding, list, text};
use analyte_config::{AstmResultLayout, ValueKind};
use analyte_model::{
    Decimal, Dilution, DilutionApplied, EvidenceList, MeasurementMethod, QcClass,
    QualitativeReaction, ReportCompletion, ResultAlarm, ResultDetails, ResultPart, ResultStatus,
    ResultValue, SourceText, ValueInterpretation,
};

const ABSENT: &str = "       ";

pub(crate) fn is_structured(layout: AstmResultLayout) -> bool {
    layout != AstmResultLayout::MeasurementMarker
}

pub(crate) fn uses_slash(layout: AstmResultLayout) -> bool {
    matches!(
        layout,
        AstmResultLayout::SlashDilutionAndAlarmCode
            | AstmResultLayout::SlashDilutionAndAlarmCodeWithQcLot
    )
}

fn slash_parts(raw: &str) -> Option<[&str; 3]> {
    let (code, rest) = raw.split_once('/')?;
    let (factor, predilution) = rest.split_once('/')?;
    (!code.is_empty() && !predilution.contains('/')).then_some([code, factor, predilution])
}

pub(crate) fn slash_code(
    source: &Source<'_>,
    index: usize,
    output: &mut Output,
) -> Result<Option<String>, Error> {
    let field = source.component(index, 3, 1, 4)?.unwrap_or_default();
    // A slash declared by H.2 is structural, so it cannot also split R.3.4.
    let collision = source.nodes.first().is_some_and(|node| match node.record {
        Record::Astm(header) => header
            .raw()
            .get(1..5)
            .is_some_and(|bytes| bytes.contains(&b'/')),
        Record::Hl7(_) => false,
    });
    if !collision && let Some([code, _, _]) = slash_parts(&field) {
        return Ok(Some(code.to_owned()));
    }
    finding(output, source.position(index, 3), Problem::ComponentParts)?;
    Ok(None)
}

pub(crate) fn measurement_values(
    source: &Source<'_>,
    index: usize,
    output: &mut Output,
) -> Result<EvidenceList<ResultValue>, Error> {
    let position = source.position(index, 4);
    let original = source.raw(index, 4)?.unwrap_or_default();
    // Appendix VI p. 86 pads decimal results with literal ASCII spaces.
    let parsed = value::decimal(original.trim_matches(' '), false);
    if !matches!(parsed, ValueInterpretation::Numeric(_)) {
        finding(output, position, Problem::UnsupportedValue)?;
    }
    list(
        vec![ResultValue::new(text(&original, position)?, parsed)],
        position,
    )
}

pub(crate) fn structured_values(
    source: &Source<'_>,
    index: usize,
    layout: AstmResultLayout,
    expected: ValueKind,
    details: &mut ResultDetails,
    output: &mut Output,
) -> Result<EvidenceList<ResultValue>, Error> {
    let position = source.position(index, 4);
    let original = source.raw(index, 4)?.unwrap_or_default();
    let separator = char::from(source.component_separator);
    let first = source.component(index, 4, 1, 1)?.unwrap_or_default();
    let second = source.component(index, 4, 1, 2)?;
    // Compare with the original spelling as these formats define no escapes or repeats.
    let exact_shape = second.as_ref().map_or(original == first, |second| {
        original.split_once(separator) == Some((first.as_str(), second.as_str()))
    });
    let parsed = if exact_shape {
        interpret(
            &first,
            second.as_deref(),
            expected,
            uses_slash(layout),
            position,
            details,
        )?
    } else {
        None
    };
    let parsed = match parsed {
        Some(parsed) => parsed,
        None => {
            finding(output, position, Problem::UnsupportedValue)?;
            value::unsupported(
                "ASTM R.4",
                position,
                "result does not fit the documented layout and test kind",
            )?
        }
    };
    list(
        vec![ResultValue::new(text(&original, position)?, parsed)],
        position,
    )
}

fn interpret(
    first: &str,
    second: Option<&str>,
    expected: ValueKind,
    message_code_layout: bool,
    position: Position,
    details: &mut ResultDetails,
) -> Result<Option<ValueInterpretation>, Error> {
    // The slash layout transmits an empty message code when there is no code.
    let second = second.filter(|value| !message_code_layout || !value.is_empty());
    if first == ABSENT && second.is_none() {
        return Ok(Some(ValueInterpretation::Absent));
    }
    let expected = match (expected, second, message_code_layout) {
        (ValueKind::Either, None, _) => ValueKind::Numeric,
        (ValueKind::Either, Some(_), false) => ValueKind::Coded,
        (explicit, _, _) => explicit,
    };
    match expected {
        ValueKind::Numeric => {
            if let Some(second) = second {
                if !message_code_layout
                    || !digits(second, 2)
                    || !second.parse::<u8>().is_ok_and(|code| code <= 31)
                {
                    return Ok(None);
                }
                details.message_code = Some(text(second, position)?);
            }
            Ok(number(first))
        }
        ValueKind::Coded => {
            let Some(second) = second else {
                return Ok(None);
            };
            let Some(cutoff) = number(second) else {
                return Ok(None);
            };
            details.cutoff_index = Some(ResultValue::new(text(second, position)?, cutoff));
            let reaction = match first {
                "-1" => QualitativeReaction::Negative,
                "0" => QualitativeReaction::Borderline,
                "1" => QualitativeReaction::Positive,
                _ => return Ok(None),
            };
            Ok(Some(ValueInterpretation::Qualitative(reaction)))
        }
        ValueKind::Either => Ok(None),
    }
}

fn number(raw: &str) -> Option<ValueInterpretation> {
    if raw == ABSENT {
        return Some(ValueInterpretation::Absent);
    }
    if raw.len() > 7 || (raw.starts_with("<=") || raw.starts_with(">=")) {
        return None;
    }
    let parsed = value::decimal(raw, true);
    matches!(parsed, ValueInterpretation::Numeric(_)).then_some(parsed)
}

fn digits(raw: &str, max: usize) -> bool {
    !raw.is_empty() && raw.len() <= max && raw.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn specimen(
    source: &Source<'_>,
    order: usize,
    layout: AstmResultLayout,
    details: &mut ResultDetails,
    output: &mut Output,
) -> Result<bool, Error> {
    let position = source.position(order, 3);
    let label = source.raw(order, 3)?.unwrap_or_default();
    details.specimen_label = Some(text(&label, position)?);
    let (control, patient) = material(source, order, layout)?;
    if control {
        if !patient && layout != AstmResultLayout::SlashDilutionAndAlarmCodeWithQcLot {
            let first = source.component(order, 3, 1, 1)?.unwrap_or_default();
            if !label.is_empty() && label == first {
                details.control_name = Some(text(&label, position)?);
            }
        }
        finding(output, position, Problem::MissingSample)?;
        return Ok(false);
    }
    let first = source.component(order, 3, 1, 1)?.unwrap_or_default();
    if !patient || label != first || label.is_empty() || label.starts_with('@') {
        finding(output, position, Problem::MissingSample)?;
        return Ok(false);
    }
    Ok(true)
}

fn material(
    source: &Source<'_>,
    order: usize,
    layout: AstmResultLayout,
) -> Result<(bool, bool), Error> {
    let sample_type = source.component(order, 4, 1, 5)?;
    let action = source.repetitions(order, 12)?;
    let (control, patient) = if uses_slash(layout) {
        (
            sample_type.as_deref() == Some("QC") || action.iter().any(|entry| entry == "Q"),
            sample_type
                .as_deref()
                .is_some_and(|code| matches!(code, "S1" | "S2" | "S5")),
        )
    } else {
        (
            sample_type.as_deref() == Some("CONTROL")
                || action.get(1).is_some_and(|entry| entry == "Q"),
            sample_type.as_deref() == Some("SAMPLE"),
        )
    };
    Ok((control, patient))
}

pub(crate) fn enrich(
    source: &Source<'_>,
    index: usize,
    order: usize,
    layout: AstmResultLayout,
    part: &mut ResultPart,
    details: &mut ResultDetails,
    output: &mut Output,
) -> Result<(), Error> {
    match layout {
        AstmResultLayout::MeasurementMarker => {
            let position = source.position(index, 3);
            details.method = match source.component(index, 3, 1, 5)?.as_deref() {
                Some("M") => Some(MeasurementMethod::Measured),
                Some("C") => Some(MeasurementMethod::Calculated),
                _ => {
                    finding(output, position, Problem::UnsupportedValue)?;
                    None
                }
            };
            details.specimen_descriptor = source_text(source, order, 16)?;
            if source.raw(order, 26)?.as_deref() == Some("P") {
                details.report_status = Some(ResultStatus::Preliminary);
            }
            part.status = ResultStatus::Unknown;
            part.qc_class = QcClass::Unknown;
            record_completion(source, details, output)?;
        }
        AstmResultLayout::CaretDilutionAndAlarm
        | AstmResultLayout::SlashDilutionAndAlarmCode
        | AstmResultLayout::SlashDilutionAndAlarmCodeWithQcLot => {
            let (control, patient) = material(source, order, layout)?;
            part.qc_class = match (control, patient) {
                (true, false) => QcClass::Control,
                (false, true) => QcClass::Patient,
                (false, false) | (true, true) => QcClass::Unknown,
            };
            if part.source_status.as_str() == "C" && uses_slash(layout) {
                part.status = ResultStatus::Unknown;
            } else if layout == AstmResultLayout::CaretDilutionAndAlarm {
                part.status = match part.source_status.as_str() {
                    "X" => ResultStatus::NotPerformed,
                    "V" => ResultStatus::Final,
                    _ => ResultStatus::Unknown,
                };
            } else {
                // Initial and rerun codes do not make release claims.
                part.status = ResultStatus::Unknown;
            }
            part.dilution = dilution(source, index, layout, details)?;
            details.alarms = alarms(source, index, uses_slash(layout), output)?;
            details.specimen_descriptor = source
                .present(source.component(order, 4, 1, 5)?)
                .map(|raw| text(&raw, source.position(order, 4)))
                .transpose()?;
        }
    }
    Ok(())
}

fn record_completion(
    source: &Source<'_>,
    details: &mut ResultDetails,
    output: &mut Output,
) -> Result<(), Error> {
    let index = source.nodes.len().saturating_sub(1);
    if source.nodes.get(index).is_some_and(|node| node.name == "L") {
        details.termination = source
            .raw(index, 3)?
            .map(|raw| text(&raw, source.position(index, 3)))
            .transpose()?;
    }
    let completion = match details.termination.as_ref().map(SourceText::as_str) {
        Some("N") => ReportCompletion::Normal,
        Some("T") => ReportCompletion::SenderAborted,
        _ => ReportCompletion::Unknown,
    };
    details.completion = Some(completion);
    if completion != ReportCompletion::Normal {
        finding(
            output,
            source.position(index, 3),
            Problem::IncompleteAstmReport,
        )?;
    }
    Ok(())
}

fn source_text(
    source: &Source<'_>,
    index: usize,
    field: usize,
) -> Result<Option<SourceText>, Error> {
    source
        .present(source.raw(index, field)?)
        .map(|raw| text(&raw, source.position(index, field)))
        .transpose()
}

fn dilution(
    source: &Source<'_>,
    index: usize,
    layout: AstmResultLayout,
    details: &mut ResultDetails,
) -> Result<Option<Dilution>, Error> {
    let position = source.position(index, 3);
    let (selector, predilution) = if uses_slash(layout) {
        let raw = source.component(index, 3, 1, 4)?.unwrap_or_default();
        let Some([_, ratio, pre]) = slash_parts(&raw) else {
            return Ok(None);
        };
        (Some(ratio.to_owned()), Some(pre.to_owned()))
    } else {
        (
            source.component(index, 3, 1, 5)?,
            source.component(index, 3, 1, 6)?,
        )
    };
    details.predilution = predilution.map(|pre| text(&pre, position)).transpose()?;
    selector
        .map(|selector| {
            let profile = if uses_slash(layout) {
                analyte_astm::OrderProfile::Cobas
            } else {
                analyte_astm::OrderProfile::Elecsys
            };
            let ratio = super::dilution::decode(profile, &selector);
            Ok(Dilution {
                original: Some(text(&selector, position)?),
                factor: ratio.and_then(|ratio| Decimal::new(ratio).ok()),
                applied: DilutionApplied::Unknown,
            })
        })
        .transpose()
}

fn alarms(
    source: &Source<'_>,
    index: usize,
    coded: bool,
    output: &mut Output,
) -> Result<EvidenceList<ResultAlarm>, Error> {
    let Some(node) = source.nodes.get(index) else {
        return Ok(EvidenceList::default());
    };
    let mut alarms = Vec::new();
    for &comment in &node.comments {
        if source.raw(comment, 3)?.as_deref() != Some("I")
            || source.raw(comment, 5)?.as_deref() != Some("I")
        {
            continue;
        }
        let position = source.position(comment, 4);
        let raw = source.raw(comment, 4)?.unwrap_or_default();
        let number = source.component(comment, 4, 1, 1)?.unwrap_or_default();
        let description = source.component(comment, 4, 1, 2)?;
        let valid = if coded {
            raw == number && digits(&number, 3)
        } else {
            digits(&number, 2)
                && description.as_ref().is_some_and(|description| {
                    raw.split_once(char::from(source.component_separator))
                        == Some((number.as_str(), description.as_str()))
                })
        };
        if !valid {
            finding(output, position, Problem::UnsupportedValue)?;
            continue;
        }
        alarms.push(ResultAlarm {
            number: text(&number, position)?,
            text: description
                .map(|description| text(&description, position))
                .transpose()?,
        });
    }
    list(alarms, source.position(index, 4))
}
