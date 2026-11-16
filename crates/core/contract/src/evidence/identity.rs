//! Defines version-1 identity evidence types and validates their wire values.
use super::{Identifier, Reason, SourceText};
use serde::{Deserialize, Serialize};
use uuid::{Uuid, Variant, Version};
/// A supplied UUID is not an RFC `UUIDv7`.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("identity must be an RFC `UUIDv7`")]
pub struct InvalidUuid;
macro_rules! uuid_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "Uuid", into = "Uuid")]
        pub struct $name(Uuid);
        impl TryFrom<Uuid> for $name {
            type Error = InvalidUuid;
            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                if value.get_version() != Some(Version::SortRand)
                    || value.get_variant() != Variant::RFC4122
                {
                    return Err(InvalidUuid);
                }
                Ok(Self(value))
            }
        }
        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

uuid_id!(
    ReceiptId,
    "A host-created `UUIDv7` identifying one receipt, retained on reprocessing."
);

uuid_id!(
    DeliveryId,
    "A host-created `UUIDv7` identifying immutable delivery bytes, retained on retries."
);

/// An identifier with its assigning system; neither part can be empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespacedId {
    /// Assigning system, kept exactly.
    pub namespace: Identifier,
    /// Value assigned by that system.
    pub value: Identifier,
}

/// A trusted observation and its Analyte revision; reruns use a new observation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResultIdentity {
    /// Stable observation key, never derived from a value, ordinal or test name.
    pub observation: ObservationIdentity,
    /// Revision assigned by Analyte, independently of source action/status.
    pub revision: Revision,
}

/// Resolution evidence; missing evidence never becomes a guessed result key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatus {
    /// Sufficient evidence identifies the observation and revision.
    Resolved(ResultIdentity),
    /// Operator reconciliation is required.
    Unresolved(Reason),
}

/// Analyte-assigned revision number; zero is a valid first revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Revision(pub u64);
/// Observation key from source identity evidence, with namespaced order context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservationIdentity {
    /// Observation code and code system, never a display/test name.
    pub code: NamespacedId,
    /// Source sub-ID; empty is distinct from unavailable identity evidence.
    pub sub_id: SourceText,
    /// Source run evidence (for example a completion time or run counter);
    /// empty when the machine gives none, in which case a rerun cannot be told
    /// from a retransmission and differing content is parked as a conflict.
    pub run: SourceText,
    /// Namespaced order identity; missing context requires unresolved status.
    pub order: NamespacedId,
}

impl From<analyte_model::ReceiptId> for ReceiptId {
    fn from(v: analyte_model::ReceiptId) -> Self {
        Self(Uuid::from(v))
    }
}

impl From<analyte_model::DeliveryId> for DeliveryId {
    fn from(v: analyte_model::DeliveryId) -> Self {
        Self(Uuid::from(v))
    }
}

impl From<analyte_model::Revision> for Revision {
    fn from(v: analyte_model::Revision) -> Self {
        Self(v.0)
    }
}
