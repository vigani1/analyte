//! POCT original observation evidence and configured result mapping; no I/O.
mod observations;
use crate::inbound::evidence::{Error, map_result, map_test, map_units};
use crate::{MappedResult, Output, Position};
use analyte_config::Validated;
use analyte_model::*;
pub use observations::{MappedObservation, MappingError, map as map_observations};
/// Applies explicit POCT code tables and identity policy to faithfully mapped observations.
/// # Errors
/// Preserves syntax, model, identity and mapping failures.
pub fn map(
    machine: &Validated,
    receipt: ReceiptId,
    message: &analyte_poct::Message,
    time: jiff::Timestamp,
    device: Option<&str>,
) -> Result<Output, Error> {
    let observations = map_observations(message, time)?;
    if observations.is_empty() {
        return Err(Error::Context(
            "POCT observations transfer contains no observations",
        ));
    }
    let mut output = Output::default();
    for observation in observations {
        let mut part = observation.part;
        let mut identity = observation.identity;
        let mut processing = ProcessingMode::Unknown;
        let mut correction = false;
        if let analyte_config::Stack::Poct(settings) = machine.effective().stack()
            && let Some(policy) = &settings.policy().identity
        {
            processing = policy.processing;
            let service_index = part
                .source_path
                .as_slice()
                .iter()
                .find(|node| node.kind.as_str() == "SVC")
                .map(|node| node.occurrence.saturating_sub(1));
            let field = |name: &str| -> Option<&str> {
                message
                    .document()
                    .elements()
                    .iter()
                    .find(|e| e.parent() == service_index && e.name() == name)
                    .and_then(|e| {
                        e.attributes()
                            .iter()
                            .find(|(k, _)| k == "V")
                            .map(|(_, v)| v.as_str())
                    })
            };
            correction = field("SVC.reason_cd") == Some("EDT");
            let observed = field("SVC.observation_dttm").filter(|v| !v.is_empty());
            let sequence = field("SVC.sequence_nbr").filter(|v| !v.is_empty());
            if let (Some(device), Some(observed)) = (device.filter(|v| !v.is_empty()), observed)
                && (!policy.sequence_number || sequence.is_some())
            {
                let mut run = vec![observed];
                if policy.sequence_number
                    && let Some(sequence) = sequence
                {
                    run.push(sequence);
                }
                let code = NamespacedId {
                    namespace: Identifier::new(
                        observation
                            .source_code
                            .system
                            .as_deref()
                            .or(observation.source_code.system_name.as_deref())
                            .ok_or(Error::Context("POCT analyte namespace missing"))?,
                    )?,
                    value: Identifier::new(&observation.source_code.value)?,
                };
                identity = IdentityStatus::Resolved(ResultIdentity {
                    observation: ObservationIdentity {
                        code,
                        sub_id: SourceText::new("")?,
                        run: crate::identity_run(&run)?,
                        order: NamespacedId {
                            namespace: Identifier::new("poct-device")?,
                            value: Identifier::new(device)?,
                        },
                    },
                    revision: Revision(0),
                });
            } else {
                identity = IdentityStatus::Unresolved(Reason::new(
                    "POCT configured device/time/sequence identity evidence missing",
                )?);
            }
        }
        let system = observation
            .source_code
            .system
            .as_deref()
            .or(observation.source_code.system_name.as_deref());
        let Some(test) = map_test(machine, &observation.source_code.value, system) else {
            output.unmapped_test(
                Position {
                    record: observation.source_index.saturating_add(1),
                    field: 0,
                },
                part.values,
            )?;
            continue;
        };
        part.test = test;
        map_units(machine, &mut part)?;
        let mut content = map_result(receipt, observation.sample, part, identity)?;
        content.processing = processing;
        output.results.push(MappedResult {
            content,
            corrections: EvidenceList::new(vec![correction])?,
        });
    }
    Ok(output)
}
