//! Replacement grouping and mapping orchestration.
use crate::FieldSyntax;
use crate::source::Source;
use crate::{Error, Input, Output, Position, Problem, finding, identifier, list, present, text};
use crate::{MappedResult, UnmappedObservation, identity, result, value};
use analyte_config::Validated;
use analyte_model::*;

fn processing(source: &Source<'_>, output: &mut Output) -> Result<ProcessingMode, Error> {
    let field = match source.syntax {
        FieldSyntax::AstmLis02 => 12,
        FieldSyntax::Hl7V2 => 11,
    };
    match source.component(0, field, 1, 1)?.as_deref() {
        Some("P") => Ok(ProcessingMode::Production),
        Some("T") => Ok(ProcessingMode::Training),
        Some("D") => Ok(ProcessingMode::Debug),
        Some("Q") if source.syntax == FieldSyntax::AstmLis02 => Ok(ProcessingMode::QualityControl),
        _ => {
            finding(
                output,
                source.position(0, field),
                Problem::UnknownProcessing,
            )?;
            // There is no Unknown model variant. Refuse rather than invent a mode.
            Err(Error::Structure {
                position: source.position(0, field),
                reason: "unknown processing mode",
            })
        }
    }
}

pub(crate) fn message(
    source: &Source<'_>,
    config: &Validated,
    input: Input,
) -> Result<Output, Error> {
    let machine = config.effective().machine();
    if source.nodes.len() > machine.syntax.records {
        return Err(Error::Limit {
            position: source.position(0, 0),
            resource: "records",
        });
    }
    let mut output = Output {
        results: Vec::new(),
        unmapped: Vec::new(),
        findings: Vec::new(),
        comments: Vec::new(),
        evidence_budget: 0,
    };
    let mode = processing(source, &mut output)?;
    for (index, node) in source.nodes.iter().enumerate() {
        if node.name == "C" || node.name == "NTE" {
            crate::reserve_evidence(&mut output, source, index)?;
            let position = source.position(index, if node.name == "C" { 4 } else { 3 });
            let parent = node.parent.ok_or(Error::Structure {
                position,
                reason: "note has no parent",
            })?;
            output.comments.push(crate::SourceComment {
                position,
                parent_path: result::source_path(source, parent)?,
                text: text(
                    &source.raw(index, position.field)?.unwrap_or_default(),
                    position,
                )?,
            });
        }
    }
    // Group only resolved identities, within the same source order occurrence.
    let mut groups: Vec<(usize, Option<ObservationIdentity>)> = Vec::new();
    for (index, node) in source.nodes.iter().enumerate() {
        if node.name != "R" && node.name != "OBX" {
            continue;
        }
        crate::reserve_evidence(&mut output, source, index)?;
        let position = source.position(index, 0);
        let finding_start = output.findings.len();
        let astm = source.syntax == FieldSyntax::AstmLis02;
        let value_field = if astm { 4 } else { 5 };
        let values = value::values(source, index, value_field, &mut output)?;
        let sample = identity::sample(source, index, config, &mut output)?;
        let standard_code = present(source.component(index, 3, 1, 1)?);
        let manufacturer_code = if astm {
            present(source.component(index, 3, 1, 4)?)
        } else {
            None
        };
        let system = if astm {
            None
        } else {
            present(source.component(index, 3, 1, 3)?)
        };
        // LIS02 section 5.6.1 provides both a universal and a manufacturer identifier.
        // The explicit table must select exactly one interpretation if both occur.
        let codes = [standard_code.as_deref(), manufacturer_code.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let translation = crate::test_translation(machine.tests(), &codes, system.as_deref());
        let source_code = match (standard_code.as_deref(), system.as_deref(), translation) {
            (Some(code), Some(system), _) => Some(NamespacedId {
                value: identifier(code, position)?,
                namespace: identifier(system, position)?,
            }),
            (Some(_), None, Some(entry)) | (None, None, Some(entry)) => Some(NamespacedId {
                value: entry.machine_code.clone(),
                namespace: entry.machine_system.clone(),
            }),
            (Some(_), None, None) | (None, None, None) | (None, Some(_), _) => None,
        };
        if translation.is_none() {
            finding(&mut output, source.position(index, 3), Problem::UnknownTest)?;
        }
        let Some(source_code) = source_code else {
            output
                .unmapped
                .push(UnmappedObservation { position, values });
            continue;
        };
        let test = translation.map_or_else(
            || source_code.clone(),
            |t| NamespacedId {
                namespace: t.app_system.clone(),
                value: t.app_code.clone(),
            },
        );

        let observation = identity::observation(source, index, config, source_code, &mut output)?;
        let status_field = if astm { 9 } else { 11 };
        let status_text = source.raw(index, status_field)?.unwrap_or_default();
        let correction = crate::field_mappings(config)?
            .identity
            .correction_status_codes
            .iter()
            .any(|code| code.as_str() == status_text);
        let part = result::part(source, index, config, input, test, values, &mut output)?;
        let order = source
            .ancestors(index)?
            .into_iter()
            .rfind(|i| {
                source
                    .nodes
                    .get(*i)
                    .is_some_and(|n| n.name == "O" || n.name == "OBR" || n.name == "SPM")
            })
            .unwrap_or(index);
        let existing = observation.as_ref().and_then(|key| {
            groups
                .iter()
                .position(|(owner, other)| *owner == order && Some(key) == other.as_ref())
        });
        if let Some(group_index) = existing {
            let result = output
                .results
                .get_mut(group_index)
                .ok_or(Error::Structure {
                    position,
                    reason: "replacement group missing",
                })?;
            if result.content.sample != sample {
                return Err(Error::Structure {
                    position,
                    reason: "same replacement identity reports different sample identities",
                });
            }
            let mut parts = result.content.parts.as_slice().to_vec();
            if parts.len() == 256 {
                return Err(Error::Limit {
                    position,
                    resource: "replacement parts",
                });
            }
            parts.push(part);
            result.content.parts = nonempty(parts, position)?;
            let mut corrections = result.corrections.as_slice().to_vec();
            corrections.push(correction);
            result.corrections = list(corrections, position)?;
        } else {
            let identity = match observation.clone() {
                Some(observation) if translation.is_some() => {
                    IdentityStatus::Resolved(ResultIdentity {
                        observation,
                        revision: input.revision,
                    })
                }
                Some(_) | None => IdentityStatus::Unresolved(
                    Reason::new(
                        &output
                            .findings
                            .iter()
                            .skip(finding_start)
                            .rev()
                            .find_map(|f| match f.problem {
                                Problem::MissingIdentity { setting } => Some(format!(
                                    "missing identity {setting} at record {} field {}",
                                    f.position.record, f.position.field
                                )),
                                Problem::MissingSample
                                | Problem::UnknownTest
                                | Problem::UnsupportedValue
                                | Problem::UnknownProcessing
                                | Problem::ConflictingClassification
                                | Problem::InvalidTime
                                | Problem::NoObservations => None,
                            })
                            .unwrap_or_else(|| "unknown test mapping".into()),
                    )
                    .map_err(|source| Error::Text { position, source })?,
                ),
            };
            // Keep group slots aligned with result slots, including unresolved units.
            groups.push((order, observation));
            output.results.push(MappedResult {
                content: ResultContent {
                    receipt: input.receipt,
                    sample,
                    processing: mode,
                    identity,
                    parts: nonempty(vec![part], position)?,
                },
                corrections: list(vec![correction], position)?,
            });
        }
    }
    if output.results.is_empty() && output.unmapped.is_empty() {
        finding(&mut output, source.position(0, 0), Problem::NoObservations)?;
    }
    Ok(output)
}

fn nonempty(
    parts: Vec<ResultPart>,
    position: Position,
) -> Result<NonEmptyEvidenceList<ResultPart>, Error> {
    NonEmptyEvidenceList::new(list(parts, position)?).map_err(|_| Error::Structure {
        position,
        reason: "replacement unit has no parts",
    })
}
