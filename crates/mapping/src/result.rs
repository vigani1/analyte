//! Source evidence accompanying each observation, without clinical decisions.
use crate::FieldSyntax;
use crate::source::Source;
use crate::time;
use crate::{Error, Input, Output, Problem, finding, identifier, list, present, text};
use analyte_config::Classification;
use analyte_config::Validated;
use analyte_model::*;

pub(crate) fn part(
    source: &Source<'_>,
    index: usize,
    config: &Validated,
    input: Input,
    test: NamespacedId,
    values: EvidenceList<ResultValue>,
    output: &mut Output,
) -> Result<ResultPart, Error> {
    let astm = source.syntax == FieldSyntax::AstmLis02;
    let status_text = source
        .raw(index, if astm { 9 } else { 11 })?
        .unwrap_or_default();
    let position = source.position(index, 0);
    let units_field = if astm { 5 } else { 6 };
    let units = present(source.raw(index, units_field)?)
        .map(|value| {
            let original = text(&value, source.position(index, units_field))?;
            match crate::unit_translation(config.effective().machine().units(), &value) {
                Some(entry) => Units::mapped(
                    original.clone(),
                    UnitMapping {
                        source: original,
                        target: entry.ucum.clone(),
                    },
                )
                .map_err(|_| Error::Structure {
                    position,
                    reason: "unit mapping source mismatch",
                }),
                None => Ok(Units::original(original)),
            }
        })
        .transpose()?;
    let mut qc_class = QcClass::Unknown;
    let mut classified = false;
    for rule in &crate::field_mappings(config)?.qc {
        if source.selected(index, &rule.field)?.as_deref() == Some(rule.value.as_str()) {
            let selected = match rule.classification {
                Classification::Patient => QcClass::Patient,
                Classification::Control => QcClass::Control,
                Classification::Calibration => QcClass::Calibration,
                Classification::Unknown => QcClass::Unknown,
            };
            if classified && selected != qc_class {
                finding(output, position, Problem::ConflictingClassification)?;
                qc_class = QcClass::Unknown;
                break;
            }
            qc_class = selected;
            classified = true;
        }
    }
    let ancestors = source.ancestors(index)?;
    let path = source_path(source, index)?;
    let mut times = Vec::new();
    for ancestor in &ancestors {
        let Some(node) = source.nodes.get(*ancestor) else {
            continue;
        };
        let fields: &[usize] = match node.name.as_str() {
            "H" => &[14],
            "O" => &[7, 8, 9, 15, 23],
            "R" => &[10, 12, 13],
            "MSH" => &[7],
            "OBR" => &[7, 8, 14, 22],
            "OBX" => &[12, 14, 19],
            "SPM" => &[17, 18, 19],
            _ => &[],
        };
        for field in fields {
            if let Some(raw) = present(source.component(*ancestor, *field, 1, 1)?) {
                let mut time = time::parse(&raw, source.position(*ancestor, *field))?;
                if let Some(original) = source.raw(*ancestor, *field)? {
                    time.original = text(&original, source.position(*ancestor, *field))?;
                }
                if !astm && let Some(degree) = present(source.component(*ancestor, *field, 1, 2)?) {
                    let declared = match degree.as_str() {
                        "Y" => TimePrecision::Year,
                        "L" => TimePrecision::Month,
                        "D" => TimePrecision::Day,
                        "H" => TimePrecision::Hour,
                        "M" => TimePrecision::Minute,
                        "S" => TimePrecision::Second,
                        _ => TimePrecision::Unknown,
                    };
                    // A deprecated TS precision indicator cannot make invalid or
                    // incomplete timestamp digits into a valid source time.
                    let rank = |precision| match precision {
                        TimePrecision::Unknown => 0,
                        TimePrecision::Year => 1,
                        TimePrecision::Month => 2,
                        TimePrecision::Day => 3,
                        TimePrecision::Hour => 4,
                        TimePrecision::Minute => 5,
                        TimePrecision::Second => 6,
                        TimePrecision::FractionalSecond => 7,
                    };
                    time.precision = if rank(declared) <= rank(time.precision) {
                        declared
                    } else {
                        TimePrecision::Unknown
                    };
                    if time.precision != TimePrecision::Second {
                        time.parsed = None;
                    }
                }
                if time.precision == TimePrecision::Unknown {
                    finding(
                        output,
                        source.position(*ancestor, *field),
                        Problem::InvalidTime,
                    )?;
                }
                times.push(NamedSourceTime {
                    role: identifier(&format!("{}.{field}", node.name), position)?,
                    time,
                });
            }
        }
    }
    let mut comments = Vec::new();
    for ancestor in &ancestors {
        let Some(node) = source.nodes.get(*ancestor) else {
            continue;
        };
        for comment_index in &node.comments {
            if comments.len() == 256 {
                return Err(Error::Limit {
                    position,
                    resource: "comments",
                });
            }
            if let Some(raw) = source.raw(*comment_index, if astm { 4 } else { 3 })? {
                comments.push(text(
                    &raw,
                    source.position(*comment_index, if astm { 4 } else { 3 }),
                )?);
            }
        }
    }
    let flags_field = if astm { 7 } else { 8 };
    let flags = source
        .repetitions(index, flags_field)?
        .into_iter()
        .map(|s| text(&s, source.position(index, flags_field)))
        .collect::<Result<Vec<_>, _>>()?;
    let range_field = if astm { 6 } else { 7 };
    Ok(ResultPart {
        test,
        values,
        units,
        dilution: None,
        flags: list(flags, position)?,
        reference_range: present(source.raw(index, range_field)?)
            .map(|s| text(&s, source.position(index, range_field)))
            .transpose()?,
        comments: list(comments, position)?,
        qc_class,
        source_status: text(&status_text, position)?,
        status: status(&status_text, astm),
        source_path: path,
        times: list(times, position)?,
        received_at: input.received_at,
    })
}

pub(crate) fn source_path(
    source: &Source<'_>,
    index: usize,
) -> Result<EvidenceList<SourceNode>, Error> {
    let mut path = Vec::new();
    let astm = source.syntax == FieldSyntax::AstmLis02;
    for ancestor in source.ancestors(index)? {
        let position = source.position(ancestor, 0);
        let node = source.nodes.get(ancestor).ok_or(Error::Structure {
            position,
            reason: "missing hierarchy node",
        })?;
        let status_field = match node.name.as_str() {
            "H" => Some(12),
            "O" => Some(26),
            "R" => Some(9),
            "MSH" | "OBX" => Some(11),
            "OBR" => Some(25),
            "ORC" => Some(5),
            _ => None,
        };
        path.push(SourceNode {
            occurrence: ancestor.saturating_add(1),
            kind: identifier(&node.name, position)?,
            sequence: if node.name == "H" || node.name == "MSH" || node.name == "ORC" {
                None
            } else {
                source
                    .raw(ancestor, if astm { 2 } else { 1 })?
                    .map(|s| text(&s, position))
                    .transpose()?
            },
            status: status_field
                .map(|f| source.raw(ancestor, f))
                .transpose()?
                .flatten()
                .map(|s| text(&s, position))
                .transpose()?,
        });
    }
    list(path, source.position(index, 0))
}

fn status(code: &str, astm: bool) -> ResultStatus {
    // LIS02 section 9.9 and HL7 table 0085: unmapped meanings stay Unknown,
    // with the exact source code retained independently.
    match code {
        "C" => ResultStatus::Corrected,
        "P" | "S" => ResultStatus::Preliminary,
        "F" => ResultStatus::Final,
        "X" => ResultStatus::NotPerformed,
        "I" => ResultStatus::Pending,
        "R" if !astm => ResultStatus::Preliminary,
        "D" | "W" if !astm => ResultStatus::Withdrawn,
        "U" if !astm => ResultStatus::Final,
        _ => ResultStatus::Unknown,
    }
}
