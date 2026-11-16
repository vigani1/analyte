//! Holds evidence lists capped at 256 entries during deserialization, plus a non-empty variant.
use serde::{Deserialize, Serialize};
/// A bounded sequence, capped at 256 entries including during deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EvidenceList<T>(pub(super) Vec<T>);
/// A source supplied too many evidence entries.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("evidence list exceeds 256 entries")]
pub struct ListError;
impl<T> EvidenceList<T> {
    /// Accepts at most 256 entries.
    /// # Errors
    /// Rejects a longer list.
    pub fn new(entries: Vec<T>) -> Result<Self, ListError> {
        if entries.len() > 256 {
            return Err(ListError);
        }
        Ok(Self(entries))
    }

    /// Borrows entries without permitting unbounded mutation.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for EvidenceList<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = EvidenceList<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("at most 256 evidence entries")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = seq.next_element()? {
                    if entries.len() == 256 {
                        return Err(serde::de::Error::custom(ListError));
                    }
                    entries.push(entry);
                }
                Ok(EvidenceList(entries))
            }
        }
        d.deserialize_seq(Visitor(std::marker::PhantomData))
    }
}

/// A replacement unit must contain source evidence, even when its value is unsupported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyEvidenceList<T>(pub(super) EvidenceList<T>);
/// Replacement evidence cannot be absent.
#[derive(Debug, thiserror::Error)]
#[error("replacement unit has no source parts")]
pub struct EmptyEvidence;
impl<T> NonEmptyEvidenceList<T> {
    /// Requires at least one entry in an already bounded list.
    /// # Errors
    /// Rejects an empty replacement unit.
    pub fn new(entries: EvidenceList<T>) -> Result<Self, EmptyEvidence> {
        if entries.as_slice().is_empty() {
            return Err(EmptyEvidence);
        }
        Ok(Self(entries))
    }

    /// Returns source parts in order, retaining repeated content.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyEvidenceList<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(EvidenceList::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
