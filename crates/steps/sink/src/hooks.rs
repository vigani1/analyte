//! Constructs literal-loopback test destinations without changing production URL policy.
use crate::{Destination, DestinationError, Limits, WebhookSender};
use analyte_model::Identifier;

impl Destination {
    /// Allows literal loopback HTTP solely for local synthetic reference-peer tests.
    /// This explicit opt-in never disables TLS verification for HTTPS.
    /// # Errors
    /// Rejects non-loopback HTTP and the same invalid bounds as `new`.
    pub fn for_loopback_test(
        id: Identifier,
        base: &str,
        secrets: Identifier,
        limits: Limits,
    ) -> Result<Self, DestinationError> {
        let base = if base.starts_with("https://") {
            analyte_config::HttpsUrl::parse(base)
                .map_err(DestinationError::UrlPolicy)?
                .url()
                .clone()
        } else {
            analyte_config::loopback_test_url(base).map_err(DestinationError::UrlPolicy)?
        };
        Self::from_parsed(id, base, secrets, limits)
    }
}

impl WebhookSender {
    /// Creates an HTTP sender for a literal loopback synthetic receiver only.
    /// # Errors
    /// Refuses non-loopback hosts, credentials, queries and fragments.
    pub fn for_loopback_test(
        url: &str,
        timeout: std::time::Duration,
    ) -> Result<Self, DestinationError> {
        let url = analyte_config::loopback_test_url(url).map_err(DestinationError::UrlPolicy)?;
        if timeout.is_zero() || timeout > std::time::Duration::from_secs(300) {
            return Err(DestinationError::Configuration);
        }
        Ok(Self {
            client: crate::http_client(timeout)?,
            url,
        })
    }
}
