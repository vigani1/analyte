//! Looks up orders, reserves machine commands and reconciles operator decisions.
use crate::orders::OrderError as Error;
pub(crate) mod astm;
pub(crate) mod autolyser;
pub(crate) mod hl7;
pub(crate) mod law;
mod operator;
/// Store failures of the audited order operator command.
pub use operator::CommandError;
mod admission;
mod reservation;
pub(crate) mod sending;
pub(crate) mod vidas;
use crate::installation::config::Machine;
use analyte_contract::{EventFields, OrderEvent, Source, Text};
use analyte_mapping::app_order;
use analyte_model::{Identifier, SampleId};
use analyte_store::{MachineOrder, OrderEventDelivery};
pub use operator::{Command, Outcome, run};
mod error;
pub use error::OrderError;

/// A mapped order version to recheck before issuing any query response bytes.
pub(crate) struct Candidate {
    pub(crate) identity: analyte_store::MachineOrderKey,
    pub(crate) version: std::num::NonZeroU64,
}

/// One cache policy for every query dialog: timeout is unavailable, never empty.
pub(crate) async fn lookup(
    store: &analyte_store::Handle,
    machine: &Machine,
    sample: SampleId,
    until: std::time::Instant,
) -> Result<analyte_store::Lookup, Error> {
    let freshness = machine.destination_limits.freshness.as_secs();
    let destination = destination(machine);
    match store
        .request(until, move |store| {
            store.orders(
                &destination,
                &sample,
                jiff::Timestamp::now().as_second(),
                freshness,
            )
        })
        .await
    {
        Ok(answer) => Ok(answer),
        Err(
            analyte_store::OrderError::Storage(analyte_store::StorageError::Busy(_))
            | analyte_store::OrderError::Worker(
                analyte_store::WorkerError::AdmissionTimeout { .. }
                | analyte_store::WorkerError::ResponseTimeout(_),
            ),
        ) => Ok(analyte_store::Lookup::NotTrusted),
        Err(error) => Err(error.into()),
    }
}

/// Only current requested work addressed to this machine enters any outbound mapper.
pub(crate) fn eligible(order: &analyte_contract::Order, machine: &Machine) -> bool {
    let fields = order.fields();
    analyte_model::OrderState::from(fields.state)
        .eligible_for(
            analyte_model::OrderState::Requested,
            fields.tombstone,
            fields
                .target_analyzers
                .as_ref()
                .map(|targets| targets.iter().map(Text::as_str)),
            machine.id.as_str(),
        )
        .is_ok()
}

/// Builds an immutable contract event from a committed lifecycle snapshot.
/// # Errors
/// Refuses invalid timestamps or contract fields.
pub fn event(record: &MachineOrder) -> Result<OrderEventDelivery, analyte_store::OrderError> {
    build_event(record, None).map_err(EventError::into_store)
}

fn build_event(
    record: &MachineOrder,
    reason: Option<&str>,
) -> Result<OrderEventDelivery, EventError> {
    let time = jiff::Timestamp::from_second(record.updated_at)?.to_string();
    let text = |value: &str| Text::try_from(value.to_owned());
    let event = OrderEvent::try_from(EventFields {
        contract_version: 1,
        delivery_id: text(&uuid::Uuid::now_v7().to_string())?,
        created_at: text(&time)?,
        source: Source {
            analyzer: text(record.key.machine.as_str())?,
            connection: text(record.key.machine.as_str())?,
        },
        order_id: text(record.key.order.as_str())?,
        order_version: record.version,
        state: (&record.state).try_into()?,
        occurred_at: text(&time)?,
        source_status: (&record.source_code).into(),
        reason: reason.map(text).transpose()?,
    })?;
    Ok(analyte_sink::encode_order_event(
        &event,
        record.key.destination.clone(),
    )?)
}

/// Returns the app namespace used for cache lookups and lifecycle events.
pub(crate) fn destination(machine: &Machine) -> Identifier {
    machine.snapshot.effective().site().destination.name.clone()
}

/// Event construction is narrower than order orchestration; the store retains its leaf cause.
#[derive(Debug, thiserror::Error)]
enum EventError {
    #[error(transparent)]
    Timestamp(#[from] jiff::Error),
    #[error(transparent)]
    Document(#[from] analyte_contract::DocumentError),
    #[error(transparent)]
    SinkDeliveryError(#[from] analyte_sink::DeliveryError),
}
impl EventError {
    fn into_store(self) -> analyte_store::OrderError {
        match self {
            Self::Timestamp(source) => analyte_store::OrderError::DeliveryBuild(Box::new(source)),
            Self::Document(source) => analyte_store::OrderError::DeliveryBuild(Box::new(source)),
            Self::SinkDeliveryError(source) => {
                analyte_store::OrderError::DeliveryBuild(Box::new(source))
            }
        }
    }
}
