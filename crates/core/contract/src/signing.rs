//! Signs and verifies integration HTTP requests without network I/O.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// Authentication failure without secret or payload disclosure.
#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    /// Secret encoding or length is invalid.
    #[error("invalid signing secret")]
    Secret,
    /// Metadata is malformed or oversized.
    #[error("invalid webhook metadata")]
    Metadata,
    /// Timestamp is outside the five-minute window.
    #[error("webhook timestamp outside tolerance")]
    Timestamp,
    /// No trusted signature matched.
    #[error("signature mismatch")]
    Mismatch,
}
/// Whether an API key value meets the contract's bearer bound: 1 to 4096
/// printable ASCII characters, space through `~`. A TAB or any other control
/// character is refused (contract v1 "HTTP and authentication"). Analyte, its
/// sink and the contract checker all use this one rule.
#[must_use]
pub fn api_key_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && value.bytes().all(|b| (32..=126).contains(&b))
}

/// A decoded 24â€“64 byte Standard Webhooks secret; deliberately has no Debug output.
#[derive(Clone)]
pub struct SigningSecret(Vec<u8>);
impl SigningSecret {
    /// Decodes a prefixed, standard-base64 secret.
    ///
    /// # Errors
    /// Rejects missing prefixes, invalid base64 and invalid key lengths.
    pub fn parse(encoded: &str) -> Result<Self, SignatureError> {
        if encoded.len() > 94 {
            return Err(SignatureError::Secret);
        }
        let key = STANDARD
            .decode(
                encoded
                    .strip_prefix("whsec_")
                    .ok_or(SignatureError::Secret)?,
            )
            .map_err(|_| SignatureError::Secret)?;
        if !(24..=64).contains(&key.len()) {
            return Err(SignatureError::Secret);
        }
        Ok(Self(key))
    }
    fn mac(&self, id: &str, timestamp: &str, body: &[u8]) -> Result<Hmac<Sha256>, SignatureError> {
        if id.is_empty()
            || id.len() > 256
            || id.contains('.')
            || timestamp.len() > 20
            || timestamp.is_empty()
            || !timestamp.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(SignatureError::Metadata);
        }
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.0).map_err(|_| SignatureError::Secret)?;
        mac.update(id.as_bytes());
        mac.update(b".");
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(body);
        Ok(mac)
    }
    /// Signs the exact body and metadata as Standard Webhooks v1.
    ///
    /// # Errors
    /// Rejects oversized bodies or malformed metadata.
    pub fn sign(&self, id: &str, timestamp: &str, body: &[u8]) -> Result<String, SignatureError> {
        within_document_limit(body)?;
        self.sign_unbounded(id, timestamp, body)
    }
    /// Signs like [`Self::sign`] but without the contract's document size bound.
    ///
    /// Only for conformance checks that must send a document over the limit to
    /// see the receiver refuse it. Analyte never sends such a document, so its
    /// own deliveries use [`Self::sign`].
    ///
    /// # Errors
    /// Rejects malformed metadata.
    pub fn sign_unbounded(
        &self,
        id: &str,
        timestamp: &str,
        body: &[u8],
    ) -> Result<String, SignatureError> {
        Ok(format!(
            "v1,{}",
            STANDARD.encode(self.mac(id, timestamp, body)?.finalize().into_bytes())
        ))
    }
    /// Verifies all v1 candidates with constant-time MAC comparison.
    ///
    /// # Errors
    /// Rejects malformed metadata, stale/future timestamps and signature mismatches.
    pub fn verify(
        &self,
        id: &str,
        timestamp: &str,
        body: &[u8],
        signatures: &str,
        now: i64,
    ) -> Result<(), SignatureError> {
        within_document_limit(body)?;
        let mac = self.mac(id, timestamp, body)?;
        let sent = timestamp
            .parse::<i64>()
            .map_err(|_| SignatureError::Timestamp)?;
        if now.abs_diff(sent) > 300 {
            return Err(SignatureError::Timestamp);
        }
        if signatures.len() > 4096 {
            return Err(SignatureError::Metadata);
        }
        let mut valid = false;
        for signature in signatures.split_ascii_whitespace() {
            if let Some(encoded) = signature.strip_prefix("v1,")
                && let Ok(bytes) = STANDARD.decode(encoded)
            {
                valid |= mac.clone().verify_slice(&bytes).is_ok();
            }
        }
        if valid {
            Ok(())
        } else {
            Err(SignatureError::Mismatch)
        }
    }
}

/// A signed or verified body holds at most one contract document.
fn within_document_limit(body: &[u8]) -> Result<(), SignatureError> {
    if body.len() > crate::MAX_DOCUMENT_BYTES {
        Err(SignatureError::Metadata)
    } else {
        Ok(())
    }
}
