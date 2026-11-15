use crate::{Identifier, Reason};
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

/// A trusted observation and its source revision; reruns use a new observation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResultIdentity {
    /// Stable observation key, never derived from a value, ordinal or test name.
    pub observation: ObservationIdentity,
    /// Revision assigned by Analyte, independently of source action/status.
    pub revision: Revision,
}

/// Resolution evidence; missing evidence never becomes a guessed result key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityStatus {
    /// Sufficient evidence identifies the observation and revision.
    Resolved(ResultIdentity),
    /// Operator reconciliation is required.
    Unresolved(Reason),
}

/// SHA-256 digest bytes; hashing and protecting raw bytes belong to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest(pub [u8; 32]);
/// Evidence of different content under one trusted identity; preserves both receipts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityConflict {
    /// Identity claimed by both versions.
    pub identity: ResultIdentity,
    /// First receipt, retained rather than overwritten.
    pub existing: ReceiptId,
    /// Conflicting receipt to park for an operator.
    pub incoming: ReceiptId,
}

/// Provenance required to reproduce an interpretation (ADR 0002 and 0003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptMetadata {
    /// Stable receipt identifier.
    pub id: ReceiptId,
    /// Digest of the original raw bytes.
    pub raw_digest: Digest,
    /// Configured connection identifier.
    pub connection: Identifier,
    /// Configured machine identifier.
    pub machine: Identifier,
    /// Driver name and version.
    pub driver: NamespacedId,
    /// Effective configuration version.
    pub config_version: Identifier,
    /// Digest of the effective configuration retained by the host.
    pub config_digest: Digest,
    /// Host receive instant.
    pub received_at: jiff::Timestamp,
}

/// Analyte-assigned revision number; zero is a valid first revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Revision(pub u64);
impl Revision {
    /// Advances without wrapping; source actions are never used as revisions.
    /// # Errors
    /// Returns an error when the revision space is exhausted.
    pub fn next(self) -> Result<Self, RevisionExhausted> {
        self.0.checked_add(1).map(Self).ok_or(RevisionExhausted)
    }
}

/// Revision counter cannot advance without overflow.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("revision counter exhausted")]
pub struct RevisionExhausted;
/// Observation key from source identity evidence, with namespaced order context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservationIdentity {
    /// Observation code and code system, never a display/test name.
    pub code: NamespacedId,
    /// Source sub-ID; empty is distinct from unavailable identity evidence.
    pub sub_id: crate::SourceText,
    /// Namespaced order identity; missing context requires unresolved status.
    pub order: NamespacedId,
}
