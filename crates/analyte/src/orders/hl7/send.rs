//! Sends unsolicited HL7 work and cancellations (HL7 ch.4).
use crate::orders::OrderError as Error;
use crate::orders::outcome::Hl7End;
use crate::{deadline, installation::config::Machine};
use analyte_hl7::{
    Options,
    order::{self, OrderMessage, OrderResponse, Outcome},
};
use analyte_model::{Identifier, OrderState, SourceText};
use analyte_store::{Handle, ItemDecision, ItemOutcome, MachineOrder, MachineOrderKey};

pub(super) struct Exchange {
    pub(super) record: MachineOrder,
    pub(super) message: OrderMessage,
}
type Command = analyte_mapping::Hl7Command;
fn command(
    stack: &analyte_config::Hl7Stack,
    app: &analyte_contract::Order,
    options: Options,
) -> Result<Command, Error> {
    Ok(analyte_mapping::map_hl7_order(
        stack.settings().syntax(),
        stack.orders().ok_or(Error::Hl7OrdersMissing)?,
        app,
        options,
    )?)
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
    let metadata =
        crate::orders::hl7::response::response_metadata(crate::orders::hl7::response::Profile::Hl7);
    Ok(command.map(&items, cancel, metadata.response()?)?)
}

pub(super) async fn next(
    store: &Handle,
    machine: &Machine,
    stack: &analyte_config::Hl7Stack,
    owner: &Identifier,
    options: Options,
    after: &mut String,
    reserving: &mut Vec<MachineOrderKey>,
) -> Result<Option<Exchange>, Error> {
    let settings = stack.orders().ok_or(Error::Hl7SettingsMissing)?;
    let Some(_) = &settings.send() else {
        return Ok(None);
    };
    if let Some(exchange) = next_cancel(store, machine, stack, owner, options, reserving).await? {
        return Ok(Some(exchange));
    }
    next_work(store, machine, stack, owner, options, after, reserving).await
}

async fn next_cancel(
    store: &Handle,
    machine: &Machine,
    stack: &analyte_config::Hl7Stack,
    owner: &Identifier,
    options: Options,
    reserving: &mut Vec<MachineOrderKey>,
) -> Result<Option<Exchange>, Error> {
    let freshness = machine.destination_limits.freshness.as_secs();
    let cancels = crate::orders::reservation::cancellations(store, machine, deadline()?).await?;
    for (record, cached) in cancels {
        let app: analyte_contract::Order = analyte_contract::decode(cached.document.as_bytes())?;
        let command = command(stack, &app, options)?;
        let message = build(&command, &record, true)?;
        let id = record.id;
        reserving.push(record.key.clone());
        let version = app.fields().version;
        match crate::orders::reservation::cancel(
            store,
            deadline()?,
            id,
            version,
            owner.clone(),
            freshness,
        )
        .await
        {
            Ok(record) => return Ok(Some(Exchange { record, message })),
            Err(analyte_store::OrderError::Conflict) => {
                reserving.clear();
                continue;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

async fn next_work(
    store: &Handle,
    machine: &Machine,
    stack: &analyte_config::Hl7Stack,
    owner: &Identifier,
    options: Options,
    after: &mut String,
    reserving: &mut Vec<MachineOrderKey>,
) -> Result<Option<Exchange>, Error> {
    let freshness = machine.destination_limits.freshness.as_secs();
    let settings = stack.orders().ok_or(Error::Hl7SettingsMissing)?;
    let Some(send) = &settings.send() else {
        return Ok(None);
    };
    let candidates = crate::orders::sending::candidate_page(store, machine, after).await?;
    for cached in candidates {
        *after = cached.id.as_str().to_owned();
        let crate::orders::admission::Prepared::Mapped(candidate, command) =
            crate::orders::admission::prepare(
                store,
                machine,
                cached,
                deadline()?,
                |error| format!("rejected: {error}"),
                |app| command(stack, app, options),
            )
            .await?
        else {
            continue;
        };
        let key = candidate.identity;
        let version = candidate.version;
        let namespace = send.placer_namespace().clone();
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
        let reserved = crate::orders::reservation::admit(
            store,
            deadline()?,
            crate::orders::reservation::Admission {
                identity,
                version,
                freshness,
                owner,
                groups,
                namespace,
                max_identity_bytes,
            },
            move |record| {
                build(&command, record, false)
                    .map_err(|error| analyte_store::OrderError::DeliveryBuild(error.into()))
            },
        )
        .await;
        match reserved {
            Ok(Ok((record, message))) => return Ok(Some(Exchange { record, message })),
            Ok(Err(refusal)) => {
                tracing::info!(reason=%refusal,"HL7 order reservation refused");
                reserving.clear();
            }
            Err(analyte_store::OrderError::ItemIdentityTooLong) => {
                reserving.clear();
                reject(
                    store,
                    machine,
                    crate::orders::Candidate {
                        identity: key,
                        version,
                    },
                    Hl7End::PlacerIdentityTooLong.as_str(),
                )
                .await?;
            }
            Err(analyte_store::OrderError::DeliveryBuild(error)) => {
                reserving.clear();
                reject(
                    store,
                    machine,
                    crate::orders::Candidate {
                        identity: key,
                        version,
                    },
                    &crate::orders::outcome::Hl7Detail::OrderPreflight(error.to_string())
                        .to_string(),
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
    machine: &Machine,
    candidate: crate::orders::Candidate,
    reason: &str,
) -> Result<(), Error> {
    crate::orders::admission::reject(store, machine, deadline()?, candidate, reason).await
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
                            Hl7End::ApplicationNegative.as_str().to_owned(),
                        )
                    } else {
                        match decision.outcome() {
                            Outcome::Accepted => {
                                (ItemDecision::Accepted, Hl7End::Ok.as_str().to_owned())
                            }
                            Outcome::Rejected { reason } => (
                                ItemDecision::Rejected,
                                crate::orders::outcome::Hl7Detail::Ua(
                                    reason
                                        .map_or(b"".as_slice(), |field| field.raw())
                                        .escape_ascii()
                                        .to_string(),
                                )
                                .to_string(),
                            ),
                            Outcome::CancelAccepted => (
                                ItemDecision::CancelConfirmed,
                                Hl7End::Cr.as_str().to_owned(),
                            ),
                            Outcome::UnableToCancel { reason } => (
                                ItemDecision::CancelRefused,
                                crate::orders::outcome::Hl7Detail::Uc(
                                    reason
                                        .map_or(b"".as_slice(), |field| field.raw())
                                        .escape_ascii()
                                        .to_string(),
                                )
                                .to_string(),
                            ),
                            Outcome::NotReported => (
                                ItemDecision::Uncertain,
                                Hl7End::OrderNotReported.as_str().to_owned(),
                            ),
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
                        code: SourceText::new(
                            &crate::orders::outcome::Hl7Detail::InvalidOrderResponse(
                                error.to_string(),
                            )
                            .to_string(),
                        )?,
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
        .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
            let current = store
                .machine_order(&identity)?
                .ok_or(analyte_store::OrderError::Corruption)?;
            if matches!(
                current.state,
                OrderState::Sending | OrderState::CancelRequested
            ) {
                store.finish_order_items(
                    current.id,
                    &outcomes,
                    jiff::Timestamp::now().as_second(),
                    crate::orders::event,
                )?;
            }
            Ok::<_, analyte_store::OrderError>(())
        })
        .await?;
    Ok(())
}
