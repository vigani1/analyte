//! Host-owned HL7 unsolicited work and cancellation; HL7 ch.4 and ADR 0003/0004.
use crate::{Error, config::Machine, deadline};
use analyte_hl7::{
    Options,
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
type Command = analyte_mapping::Hl7Command;
fn command(
    machine: &Machine,
    app: &analyte_contract::Order,
    options: Options,
) -> Result<Command, Error> {
    Ok(Command::new(&machine.snapshot, app, options)?)
}
fn build(command: &Command, record: &MachineOrder, cancel: bool) -> Result<OrderMessage, Error> {
    let items = record
        .items
        .iter()
        .map(|item| analyte_mapping::OutboundItem {
            placer: &item.placer,
            namespace: item.placer_namespace.as_ref(),
            tests: &item.tests,
        })
        .collect::<Vec<_>>();
    let metadata = crate::hl7_response::response_metadata(crate::hl7_response::Profile::Hl7);
    Ok(command.map(&items, cancel, metadata.response()?)?)
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
    let settings = super::settings::hl7(machine).ok_or("HL7 settings missing")?;
    let Some(send) = &settings.send else {
        return Ok(None);
    };
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
        let command = command(machine, &app, options)?;
        let message = build(&command, &record, true)?;
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
        let command = match command(machine, &app, options) {
            Ok(command) => command,
            Err(error) => {
                reject(store, key, version, &error.to_string(), freshness).await?;
                continue;
            }
        };
        let namespace = send.placer_namespace.clone();
        let groups = command
            .tests()
            .iter()
            .cloned()
            .map(|test| vec![test])
            .collect::<Vec<_>>();
        let max_identity_bytes = order::EI_MAX_BYTES
            .saturating_sub(namespace.as_str().len())
            .saturating_sub(1);
        let identity = key.clone();
        let owner = owner.clone();
        reserving.push(key.clone());
        let reserved = store
            .request(deadline(), move |store| {
                let mut message = None;
                let record = store.reserve_send(
                    &super::reservation::candidate(&identity, version, freshness),
                    analyte_store::ReserveOptions {
                        items: OrderItems::Groups {
                            groups: &groups,
                            namespace: Some(&namespace),
                            max_identity_bytes,
                        },
                        owner: Some(&owner),
                        prepare: Some(&mut |record| {
                            message = Some(
                                build(&command, record, false)
                                    .map_err(analyte_store::Error::DeliveryBuild)?,
                            );
                            Ok(())
                        }),
                    },
                )?;
                match record {
                    Ok(record) => Ok(Ok((
                        record,
                        message.ok_or(analyte_store::Error::InvalidInput)?,
                    ))),
                    Err(refusal) => Ok(Err(refusal)),
                }
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
