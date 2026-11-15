//! Bounded app-to-AWOS adaptation; protocol rendering stays in analyte-hl7.
use super::response_metadata;
use crate::{Error, invalid};
use analyte_hl7::{Message, Options, law};
use analyte_store::MachineOrder;
use std::sync::Arc;

pub(super) struct Command {
    pub(super) app: analyte_mapping::AppOrder,
    pub(super) settings: Arc<analyte_config::Orders>,
    pub(super) work: Arc<analyte_config::LawWork>,
    pub(super) priority: bool,
    pub(super) specimen: bool,
}
struct WireAwos {
    id: String,
    specimen: String,
    container: String,
    specimen_type: String,
    test: String,
    priority: Option<String>,
}

fn plain(value: &str) -> Result<&str, Error> {
    if value.is_empty()
        || !value.is_ascii()
        || value
            .bytes()
            .any(|b| b.is_ascii_control() || b"|^~\\&".contains(&b))
    {
        return Err(invalid(
            "LAW outbound value cannot be represented without changing its identity",
        ));
    }
    Ok(value)
}

pub(super) fn render(
    commands: &[(&Command, &MachineOrder)],
    options: Options,
) -> Result<Message, Error> {
    let (first, _) = commands.first().ok_or("LAW work empty")?;
    let mut wire = Vec::new();
    for (command, record) in commands {
        let sample = plain(command.app.sample.as_str())?;
        let specimen = command
            .app
            .specimen_type
            .as_ref()
            .ok_or("LAW specimen type missing")?;
        let mapped = command
            .settings
            .specimen_types
            .as_ref()
            .and_then(|types| types.iter().find(|entry| entry.app == *specimen))
            .ok_or("LAW specimen type unmapped")?;
        let priority = if command.priority {
            let app = match command.app.priority {
                analyte_mapping::AppOrderPriority::Routine => "routine",
                analyte_mapping::AppOrderPriority::Urgent => "urgent",
                analyte_mapping::AppOrderPriority::Stat => "stat",
            };
            Some(format!(
                "{}^^HL70485",
                plain(
                    command
                        .settings
                        .priorities
                        .iter()
                        .find(|p| p.app.as_str() == app)
                        .ok_or("LAW priority unmapped")?
                        .machine
                        .as_str()
                )?
            ))
        } else {
            None
        };
        for item in &record.items {
            if wire.len() >= options.limits.segments {
                return Err(invalid("LAW query AWOS limit exceeded"));
            }
            let [test] = item.tests.as_slice() else {
                return Err(invalid("LAW item must cover one AWOS test"));
            };
            let mapped_test = command
                .settings
                .tests
                .iter()
                .find(|entry| entry.app == *test)
                .ok_or("LAW test unmapped")?;
            wire.push(WireAwos {
                // C.6 manager-authored AWOS supports EI.1 only. The shared store
                // still retains the explicit logical namespace beside this identity.
                id: item.placer.clone(),
                specimen: format!("{sample}&{}", command.work.specimen_namespace.as_str()),
                container: sample.to_owned(),
                specimen_type: format!(
                    "{}^^{}",
                    plain(mapped.machine.as_str())?,
                    command.work.specimen_system.as_str()
                ),
                test: mapped_test.machine_components.join("^"),
                priority: priority.clone(),
            });
        }
    }
    let awos = wire
        .iter()
        .map(|item| {
            Ok(law::Awos::new(
                item.id.as_bytes(),
                item.specimen.as_bytes(),
                item.specimen_type.as_bytes(),
                first.work.specimen_role.as_str().as_bytes(),
                item.container.as_bytes(),
                item.test.as_bytes(),
                law::WorkAction::New,
            )?
            .with_instructions(law::Instructions {
                priority: item.priority.as_ref().map(String::as_bytes),
                ..law::Instructions::default()
            }))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let capabilities = law::Capabilities {
        priority: first.priority,
        specimen: first.specimen,
        ..law::Capabilities::default()
    };
    response_metadata(|response| {
        Ok(law::work_order(
            &awos,
            None,
            address(&first.work),
            response,
            capabilities,
            options,
        )?)
    })
}

pub(super) fn address(config: &analyte_config::LawWork) -> law::Address<'_> {
    law::Address {
        application: config.application.as_str().as_bytes(),
        facility: config.facility.as_str().as_bytes(),
        peer_application: config.peer_application.as_str().as_bytes(),
        peer_facility: config.peer_facility.as_str().as_bytes(),
    }
}
