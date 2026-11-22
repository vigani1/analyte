//! Groups replacement results and coordinates result mapping.
use crate::FieldSyntax;
use crate::MappedResult;
use crate::RecordSettings;
use crate::inbound::source::Source;
use crate::inbound::{identity, result, value};
use crate::{Error, Input, Output, Position, Problem, finding, identifier, list, text};
use analyte_model::{
    Decimal, Dilution, DilutionApplied, EvidenceList, IdentityStatus, NamespacedId,
    NonEmptyEvidenceList, ObservationIdentity, ProcessingMode, Reason, ResultContent,
    ResultIdentity, ResultPart, ResultValue, SampleIdentity,
};

fn processing(source: &Source<'_>, output: &mut Output) -> Result<ProcessingMode, Error> {
    let (field, mode) = match source.syntax {
        // LIS02 section 6.12: H-12 is one whole code, so a composite or
        // repeated label never counts as its first component.
        FieldSyntax::AstmLis02 => {
            use analyte_astm::ProcessingId;
            let raw = source.raw(0, 12)?.unwrap_or_default();
            let mode = match ProcessingId::from_bytes(raw.as_bytes()) {
                ProcessingId::Production => Some(ProcessingMode::Production),
                ProcessingId::Training => Some(ProcessingMode::Training),
                ProcessingId::Debug => Some(ProcessingMode::Debug),
                ProcessingId::QualityControl => Some(ProcessingMode::QualityControl),
                ProcessingId::Other(_) => None,
            };
            (12, mode)
        }
        FieldSyntax::Hl7V2 => {
            let mode = match source.component(0, 11, 1, 1)?.as_deref() {
                Some("P") => Some(ProcessingMode::Production),
                Some("T") => Some(ProcessingMode::Training),
                Some("D") => Some(ProcessingMode::Debug),
                _ => None,
            };
            (11, mode)
        }
    };
    if let Some(mode) = mode {
        return Ok(mode);
    }
    finding(
        output,
        source.position(0, field),
        Problem::UnknownProcessing,
    )?;
    // These record standards require an explicit mode; Unknown is not a fallback.
    Err(Error::Structure {
        position: source.position(0, field),
        reason: "unknown processing mode",
    })
}

pub(crate) fn message(
    source: &Source<'_>,
    config: &RecordSettings<'_>,
    input: Input<'_>,
    law: Option<&analyte_hl7::law::Results<'_>>,
) -> Result<Output, Error> {
    if source.nodes.len() > config.limits.records() {
        return Err(Error::Limit {
            position: source.position(0, 0),
            resource: "records",
        });
    }
    let mut output = Output::default();
    // Join source occurrences, never clinical identities or buffer addresses.
    let law = law.map(|results| {
        results
            .observations()
            .iter()
            .map(|observation| (observation.segment_index(), observation))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    let mode = processing(source, &mut output)?;
    map_comments(source, &mut output)?;
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
        let Some(codes) = map_codes(source, index, config, &mut output)? else {
            // The component_parts finding already names the cause.
            output.uncoded.push(crate::UncodedObservation {
                position: source.position(index, 3),
                values,
            });
            continue;
        };
        let Some(test) = map_test(source, index, config, &codes, &values, &mut output)? else {
            output.missing_test(source.position(index, 3), values)?;
            continue;
        };
        let observation = identity::observation(
            source,
            index,
            config,
            test.clone(),
            &mut output,
            law.as_ref(),
        )?;
        let status_field = if astm { 9 } else { 11 };
        let status_text = source.raw(index, status_field)?.unwrap_or_default();
        let correction = config
            .mappings
            .identity
            .correction_status_codes
            .iter()
            .any(|code| code.as_str() == status_text);
        let mut part = result::part(source, index, config, input, test, values, &mut output)?;
        part.dilution = codes.dilution;
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
        append_result(
            &mut output,
            &mut groups,
            input,
            mode,
            MappedPart {
                order,
                observation,
                sample,
                part,
                correction,
                position,
                finding_start,
            },
        )?;
    }
    if output.results.is_empty() && output.uncoded.is_empty() && law.is_none() {
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

fn map_comments(source: &Source<'_>, output: &mut Output) -> Result<(), Error> {
    for (index, node) in source.nodes.iter().enumerate() {
        if node.name == "C" || node.name == "NTE" {
            crate::reserve_evidence(output, source, index)?;
            let position = source.position(index, if node.name == "C" { 4 } else { 3 });
            let parent = node.parent.ok_or(Error::Structure {
                position,
                reason: "note has no parent",
            })?;
            let original = source.raw(index, position.field)?.unwrap_or_default();
            let problem = if node.name == "NTE"
                && original.contains(char::from(source.component_separator))
            {
                Some(Problem::UnsupportedFormatting)
            } else {
                source.text_problem(&original)
            };
            if let Some(problem) = problem {
                finding(output, position, problem)?;
            }
            output.comments.push(crate::SourceComment {
                position,
                parent_path: result::source_path(source, parent)?,
                text: text(&original, position)?,
            });
        }
    }
    Ok(())
}

struct Codes {
    code: Option<String>,
    system: Option<String>,
    dilution: Option<Dilution>,
}

// LIS02 section 5.6.1: R.3 holds the test ID; the definition names which
// component carries the machine's code. HL7 OBX-3 carries the code in CE.1
// and the coding system the machine names in CE.3.
fn map_codes(
    source: &Source<'_>,
    index: usize,
    config: &RecordSettings<'_>,
    output: &mut Output,
) -> Result<Option<Codes>, Error> {
    let Some(component) = config.mappings.test_component else {
        return Ok(Some(Codes {
            code: source.present(source.component(index, 3, 1, 1)?),
            system: source.present(source.component(index, 3, 1, 3)?),
            dilution: None,
        }));
    };
    let component = usize::from(component.number());
    let Some(parts) = &config.mappings.component_parts else {
        return Ok(Some(Codes {
            code: source.present(source.component(index, 3, 1, component)?),
            system: None,
            dilution: None,
        }));
    };
    let original = source
        .component(index, 3, 1, component)?
        .unwrap_or_default();
    // The message may declare non-default delimiters: never split one as data.
    let header = source.nodes.first().and_then(|n| match n.record {
        crate::inbound::source::Record::Astm(r) => Some(r.raw()),
        crate::inbound::source::Record::Hl7(_) => None,
    });
    let collision = header
        .and_then(|h| h.get(1..5))
        .is_some_and(|h| h.contains(&(parts.separator as u8)))
        && parts.separator.is_ascii();
    let split: Vec<_> = original.split(parts.separator).collect();
    if collision || split.len() != usize::from(parts.count) {
        finding(output, source.position(index, 3), Problem::ComponentParts)?;
        return Ok(None);
    }
    let code = split
        .get(usize::from(parts.code.saturating_sub(1)))
        .map(|s| (*s).to_owned())
        .filter(|s| !s.is_empty());
    let mut dilution = None;
    if let Some(part) = parts
        .dilution
        .and_then(|i| split.get(usize::from(i.saturating_sub(1))))
    {
        let mut applied = DilutionApplied::Unknown;
        if let Some(setting) = &parts.applied {
            let reported = source.selected(index, &setting.field)?;
            if reported.as_deref() == Some(setting.yes.as_str()) {
                applied = DilutionApplied::Yes;
            }
            if reported.as_deref() == Some(setting.no.as_str()) {
                applied = DilutionApplied::No;
            }
        }
        dilution = Some(Dilution {
            original: Some(text(part, source.position(index, 3))?),
            factor: Decimal::new(part).ok(),
            applied,
        });
    }
    Ok(Some(Codes {
        code,
        system: None,
        dilution,
    }))
}

struct MappedPart {
    order: usize,
    observation: Option<ObservationIdentity>,
    sample: Option<SampleIdentity>,
    part: ResultPart,
    correction: bool,
    position: Position,
    finding_start: usize,
}

fn append_result(
    output: &mut Output,
    groups: &mut Vec<(usize, Option<ObservationIdentity>)>,
    input: Input<'_>,
    mode: ProcessingMode,
    mapped: MappedPart,
) -> Result<(), Error> {
    let MappedPart {
        order,
        observation,
        sample,
        part,
        correction,
        position,
        finding_start,
    } = mapped;
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
            Some(observation) => IdentityStatus::Resolved(ResultIdentity {
                observation,
                revision: input.revision,
            }),
            None => {
                // Identity mapping returns nothing only after a MissingIdentity finding.
                let reason = output
                    .findings
                    .iter()
                    .skip(finding_start)
                    .rev()
                    .find_map(|f| match f.problem {
                        Problem::MissingIdentity { setting } => {
                            Some(crate::Unidentified::Missing {
                                setting,
                                position: f.position,
                            })
                        }
                        Problem::ValueKindMismatch { .. }
                        | Problem::ComponentParts
                        | Problem::MissingSample
                        | Problem::MissingTest
                        | Problem::UnsupportedValue
                        | Problem::UnsupportedFormatting
                        | Problem::UnsupportedEscape
                        | Problem::UnknownProcessing
                        | Problem::ConflictingClassification
                        | Problem::InvalidTime
                        | Problem::NoObservations
                        | Problem::UnsupportedLawStatusOnly
                        | Problem::UnqualifiedHl7Report
                        | Problem::DepartureConflict
                        | Problem::DerivedStatusContract => None,
                    })
                    .ok_or(Error::Structure {
                        position,
                        reason: "unresolved identity has no missing-identity finding",
                    })?;
                IdentityStatus::Unresolved(
                    Reason::new(&reason.to_string())
                        .map_err(|source| Error::Text { position, source })?,
                )
            }
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
    Ok(())
}

// The code passes through exactly as sent (ADR 0029). Its namespace is the
// coding system the message names, else the machine's declared test_system.
fn map_test(
    source: &Source<'_>,
    index: usize,
    config: &RecordSettings<'_>,
    codes: &Codes,
    values: &EvidenceList<ResultValue>,
    output: &mut Output,
) -> Result<Option<NamespacedId>, Error> {
    let machine = config.mappings;
    let value_field = if source.syntax == FieldSyntax::AstmLis02 {
        4
    } else {
        5
    };
    let position = source.position(index, 3);
    let Some(code) = codes.code.as_deref() else {
        return Ok(None);
    };
    let test = NamespacedId {
        value: identifier(code, position)?,
        namespace: match codes.system.as_deref() {
            Some(system) => identifier(system, position)?,
            None => machine.test_system.clone(),
        },
    };
    value::check_kind(
        values,
        crate::value_kind(machine, &test),
        source.position(index, value_field),
        output,
    )?;
    Ok(Some(test))
}
