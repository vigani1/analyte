//! Maps outgoing LAW codes with supplied identities (TF-2b C.6/C.7/C.9).
use super::WireError as Error;
use analyte_hl7::{Message, Options, law};
use std::sync::Arc;
/// Pure mapper for one app-authorized LAW command.
pub struct Command {
    app: crate::orders::AppOrder,
    settings: Arc<analyte_config::LawOrders>,
    priority: bool,
    specimen: bool,
}
/// Captures LAW mappings and the peer's explicitly configured capabilities.
/// # Errors
/// Preserves disabled LAW order capability errors.
pub fn map_order(
    law: &analyte_config::LawStack,
    app: &analyte_contract::Order,
) -> Result<Command, Error> {
    let peer = law.policy().peer().capabilities();
    Ok(Command {
        app: crate::orders::app_order(app)?,
        settings: Arc::new(law.orders().ok_or(Error::MappingsAbsent)?.clone()),
        priority: peer.instructions.priority,
        specimen: peer.content.specimen,
    })
}
impl Command {
    /// Exact app tests for the host's atomic AWOS reservation.
    #[must_use]
    pub fn tests(&self) -> &[analyte_model::NamespacedId] {
        &self.app.tests
    }
    /// Explicit assigning namespace retained beside each store-generated identity.
    #[must_use]
    pub fn placer_namespace(&self) -> &analyte_model::Identifier {
        self.settings.placer_namespace()
    }
}
struct WireAwos {
    id: String,
    container: String,
    fields: law::WorkFields,
}

fn validate_field(value: &str) -> Result<&str, Error> {
    if !analyte_hl7::is_safe_field(
        value.as_bytes(),
        usize::MAX,
        analyte_hl7::FieldSyntax::Plain,
    ) {
        return Err(Error::UnrepresentableIdentity);
    }
    Ok(value)
}

/// Maps every reserved AWOS using the explicit priority translation; test and
/// specimen codes go out as written, in the machine's own code systems.
/// # Errors
/// Preserves foreign-test, unmapped-code, repertoire, cardinality and LAW-profile failures.
pub fn map_work(
    commands: &[(&Command, &[crate::orders::OutboundItem<'_>])],
    action: law::WorkAction,
    options: Options,
    response: analyte_hl7::Response<'_>,
) -> Result<Message, Error> {
    let (first, _) = commands.first().ok_or(Error::EmptyWork)?;
    let mut wire = Vec::new();
    for (command, items) in commands {
        let sample = validate_field(command.app.sample.as_str())?;
        let specimen = command
            .app
            .specimen_type
            .as_ref()
            .ok_or(Error::SpecimenTypeMissing)?;
        let specimen_code = crate::orders::codes::map_specimen(
            command.settings.tables().specimen_system(),
            specimen,
        )
        .map_err(Error::ForeignSpecimenType)?;
        let priority = if command.priority && action == law::WorkAction::New {
            Some(validate_field(
                crate::orders::codes::map_priority(
                    command.settings.tables().priorities(),
                    command.app.priority,
                )
                .map_err(Error::UnmappedPriority)?
                .machine
                .as_str(),
            )?)
        } else {
            None
        };
        for item in *items {
            if wire.len() >= options.limits.segments {
                return Err(Error::QueryLimitExceeded);
            }
            let [requested] = item.tests else {
                return Err(Error::ItemTestCount);
            };
            let tables = command.settings.tables();
            let code = crate::orders::codes::map_test(tables.test_system(), requested)
                .map_err(Error::ForeignTest)?;
            // TF-2b OBR-4 requires its text component; only the lab app names tests.
            let name = command
                .app
                .test_name(requested)
                .ok_or(Error::TestNameMissing)?;
            wire.push(WireAwos {
                // C.6 manager-authored AWOS supports EI.1 only. The shared store
                // still retains the explicit logical namespace beside this identity.
                id: item.placer.to_owned(),
                container: sample.to_owned(),
                fields: {
                    // SPM-4 carries the machine's specimen code and system
                    // (ADR 0030); OBR-4 the machine's test code and system and
                    // the lab app's name for the test (ADR 0029).
                    law::WorkFields::new(
                        sample,
                        command.settings.work().specimen_namespace.as_str(),
                        law::WorkCode {
                            code: validate_field(specimen_code)?,
                            text: "",
                            system: validate_field(specimen.namespace.as_str())?,
                        },
                        law::WorkCode {
                            code: validate_field(code)?,
                            text: validate_field(name.as_str())?,
                            system: validate_field(tables.test_system().as_str())?,
                        },
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
                first.settings.work().specimen_role.as_str().as_bytes(),
                item.container.as_bytes(),
                action,
            )?)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let capabilities = law::Capabilities {
        content: law::PeerContent {
            specimen: first.specimen,
            ..law::Capabilities::default().content
        },
        instructions: law::PeerInstructions {
            priority: first.priority,
            ..law::Capabilities::default().instructions
        },
    };
    Ok(law::work_order(
        &awos,
        None,
        address(first.settings.work()),
        response,
        capabilities,
        options,
    )?)
}

/// Borrows the configured LAW addressing without changing its wire spelling.
#[must_use]
pub fn address(config: &analyte_config::LawWork) -> law::Address<'_> {
    law::Address {
        application: config.application.as_str().as_bytes(),
        facility: config.facility.as_str().as_bytes(),
        peer_application: config.peer_application.as_str().as_bytes(),
        peer_facility: config.peer_facility.as_str().as_bytes(),
    }
}
