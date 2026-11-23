//! Owns the durable key encoding: compact JSON strings, ordered objects and arrays.
//! Field order and UUID spelling are storage format, independent of model serde.
//! This preserves the original bytes; changes require a storage migration.
use crate::StorageError as Error;
use analyte_model::{DeliveryId, Identifier, NamespacedId, ObservationIdentity, ReceiptId};
use serde::Serialize;
#[cfg(test)]
mod tests;

pub(crate) trait Key {
    fn wire(&self) -> impl Serialize;
}
pub(crate) fn key<T: Key>(value: &T) -> Result<String, Error> {
    Ok(serde_json::to_string(&value.wire())?)
}
impl Key for ReceiptId {
    fn wire(&self) -> impl Serialize {
        uuid::Uuid::from(*self).to_string()
    }
}
impl Key for DeliveryId {
    fn wire(&self) -> impl Serialize {
        uuid::Uuid::from(*self).to_string()
    }
}
impl Key for NamespacedId {
    fn wire(&self) -> impl Serialize {
        #[derive(Serialize)]
        struct Wire<'a> {
            namespace: &'a str,
            value: &'a str,
        }
        Wire {
            namespace: self.namespace.as_str(),
            value: self.value.as_str(),
        }
    }
}
#[derive(Serialize)]
struct Observation<'a> {
    code: Namespace<'a>,
    sub_id: &'a str,
    run: &'a str,
    order: Namespace<'a>,
}
#[derive(Serialize)]
struct Namespace<'a> {
    namespace: &'a str,
    value: &'a str,
}
fn namespace(id: &NamespacedId) -> Namespace<'_> {
    Namespace {
        namespace: id.namespace.as_str(),
        value: id.value.as_str(),
    }
}
fn observation(id: &ObservationIdentity) -> Observation<'_> {
    Observation {
        code: namespace(&id.code),
        sub_id: id.sub_id.as_str(),
        run: id.run.as_str(),
        order: namespace(&id.order),
    }
}
impl Key for (&Identifier, &ObservationIdentity) {
    fn wire(&self) -> impl Serialize {
        (self.0.as_str(), observation(self.1))
    }
}
impl Key for crate::OrderingKey {
    fn wire(&self) -> impl Serialize {
        #[derive(Serialize)]
        struct Wire<'a> {
            machine: &'a str,
            observation: Observation<'a>,
        }
        Wire {
            machine: self.machine.as_str(),
            observation: observation(&self.observation),
        }
    }
}

impl Key for crate::MachineOrderKey {
    fn wire(&self) -> impl Serialize {
        #[derive(Serialize)]
        struct Wire<'a> {
            destination: &'a str,
            order: &'a str,
            machine: &'a str,
        }
        Wire {
            destination: self.destination.as_str(),
            order: self.order.as_str(),
            machine: self.machine.as_str(),
        }
    }
}
impl Key for (&Identifier, &Identifier) {
    fn wire(&self) -> impl Serialize {
        (self.0.as_str(), self.1.as_str())
    }
}
impl Key for (&String, &String) {
    fn wire(&self) -> impl Serialize {
        (self.0.as_str(), self.1.as_str())
    }
}
impl Key for analyte_model::SourceText {
    fn wire(&self) -> impl Serialize {
        self.as_str()
    }
}
impl Key for Option<&analyte_model::SampleId> {
    fn wire(&self) -> impl Serialize {
        self.map(analyte_model::SampleId::as_str)
    }
}
impl Key for crate::AlertKey {
    fn wire(&self) -> impl Serialize {
        use crate::{AlertKey, Undeliverable};
        #[derive(Serialize)]
        enum Wire<'a> {
            OldestUndelivered,
            MachineSilent(&'a str),
            MachineSoftwareVersionChanged(&'a str),
            DiskReserve,
            OrderCacheStale,
            UncertainOrders,
            ParkedItems,
            RestorePaused,
            UndeliverableDestination(&'a str, &'static str),
        }
        match self {
            AlertKey::OldestUndelivered => Wire::OldestUndelivered,
            AlertKey::MachineSilent(id) => Wire::MachineSilent(id.as_str()),
            AlertKey::MachineSoftwareVersionChanged(id) => {
                Wire::MachineSoftwareVersionChanged(id.as_str())
            }
            AlertKey::DiskReserve => Wire::DiskReserve,
            AlertKey::OrderCacheStale => Wire::OrderCacheStale,
            AlertKey::UncertainOrders => Wire::UncertainOrders,
            AlertKey::ParkedItems => Wire::ParkedItems,
            AlertKey::RestorePaused => Wire::RestorePaused,
            AlertKey::UndeliverableDestination(id, reason) => Wire::UndeliverableDestination(
                id.as_str(),
                match reason {
                    Undeliverable::NotConfigured => "NotConfigured",
                    Undeliverable::CredentialsMissing => "CredentialsMissing",
                },
            ),
        }
    }
}
