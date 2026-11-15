//! Host-owned HL7 unsolicited work and cancellation; HL7 ch.4 and ADR 0003/0004.
use crate::{Error, config::Machine, deadline, invalid};
use analyte_hl7::{
    Options, Version,
    order::{self, OrderMessage, OrderResponse, Outcome},
};
use analyte_model::{Identifier, OrderState, SourceText};
use analyte_store::{
    Handle, ItemDecision, ItemOutcome, MachineOrder, MachineOrderKey, OrderItems, OrderReservation,
};

pub(super) struct Exchange {
    pub(super) record: MachineOrder,
    pub(super) message: OrderMessage,
}
struct Command {
    settings: analyte_config::Orders,
    version: Version,
    options: Options,
    app: analyte_mapping::AppOrder,
    group: String,
}
impl Command {
    fn new(
        machine: &Machine,
        app: &analyte_contract::Order,
        options: Options,
    ) -> Result<Self, Error> {
        let settings = machine
            .snapshot
            .effective()
            .machine()
            .orders
            .clone()
            .ok_or("HL7 orders absent")?;
        let version = match machine.snapshot.effective().machine().syntax.edition {
            Some(analyte_config::Edition::V231) => Version::V231,
            Some(analyte_config::Edition::V251) => Version::V251,
            None => return Err(invalid("HL7 edition absent")),
        };
        let group = app.fields().id.as_str().to_owned();
        if group.len() > 22 {
            return Err(invalid("HL7-app-order-id-too-long-for-ORC-4"));
        }
        Ok(Self {
            settings,
            version,
            options,
            app: super::app_order(app)?,
            group,
        })
    }
    fn build(&self, record: &MachineOrder, cancel: bool) -> Result<OrderMessage, Error> {
        let cfg = self
            .settings
            .hl7_send
            .as_ref()
            .ok_or("HL7 send config absent")?;
        let timestamp = jiff::Timestamp::now()
            .strftime("%Y%m%d%H%M%S%z")
            .to_string();
        let id = uuid::Uuid::now_v7()
            .simple()
            .to_string()
            .chars()
            .skip(12)
            .collect::<String>();
        let header = order::Header {
            application: cfg.application.as_str().as_bytes(),
            facility: cfg.facility.as_ref().map_or(b"", |v| v.as_str().as_bytes()),
            peer_application: cfg.peer_application.as_str().as_bytes(),
            peer_facility: cfg
                .peer_facility
                .as_ref()
                .map_or(b"", |v| v.as_str().as_bytes()),
            control_id: analyte_hl7::ControlId::new(id.as_bytes())?,
            timestamp: order::Timestamp::new(timestamp.as_bytes())?,
            version: self.version,
            processing: match cfg.processing.as_str() {
                "P" => order::Processing::Production,
                "T" => order::Processing::Training,
                "D" => order::Processing::Debugging,
                _ => return Err(invalid("HL7 processing configuration")),
            },
        };
        if cancel {
            let mut orders = Vec::new();
            for item in &record.items {
                let namespace = item
                    .placer_namespace
                    .as_ref()
                    .ok_or("HL7-item-has-no-placer-namespace")?;
                orders.push(order::Order::Cancel {
                    placer: order::OrderId::new(
                        item.placer.as_bytes(),
                        namespace.as_str().as_bytes(),
                    )?,
                    filler: None,
                });
            }
            let sent = match self.version {
                Version::V231 => {
                    order::orm(&orders, order::Subject::Unspecified, header, self.options)?
                }
                Version::V251 => {
                    order::oml(&orders, order::Subject::Unspecified, header, self.options)?
                }
            };
            return Ok(order::context(
                &sent,
                self.group.as_bytes(),
                None,
                self.options,
            )?);
        }
        let sample = order::SampleId::new(self.app.sample.as_str().as_bytes())?;
        let source = match (&cfg.specimen_system, &self.app.specimen_type) {
            (Some(system), Some(specimen)) => {
                let mapped = self
                    .settings
                    .specimen_types
                    .as_ref()
                    .and_then(|types| types.iter().find(|value| value.app == *specimen))
                    .ok_or("HL7-unmapped-specimen-type")?;
                let code = order::Code::new(
                    mapped.machine.as_str().as_bytes(),
                    b"",
                    system.as_str().as_bytes(),
                )?;
                Some(
                    if let Some(version) = &cfg.specimen_version
                        && self.version == Version::V251
                    {
                        code.with_version(version.as_str().as_bytes())?
                    } else {
                        code
                    },
                )
            }
            (Some(_), None) => return Err(invalid("HL7-missing-specimen-type")),
            (None, _) => None,
        };
        let priority = self
            .settings
            .priorities
            .iter()
            .find(|priority| {
                priority.app.as_str()
                    == match self.app.priority {
                        analyte_mapping::AppOrderPriority::Routine => "routine",
                        analyte_mapping::AppOrderPriority::Urgent => "urgent",
                        analyte_mapping::AppOrderPriority::Stat => "stat",
                    }
            })
            .ok_or("HL7-unmapped-priority")?;
        let priority = match priority.machine.as_str() {
            "R" => order::Priority::Routine,
            "S" => order::Priority::Stat,
            "A" => order::Priority::Asap,
            "P" => order::Priority::Preop,
            "C" => order::Priority::Callback,
            "T" => order::Priority::TimingCritical,
            "PRN" => order::Priority::AsNeeded,
            _ => return Err(invalid("HL7-unsupported-priority")),
        };
        let mut orders = Vec::new();
        for item in &record.items {
            let namespace = item
                .placer_namespace
                .as_ref()
                .ok_or("HL7-item-has-no-placer-namespace")?;
            let placer =
                order::OrderId::new(item.placer.as_bytes(), namespace.as_str().as_bytes())?;
            if cancel {
                orders.push(order::Order::Cancel {
                    placer,
                    filler: None,
                });
                continue;
            }
            let [test] = item.tests.as_slice() else {
                return Err(invalid("HL7-send-item-must-cover-one-test"));
            };
            let mapped = self
                .settings
                .tests
                .iter()
                .find(|mapped| mapped.app == *test)
                .ok_or("HL7-unmapped-test")?;
            let [code, text, system] = mapped.machine_components.as_slice() else {
                return Err(invalid("HL7-test-components"));
            };
            orders.push(order::Order::New {
                placer,
                filler: None,
                test: order::Code::new(code.as_bytes(), text.as_bytes(), system.as_bytes())?,
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
                    start: None,
                    end: None,
                },
            });
        }
        let subject = if cancel {
            order::Subject::Unspecified
        } else {
            match (&self.settings.patient_namespace, &self.app.patient) {
                (Some(namespace), Some(patient)) if *namespace == patient.namespace => {
                    order::Subject::Identifier {
                        identifier: patient.value.as_str().as_bytes(),
                        namespace: patient.namespace.as_str().as_bytes(),
                    }
                }
                (Some(_), Some(_)) => return Err(invalid("HL7-patient-namespace-mismatch")),
                _ => order::Subject::Unspecified,
            }
        };
        let sent = match self.version {
            Version::V231 => order::orm(&orders, subject, header, self.options)?,
            Version::V251 => order::oml(&orders, subject, header, self.options)?,
        };
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
}

pub(super) async fn next(
    store: &Handle,
    machine: &Machine,
    owner: &Identifier,
    options: Options,
    after: &mut String,
    reserving: &mut Vec<MachineOrderKey>,
) -> Result<Option<Exchange>, Error> {
    let freshness = machine.destination_limits.freshness.as_secs();
    let settings = machine
        .snapshot
        .effective()
        .machine()
        .orders
        .as_ref()
        .ok_or("HL7 settings missing")?;
    if !settings.send_ahead {
        return Ok(None);
    }
    let dest = super::destination(machine);
    let target = machine.id.clone();
    let cancels = store
        .request(deadline(), move |store| {
            if !store
                .order_trust(&dest, jiff::Timestamp::now().as_second(), freshness)?
                .is_trusted()
            {
                return Ok(Vec::new());
            }
            store.cancel_candidates(
                &dest,
                &target,
                jiff::Timestamp::now().as_second(),
                freshness,
            )
        })
        .await?;
    for (record, cached) in cancels {
        let app: analyte_contract::Order = analyte_contract::decode(cached.document.as_bytes())?;
        let command = Command::new(machine, &app, options)?;
        let message = command.build(&record, true)?;
        let id = record.id;
        reserving.push(record.key.clone());
        let owner = owner.clone();
        let version = app.fields().version;
        match store
            .request(deadline(), move |store| {
                store.request_order_cancel(
                    id,
                    version,
                    jiff::Timestamp::now().as_second(),
                    &owner,
                    freshness,
                    super::event,
                )
            })
            .await
        {
            Ok(record) => return Ok(Some(Exchange { record, message })),
            Err(analyte_store::Error::Conflict) => {
                reserving.clear();
                continue;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let candidates = super::sending::candidate_page(store, machine, after).await?;
    for cached in candidates {
        *after = cached.id.as_str().to_owned();
        let app: analyte_contract::Order = analyte_contract::decode(cached.document.as_bytes())?;
        if !super::eligible(&app, machine) {
            continue;
        }
        let key = MachineOrderKey {
            destination: super::destination(machine),
            order: cached.id,
            machine: machine.id.clone(),
        };
        let version = app.fields().version;
        let command = match Command::new(machine, &app, options) {
            Ok(command) => command,
            Err(error) => {
                reject(store, key, version, &error.to_string(), freshness).await?;
                continue;
            }
        };
        let namespace = settings
            .placer_namespace
            .clone()
            .ok_or("HL7 placer namespace missing")?;
        let identity = key.clone();
        let owner = owner.clone();
        reserving.push(key.clone());
        let reserved = store
            .request(deadline(), move |store| {
                store.reserve_send_items_with(
                    &OrderReservation {
                        identity: &identity,
                        version,
                        now: jiff::Timestamp::now().as_second(),
                        freshness,
                    },
                    OrderItems::PerTest {
                        namespace: &namespace,
                        identity_bytes: 22,
                    },
                    &owner,
                    |record| {
                        command
                            .build(record, false)
                            .map_err(analyte_store::Error::DeliveryBuild)
                    },
                )
            })
            .await;
        match reserved {
            Ok(Ok((record, message))) => return Ok(Some(Exchange { record, message })),
            Ok(Err(refusal)) => {
                tracing::info!(reason=%refusal,"HL7 order reservation refused");
                reserving.clear();
            }
            Err(analyte_store::Error::ItemIdentityTooLong) => {
                reserving.clear();
                reject(
                    store,
                    key,
                    version,
                    "HL7-placer-identity-too-long",
                    freshness,
                )
                .await?;
            }
            Err(analyte_store::Error::DeliveryBuild(error)) => {
                reserving.clear();
                reject(
                    store,
                    key,
                    version,
                    &format!("HL7-order-preflight:{error}"),
                    freshness,
                )
                .await?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}
async fn reject(
    store: &Handle,
    key: MachineOrderKey,
    version: std::num::NonZeroU64,
    reason: &str,
    freshness: u64,
) -> Result<(), Error> {
    let reason = SourceText::new(reason)?;
    let result = store
        .request(deadline(), move |store| {
            store.reject_unsent(
                &OrderReservation {
                    identity: &key,
                    version,
                    now: jiff::Timestamp::now().as_second(),
                    freshness,
                },
                reason,
                super::event,
            )
        })
        .await?;
    if let Err(refusal) = result {
        tracing::info!(reason=%refusal,"HL7 rejection superseded");
    }
    Ok(())
}
pub(super) async fn finish(
    store: &Handle,
    exchange: &Exchange,
    message: Option<&analyte_hl7::Message>,
    fallback: &str,
) -> Result<(), Error> {
    let mut outcomes = Vec::new();
    if let Some(message) = message {
        match OrderResponse::new(message, &exchange.message) {
            Ok(response) => {
                for (item, decision) in exchange.record.items.iter().zip(response.decisions()) {
                    let (decision, code) = if response.acceptance() != analyte_hl7::AckCode::Aa {
                        (
                            ItemDecision::Uncertain,
                            "HL7-application-negative".to_owned(),
                        )
                    } else {
                        match decision.outcome() {
                            Outcome::Accepted => (ItemDecision::Accepted, "HL7-OK".to_owned()),
                            Outcome::Rejected { reason } => (
                                ItemDecision::Rejected,
                                format!(
                                    "HL7-UA:{}",
                                    reason
                                        .map_or(b"".as_slice(), |field| field.raw())
                                        .escape_ascii()
                                ),
                            ),
                            Outcome::CancelAccepted => {
                                (ItemDecision::CancelConfirmed, "HL7-CR".to_owned())
                            }
                            Outcome::UnableToCancel { reason } => (
                                ItemDecision::CancelRefused,
                                format!(
                                    "HL7-UC:{}",
                                    reason
                                        .map_or(b"".as_slice(), |field| field.raw())
                                        .escape_ascii()
                                ),
                            ),
                            Outcome::NotReported => {
                                (ItemDecision::Uncertain, "HL7-order-not-reported".to_owned())
                            }
                        }
                    };
                    outcomes.push(ItemOutcome {
                        index: item.index,
                        decision,
                        code: SourceText::new(&code)?,
                    });
                }
            }
            Err(error) => {
                for item in &exchange.record.items {
                    outcomes.push(ItemOutcome {
                        index: item.index,
                        decision: ItemDecision::Uncertain,
                        code: SourceText::new(&format!("HL7-invalid-order-response:{error}"))?,
                    });
                }
            }
        }
    } else {
        for item in &exchange.record.items {
            outcomes.push(ItemOutcome {
                index: item.index,
                decision: ItemDecision::Uncertain,
                code: SourceText::new(fallback)?,
            });
        }
    }
    let identity = exchange.record.key.clone();
    store
        .request(deadline(), move |store| {
            let current = store
                .machine_order(&identity)?
                .ok_or(analyte_store::Error::Corruption)?;
            if matches!(
                current.state,
                OrderState::Sending | OrderState::CancelRequested
            ) {
                store.finish_order_items(
                    current.id,
                    &outcomes,
                    jiff::Timestamp::now().as_second(),
                    super::event,
                )?;
            }
            Ok(())
        })
        .await?;
    Ok(())
}
