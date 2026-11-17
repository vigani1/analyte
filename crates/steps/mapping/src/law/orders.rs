//! Maps outgoing LAW codes with supplied identities (TF-2b C.6/C.7/C.9).
use crate::orders::WireMappingError as Error;
use analyte_hl7::{Message, Options, law};
use std::sync::Arc;
/// Pure mapper for one app-authorized LAW command.
pub struct Command {
    app: crate::orders::AppOrder,
    settings: Arc<analyte_config::LawOrders>,
    priority: bool,
    specimen: bool,
}
impl Command {
    /// Captures LAW mappings and the peer's explicitly configured capabilities.
    /// # Errors
    /// Preserves disabled LAW order capability errors.
    pub fn new(
        law: &analyte_config::LawStack,
        app: &analyte_contract::Order,
    ) -> Result<Self, Error> {
        let peer = law.policy().peer().capabilities();
        Ok(Self {
            app: crate::orders::app_order(app)?,
            settings: Arc::new(law.orders().ok_or(Error::LawMappingsAbsent)?.clone()),
            priority: peer.instructions.priority,
            specimen: peer.content.specimen,
        })
    }
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
        return Err(Error::LawUnrepresentableIdentity);
    }
    Ok(value)
}

/// Maps every reserved AWOS using explicit specimen, test and priority translations.
/// # Errors
/// Preserves unmapped-code, repertoire, cardinality and LAW-profile failures.
pub fn map_order(
    commands: &[(&Command, &[crate::orders::OutboundItem<'_>])],
    action: law::WorkAction,
    options: Options,
    response: analyte_hl7::Response<'_>,
) -> Result<Message, Error> {
    let (first, _) = commands.first().ok_or(Error::LawEmptyWork)?;
    let mut wire = Vec::new();
    for (command, items) in commands {
        let sample = validate_field(command.app.sample.as_str())?;
        let specimen = command
            .app
            .specimen_type
            .as_ref()
            .ok_or(Error::LawSpecimenTypeMissing)?;
        let mapped =
            crate::orders::codes::map_specimen(Some(command.settings.specimen_types()), specimen)
                .map_err(Error::LawUnmappedSpecimenType)?;
        let priority = if command.priority && action == law::WorkAction::New {
            Some(validate_field(
                crate::orders::codes::map_priority(
                    command.settings.priorities(),
                    command.app.priority,
                )
                .map_err(Error::LawUnmappedPriority)?
                .machine
                .as_str(),
            )?)
        } else {
            None
        };
        for item in *items {
            if wire.len() >= options.limits.segments {
                return Err(Error::LawQueryLimitExceeded);
            }
            let [requested] = item.tests else {
                return Err(Error::LawItemTestCount);
            };
            let mapped_test = crate::orders::codes::map_test(command.settings.tests(), requested)
                .map_err(Error::LawUnmappedTest)?;
            wire.push(WireAwos {
                // C.6 manager-authored AWOS supports EI.1 only. The shared store
                // still retains the explicit logical namespace beside this identity.
                id: item.placer.to_owned(),
                container: sample.to_owned(),
                fields: {
                    let [code, text, system] = mapped_test.machine_components.as_slice() else {
                        return Err(Error::LawTestComponents);
                    };
                    law::WorkFields::new(
                        sample,
                        command.settings.work().specimen_namespace.as_str(),
                        law::WorkCode {
                            code: validate_field(mapped.machine.as_str())?,
                            text: "",
                            system: command.settings.work().specimen_system.as_str(),
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
