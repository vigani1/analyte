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
        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <crate::schema::Uuid7 as schemars::JsonSchema>::json_schema(generator)
            }
        }
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NamespacedId {
    /// Assigning system, kept exactly.
    pub namespace: Identifier,
    /// Value assigned by that system.
    pub value: Identifier,
}

/// A trusted observation and its Analyte revision; reruns use a new observation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResultIdentity {
    /// Stable observation key, never derived from a value, ordinal or test name.
    pub observation: ObservationIdentity,
    /// Revision assigned by Analyte, independently of source action/status.
    pub revision: Revision,
}

/// Resolution evidence; missing evidence never becomes a guessed result key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatus {
    /// Sufficient evidence identifies the observation and revision.
    Resolved(ResultIdentity),
    /// Operator reconciliation is required.
    Unresolved(Reason),
}

/// Analyte-assigned revision number; zero is a valid first revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Revision(pub u64);
/// Observation key from source identity evidence, with namespaced order context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
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

/// A model part is not 1 to 64 lower-case ASCII letters, digits or hyphens.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("model must be vendor/model, each part 1 to 64 lower-case letters, digits or hyphens")]
pub struct InvalidModel;

/// Longest vendor or model part, in bytes.
const MODEL_PART_BYTES: usize = 64;

/// The machine model that sent a document: the `vendor/model` folder key of
/// its machine definition, for example `roche/cobas-e411` (ADR 0032). Lab
/// apps key their per-model test and specimen tables on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ModelId(String);

impl ModelId {
    /// Joins a vendor and a model folder name after checking both.
    ///
    /// # Errors
    /// Rejects an empty, over-long or non-key part; nothing is tidied.
    pub fn new(vendor: &str, model: &str) -> Result<Self, InvalidModel> {
        if !model_part(vendor) || !model_part(model) {
            return Err(InvalidModel);
        }
        Ok(Self(format!("{vendor}/{model}")))
    }

    /// Reads the `vendor/model` text form.
    ///
    /// # Errors
    /// Rejects text without exactly one `/` between two valid parts.
    pub fn parse(text: &str) -> Result<Self, InvalidModel> {
        let (vendor, model) = text.split_once('/').ok_or(InvalidModel)?;
        Self::new(vendor, model)
    }

    /// Returns the `vendor/model` text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the vendor part.
    #[must_use]
    pub fn vendor(&self) -> &str {
        self.0.split_once('/').map_or("", |(vendor, _)| vendor)
    }
}

/// Whether `part` can be a vendor or model folder name, as shipped.
fn model_part(part: &str) -> bool {
    (1..=MODEL_PART_BYTES).contains(&part.len())
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl schemars::JsonSchema for ModelId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ModelId".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[a-z0-9-]{1,64}/[a-z0-9-]{1,64}$",
            "description": "The machine model: its definition's vendor/model folder key, for example roche/cobas-e411 (ADR 0032). Key per-model tables on it; see machines.json.",
        })
    }
}

impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = ModelId;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a vendor/model key")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                ModelId::parse(value).map_err(E::custom)
            }
        }
        // Checked before the owned copy is made, like the other text types.
        d.deserialize_str(Visitor)
    }
}
