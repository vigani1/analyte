//! Retains keyed duplicate evidence and explicit operator holds (ADR 0013).
mod eligibility;
mod links;
pub(crate) mod orders;
mod purge;
mod tombstones;
mod vacuum;
pub use purge::PurgeRun;
pub(crate) use purge::maintenance;
mod holds;
use crate::{Error, Store};
use hmac::{Hmac, KeyInit, Mac};
pub use holds::{HoldTarget, RetentionHold};
use rusqlite::OptionalExtension;
use sha2::Sha256;

/// Host credential, intentionally neither serializable nor printable.
#[derive(Clone)]
pub struct DigestKey([u8; 32]);
impl DigestKey {
    /// Constructs a key from 32 host-provided random bytes.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        crate::digest(&self.0).0
    }
    pub(crate) fn digest(
        &self,
        domain: &str,
        value: &[u8],
    ) -> Result<[u8; 32], hmac::digest::InvalidLength> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0)?;
        mac.update(domain.as_bytes());
        mac.update(&[0]);
        mac.update(value);
        Ok(mac.finalize().into_bytes().into())
    }
}
impl Store {
    /// Installs the host key and refuses loss or replacement of existing tombstone keys.
    /// Call before admitting machines, including with `keep` when tombstones exist.
    /// # Errors
    /// Returns a specific missing-key or fingerprint-mismatch error without changing evidence.
    pub fn configure_retention_key(
        &mut self,
        key: Option<DigestKey>,
        finite: bool,
    ) -> Result<(), Error> {
        let fingerprint: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT fingerprint FROM retention_key WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let tombstones: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM retention_tombstones UNION SELECT 1 FROM retention_orders)",
            [],
            |r| r.get(0),
        )?;
        if tombstones && fingerprint.is_none() {
            return Err(Error::Corruption);
        }
        if finite && key.is_none() || fingerprint.is_some() && key.is_none() {
            return Err(Error::RetentionKeyRequired);
        }
        if let (Some(expected), Some(key)) = (fingerprint, &key)
            && expected.as_slice() != key.fingerprint()
        {
            return Err(Error::RetentionKeyMismatch);
        }
        self.retention_key = key;
        Ok(())
    }
}
#[cfg(test)]
mod tests;

impl Store {
    pub(crate) fn receipt_was_purged(
        &self,
        receipt: analyte_model::ReceiptId,
    ) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM retention_tombstones WHERE receipt=?1)",
            [crate::key(&receipt)?],
            |r| r.get(0),
        )?)
    }
    pub(crate) fn delivery_was_purged(
        &self,
        delivery: analyte_model::DeliveryId,
    ) -> Result<bool, crate::StorageError> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM retention_deliveries WHERE delivery=?1 UNION SELECT 1 FROM retention_order_deliveries WHERE delivery=?1)",
            [crate::key(&delivery)?],
            |r| r.get(0),
        )?)
    }
}
