//! Bounded app-to-AWOS adaptation; protocol rendering stays in analyte-hl7.
use crate::{Error, invalid};
use analyte_hl7::{Message, Options, law};
use analyte_store::MachineOrder;
use std::sync::Arc;

pub(super) struct Command {
    pub(super) app: analyte_mapping::AppOrder,
    pub(super) settings: Arc<analyte_config::LawOrders>,
    pub(super) priority: bool,
    pub(super) specimen: bool,
}
impl Command {
    pub(super) fn new(
        machine: &crate::config::Machine,
        app: &analyte_contract::Order,
    ) -> Result<Self, Error> {
        let law = super::super::settings::law(machine)?;
        let peer = law.policy.peer.capabilities();
        Ok(Self {
            app: super::super::app_order(app)?,
            settings: Arc::new(
                law.orders
                    .as_ref()
                    .ok_or("LAW order mappings absent")?
                    .clone(),
            ),
            priority: peer.priority,
            specimen: peer.specimen,
        })
    }
}
struct WireAwos {
    id: String,
    container: String,
    fields: law::WorkFields,
}

fn plain(value: &str) -> Result<&str, Error> {
    if !analyte_hl7::is_safe_field(
        value.as_bytes(),
        usize::MAX,
        analyte_hl7::FieldSyntax::Plain,
    ) {
        return Err(invalid(
            "LAW outbound value cannot be represented without changing its identity",
        ));
    }
    Ok(value)
}

pub(super) fn render(
    commands: &[(&Command, &MachineOrder)],
    action: law::WorkAction,
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
            .iter()
            .find(|entry| entry.app == *specimen)
            .ok_or("LAW specimen type unmapped")?;
        let priority = if command.priority && action == law::WorkAction::New {
            let app = match command.app.priority {
                analyte_mapping::AppOrderPriority::Routine => "routine",
                analyte_mapping::AppOrderPriority::Urgent => "urgent",
                analyte_mapping::AppOrderPriority::Stat => "stat",
            };
            Some(plain(
                command
                    .settings
                    .priorities
                    .iter()
                    .find(|p| p.app.as_str() == app)
                    .ok_or("LAW priority unmapped")?
                    .machine
                    .as_str(),
            )?)
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
                container: sample.to_owned(),
                fields: {
                    let [code, text, system] = mapped_test.machine_components.as_slice() else {
                        return Err(invalid("LAW test components"));
                    };
                    law::WorkFields::new(
                        sample,
                        command.settings.work.specimen_namespace.as_str(),
                        law::WorkCode {
                            code: plain(mapped.machine.as_str())?,
                            text: "",
                            system: command.settings.work.specimen_system.as_str(),
                        },
                        law::WorkCode { code, text, system },
                        priority,
                    )?
                },
            });
        }
    }
    let awos = wire
        .iter()
        .map(|item| {
            Ok(item.fields.awos(
                item.id.as_bytes(),
                first.settings.work.specimen_role.as_str().as_bytes(),
                item.container.as_bytes(),
                action,
            )?)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let capabilities = law::Capabilities {
        priority: first.priority,
        specimen: first.specimen,
        ..law::Capabilities::default()
    };
    crate::hl7_response::response_metadata(crate::hl7_response::Profile::Law).build(|response| {
        Ok(law::work_order(
            &awos,
            None,
            address(&first.settings.work),
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
