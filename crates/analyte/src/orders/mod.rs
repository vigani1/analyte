//! Host-owned order lookup, command reservation and operator reconciliation.
pub(crate) mod astm;
pub(crate) mod hl7;
mod hl7_send;
pub(crate) mod law;
mod operator;
pub(crate) mod sending;
use crate::{Error, config::Machine};
use analyte_contract::{EventFields, OrderEvent, Source, Text};
use analyte_model::{Identifier, NamespacedId, SampleId};
use analyte_store::{MachineOrder, OrderEventDelivery};
pub use operator::{Command, Outcome, run};

/// A mapped order version to recheck before issuing any query response bytes.
pub(crate) struct Candidate {
    pub(crate) identity: analyte_store::MachineOrderKey,
    pub(crate) version: std::num::NonZeroU64,
}

/// Cache policy shared with the destination poller (ADR 0004).
pub(crate) const FRESHNESS_SECONDS: u64 = 30;

/// One cache policy for every query dialog: timeout is unavailable, never empty.
pub(crate) async fn lookup(
    store: &analyte_store::Handle,
    machine: &Machine,
    sample: SampleId,
    until: std::time::Instant,
) -> Result<analyte_store::Lookup, Error> {
    let destination = destination(machine);
    match store
        .request(until, move |store| {
            store.orders(
                &destination,
                &sample,
                jiff::Timestamp::now().as_second(),
                FRESHNESS_SECONDS,
            )
        })
        .await
    {
        Ok(answer) => Ok(answer),
        Err(analyte_store::Error::Timeout) => Ok(analyte_store::Lookup::NotTrusted),
        Err(error) => Err(error.into()),
    }
}

/// Only current requested work addressed to this machine enters any outbound mapper.
pub(crate) fn eligible(order: &analyte_contract::Order, machine: &Machine) -> bool {
    let fields = order.fields();
    !fields.tombstone
        && fields.state == analyte_contract::evidence::OrderState::Requested
        && fields.target_analyzers.as_ref().is_none_or(|targets| {
            targets
                .iter()
                .any(|target| target.as_str() == machine.id.as_str())
        })
}

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
        patient: fields
            .patient
            .as_ref()
            .map(|p| namespaced(&p.id))
            .transpose()?,
        specimen_type: fields.specimen_type.as_ref().map(namespaced).transpose()?,
        priority: match fields.priority {
            analyte_contract::Priority::Routine => analyte_mapping::AppOrderPriority::Routine,
            analyte_contract::Priority::Urgent => analyte_mapping::AppOrderPriority::Urgent,
            analyte_contract::Priority::Stat => analyte_mapping::AppOrderPriority::Stat,
        },
        position: fields.position.as_ref().map(position).transpose()?,
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
        state: (&record.state).try_into()?,
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

fn namespaced(id: &analyte_contract::evidence::NamespacedId) -> Result<NamespacedId, Error> {
    Ok(NamespacedId {
        namespace: Identifier::new(id.namespace.as_str())?,
        value: Identifier::new(id.value.as_str())?,
    })
}

fn position(
    position: &analyte_contract::evidence::SamplePosition,
) -> Result<analyte_model::SamplePosition, Error> {
    use analyte_contract::evidence::CarrierKind as Wire;
    use analyte_model::CarrierKind;
    Ok(analyte_model::SamplePosition {
        kind: match &position.kind {
            Wire::Rack => CarrierKind::Rack,
            Wire::Disk => CarrierKind::Disk,
            Wire::Tray => CarrierKind::Tray,
            Wire::Other(id) => CarrierKind::Other(Identifier::new(id.as_str())?),
        },
        carrier: position
            .carrier
            .as_ref()
            .map(|id| Identifier::new(id.as_str()))
            .transpose()?,
        slot: Identifier::new(position.slot.as_str())?,
    })
}
