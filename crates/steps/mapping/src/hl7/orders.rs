//! Maps outgoing HL7 work using explicit code tables.
use crate::orders::WireMappingError as Error;
use analyte_hl7::{
    Options, Version,
    order::{self, OrderMessage},
};

/// Pure initiating HL7 command mapper; no storage, clocks or transport.
pub struct Command {
    settings: analyte_config::Hl7Orders,
    version: Version,
    options: Options,
    app: crate::orders::AppOrder,
    group: String,
}
impl Command {
    /// Captures validated outgoing mappings and the app's exact group identity.
    /// # Errors
    /// Preserves disabled capability, patient requirements and group-length refusals.
    pub fn new(
        syntax: &analyte_config::Hl7Syntax,
        settings: &analyte_config::Hl7Orders,
        app: &analyte_contract::Order,
        options: Options,
    ) -> Result<Self, Error> {
        let edition = syntax.edition();
        let settings = settings.clone();
        if settings
            .send()
            .as_ref()
            .is_some_and(|send| send.settings().require_patient_identification)
        {
            return Err(Error::Hl7PatientNameRequired);
        }
        let version: Version = edition.into();
        if version.orm_patient_name_required()
            && app.fields().patient.is_some()
            && app.fields().state == analyte_contract::document::OrderState::Requested
        {
            return Err(Error::Hl7PatientNameRequired);
        }
        let group = app.fields().id.as_str().to_owned();
        let namespace = settings
            .send()
            .as_ref()
            .ok_or(Error::Hl7SendSettingsAbsent)?
            .placer_namespace()
            .as_str();
        if group
            .len()
            .checked_add(namespace.len())
            .and_then(|n| n.checked_add(1))
            .is_none_or(|n| n > order::EI_MAX_BYTES)
        {
            return Err(Error::Hl7GroupIdTooLong);
        }
        Ok(Self {
            settings,
            version,
            options,
            app: crate::orders::app_order(app)?,
            group,
        })
    }
    /// Requested app tests for atomic per-item reservation; no wire lookup occurs here.
    #[must_use]
    pub fn tests(&self) -> &[analyte_model::NamespacedId] {
        &self.app.tests
    }
    /// Maps one already-reserved command with host-supplied response metadata.
    /// # Errors
    /// Preserves exact-code, identity, repertoire and protocol-bound refusals.
    pub fn map(
        &self,
        items: &[crate::orders::OutboundItem<'_>],
        cancel: bool,
        response: analyte_hl7::Response<'_>,
    ) -> Result<OrderMessage, Error> {
        let cfg = self
            .settings
            .send()
            .as_ref()
            .ok_or(Error::Hl7SendSettingsAbsent)?;
        let cfg = &cfg.settings();
        let header = self.map_header(cfg, response)?;
        if cancel {
            return self.map_cancel(items, header);
        }
        let sample = order::SampleId::new(self.app.sample.as_str().as_bytes())?;
        let source = self.map_specimen(cfg)?;
        let priority = self.map_priority()?;
        let mut orders = Vec::new();
        for item in items {
            let namespace = item.namespace.ok_or(Error::Hl7PlacerNamespaceMissing)?;
            let placer =
                order::OrderId::new(item.placer.as_bytes(), namespace.as_str().as_bytes())?;
            if cancel {
                orders.push(order::Order::Cancel {
                    placer,
                    filler: None,
                });
                continue;
            }
            let [requested] = item.tests else {
                return Err(Error::Hl7ItemTestCount);
            };
            let mapped = crate::orders::codes::map_test(self.settings.tables().tests(), requested)
                .map_err(|source| Error::lookup("HL7-unmapped-test", source))?;
            orders.push(order::Order::New {
                placer,
                filler: None,
                test: test_code(&mapped.machine_components)?,
                specimen: order::Specimen {
                    sample,
                    field: if cfg.sample_field == 18 {
                        order::SampleField::Placer1
                    } else {
                        order::SampleField::Placer2
                    },
                    source,
                },
                priority,
                timing: order::Timing {
                    priority_version: cfg.priority_version.as_ref().map(|v| v.as_str().as_bytes()),
                    start: None,
                    end: None,
                },
            });
        }
        let subject = if cancel {
            order::Subject::Unspecified
        } else {
            match (&self.settings.patient_namespace(), &self.app.patient) {
                (Some(namespace), Some(patient)) if *namespace == patient.namespace => {
                    order::Subject::Identifier {
                        identifier: patient.value.as_str().as_bytes(),
                        namespace: patient.namespace.as_str().as_bytes(),
                    }
                }
                (Some(_), Some(_)) => return Err(Error::Hl7PatientNamespaceMismatch),
                _ => order::Subject::Unspecified,
            }
        };
        let sent = self.encode(&orders, subject, header)?;
        let position = if cancel {
            None
        } else {
            cfg.position_field
                .zip(self.app.position.as_ref())
                .map(|(field, position)| {
                    (
                        if field == 18 {
                            order::SampleField::Placer1
                        } else {
                            order::SampleField::Placer2
                        },
                        position.slot.as_str().as_bytes(),
                    )
                })
        };
        Ok(order::context(
            &sent,
            self.group.as_bytes(),
            position,
            self.options,
        )?)
    }
    fn map_header<'a>(
        &self,
        cfg: &'a analyte_config::Hl7Send,
        response: analyte_hl7::Response<'a>,
    ) -> Result<order::Header<'a>, Error> {
        Ok(order::Header {
            application: cfg.application.as_str().as_bytes(),
            facility: cfg.facility.as_ref().map_or(b"", |v| v.as_str().as_bytes()),
            peer_application: cfg.peer_application.as_str().as_bytes(),
            peer_facility: cfg
                .peer_facility
                .as_ref()
                .map_or(b"", |v| v.as_str().as_bytes()),
            control_id: response.control_id,
            timestamp: order::Timestamp::new(response.timestamp)?,
            version: self.version,
            processing: match cfg.processing {
                analyte_config::Hl7Processing::Production => order::Processing::Production,
                analyte_config::Hl7Processing::Training => order::Processing::Training,
                analyte_config::Hl7Processing::Debugging => order::Processing::Debugging,
            },
        })
    }

    fn map_cancel(
        &self,
        items: &[crate::orders::OutboundItem<'_>],
        header: order::Header<'_>,
    ) -> Result<OrderMessage, Error> {
        let mut orders = Vec::new();
        for item in items {
            let namespace = item.namespace.ok_or(Error::Hl7PlacerNamespaceMissing)?;
            orders.push(order::Order::Cancel {
                placer: order::OrderId::new(item.placer.as_bytes(), namespace.as_str().as_bytes())?,
                filler: None,
            });
        }
        let sent = self.encode(&orders, order::Subject::Unspecified, header)?;
        Ok(order::context(
            &sent,
            self.group.as_bytes(),
            None,
            self.options,
        )?)
    }

    fn encode(
        &self,
        orders: &[order::Order<'_>],
        subject: order::Subject<'_>,
        header: order::Header<'_>,
    ) -> Result<OrderMessage, Error> {
        if self.version.orm_writer() {
            Ok(order::orm(orders, subject, header, self.options)?)
        } else if self.version.oml_writer() {
            Ok(order::oml(orders, subject, header, self.options)?)
        } else {
            Err(Error::Hl7EditionUnsupported)
        }
    }

    fn map_specimen<'a>(
        &'a self,
        cfg: &'a analyte_config::Hl7Send,
    ) -> Result<Option<order::Code<'a>>, Error> {
        Ok(match (&cfg.specimen_system, &self.app.specimen_type) {
            (Some(system), Some(specimen)) => {
                let mapped = crate::orders::codes::map_specimen(
                    self.settings.tables().specimen_types().as_deref(),
                    specimen,
                )
                .map_err(|source| Error::lookup("HL7-unmapped-specimen-type", source))?;
                let code = order::Code::new(
                    mapped.machine.as_str().as_bytes(),
                    b"",
                    system.as_str().as_bytes(),
                )?;
                Some(
                    if let Some(version) = &cfg.specimen_version
                        && self.version.specimen_code_version()
                    {
                        code.with_version(version.as_str().as_bytes())?
                    } else {
                        code
                    },
                )
            }
            (Some(_), None) => return Err(Error::Hl7SpecimenTypeMissing),
            (None, _) => None,
        })
    }

    fn map_priority(&self) -> Result<order::Priority, Error> {
        let priority = crate::orders::codes::map_priority(
            self.settings.tables().priorities(),
            self.app.priority,
        )
        .map_err(|source| Error::lookup("HL7-unmapped-priority", source))?;
        let priority = match &priority.machine {
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::R) => {
                order::Priority::Routine
            }
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::S) => {
                order::Priority::Stat
            }
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::A) => {
                order::Priority::Asap
            }
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::P) => {
                order::Priority::Preop
            }
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::C) => {
                order::Priority::Callback
            }
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::T) => {
                order::Priority::TimingCritical
            }
            analyte_config::MachinePriority::Code(analyte_config::PriorityCode::Prn) => {
                order::Priority::AsNeeded
            }
            analyte_config::MachinePriority::Custom(_) => {
                return Err(Error::Hl7PriorityUnsupported);
            }
        };
        Ok(priority)
    }
}

fn test_code(components: &[String]) -> Result<order::Code<'_>, Error> {
    let [code, text, system, rest @ ..] = components else {
        return Err(Error::Hl7TestComponents);
    };
    let code = order::Code::new(code.as_bytes(), text.as_bytes(), system.as_bytes())?;
    match rest {
        [] => Ok(code),
        [empty1, empty2, empty3, revision]
            if [empty1, empty2, empty3].iter().all(|v| v.is_empty()) =>
        {
            Ok(code.with_version(revision.as_bytes())?)
        }
        _ => Err(Error::Hl7TestComponents),
    }
}
