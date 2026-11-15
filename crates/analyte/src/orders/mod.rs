//! Host-owned order lookup, command reservation and operator reconciliation.
pub(crate) mod astm;
mod operator;
pub(crate) mod sending;
use crate::{Error, config::Machine};
use analyte_contract::{EventFields, OrderEvent, Source, Text};
use analyte_model::{Identifier, NamespacedId, SampleId};
use analyte_store::{MachineOrder, OrderEventDelivery};
pub use operator::{Command, Outcome, run};

/// Cache policy shared with the destination poller (ADR 0004).
pub(crate) const FRESHNESS_SECONDS: u64 = 30;

/// Adapts validated app fields without inventing a code system or position.
pub(crate) fn app_order(
    order: &analyte_contract::Order,
) -> Result<analyte_mapping::AppOrder, Error> {
    let fields = order.fields();
    let tests = fields
        .tests
        .iter()
        .map(|test| {
            Ok(NamespacedId {
                namespace: Identifier::new(test.namespace.as_str())?,
                value: Identifier::new(test.value.as_str())?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(analyte_mapping::AppOrder {
        sample: SampleId::new(fields.sample_id.as_str())?,
        tests,
        priority: match fields.priority {
            analyte_contract::Priority::Routine => analyte_mapping::AppOrderPriority::Routine,
            analyte_contract::Priority::Urgent => analyte_mapping::AppOrderPriority::Urgent,
            analyte_contract::Priority::Stat => analyte_mapping::AppOrderPriority::Stat,
        },
        // Contract v1 currently supplies no position. A machine requiring it is
        // rejected by the mapper; the host never substitutes its sample ID.
        position: None,
    })
}

/// Builds an immutable contract event from a committed lifecycle snapshot.
/// # Errors
/// Refuses invalid timestamps or contract fields.
pub fn event(record: &MachineOrder) -> Result<OrderEventDelivery, analyte_store::Error> {
    build_event(record, None).map_err(analyte_store::Error::DeliveryBuild)
}

fn build_event(record: &MachineOrder, reason: Option<&str>) -> Result<OrderEventDelivery, Error> {
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
        state: (&record.state).into(),
        occurred_at: text(&time)?,
        source_status: (&record.source_code).into(),
        reason: reason.map(text).transpose()?,
    })?;
    Ok(analyte_sink::build_order_event(
        &event,
        record.key.destination.clone(),
    )?)
}

/// Returns the app namespace used for cache lookups and lifecycle events.
pub(crate) fn destination(machine: &Machine) -> Identifier {
    machine.snapshot.effective().site().destination.name.clone()
}
