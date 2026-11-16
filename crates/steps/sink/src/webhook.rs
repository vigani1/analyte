//! Validates exact HTTPS webhook URLs without exposing them in diagnostics.
use crate::Error;

/// An exact HTTPS destination without embedded credentials, query or fragment.
/// Deliberately has no Debug or Display implementation.
#[derive(Clone)]
pub struct WebhookUrl(reqwest::Url);

impl WebhookUrl {
    /// Parses a bounded HTTPS URL; unlike app base URLs, preserves the exact path.
    /// # Errors
    /// Returns a fixed configuration error without retaining rejected input.
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(Error::Configuration);
        }
        let url = reqwest::Url::parse(value).map_err(|_| Error::Configuration)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Configuration);
        }
        Ok(Self(url))
    }

    /// Borrows the exact configured URL for host-owned HTTP requests.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Host HTTPS sender with the same TLS, redirect, proxy and retry policy as app delivery.
/// No Debug implementation can reveal endpoint URLs or credentials.
pub struct WebhookSender {
    client: reqwest::Client,
    url: reqwest::Url,
}
impl WebhookSender {
    /// Creates a bounded sender without opening a connection.
    /// # Errors
    /// Rejects invalid timeouts or failure to initialize verified TLS.
    pub fn new(url: &WebhookUrl, timeout: std::time::Duration) -> Result<Self, Error> {
        if timeout.is_zero() || timeout > std::time::Duration::from_secs(300) {
            return Err(Error::Configuration);
        }
        Ok(Self {
            client: crate::http_client(timeout)?,
            url: url.0.clone(),
        })
    }
    /// Sends immutable bytes; success means HTTP 2xx acceptance, not human receipt.
    /// Remote bodies and reqwest errors are discarded so URLs and secrets cannot
    /// reach caller diagnostics. Cancellation may leave peer acceptance uncertain.
    /// # Errors
    /// Returns only fixed operational categories, never remote text or credentials.
    pub async fn send(
        &self,
        event: &analyte_store::PendingAlert,
        token: &str,
    ) -> Result<(), analyte_store::AlertFailure> {
        use analyte_store::AlertFailure as F;
        if token.is_empty() || token.len() > 4096 || !token.bytes().all(|b| (32..=126).contains(&b))
        {
            return Err(F::Credentials);
        }
        let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| F::Credentials)?;
        auth.set_sensitive(true);
        let response = self
            .client
            .post(self.url.clone())
            .header(reqwest::header::AUTHORIZATION, auth)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("webhook-id", event.id().as_str())
            .body(event.payload().to_vec())
            .send()
            .await
            .map_err(|_| F::Network)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(F::Http)
        }
    }

    /// Creates an HTTP sender for a literal loopback synthetic receiver only.
    /// # Errors
    /// Refuses non-loopback hosts, credentials, queries and fragments.
    pub fn for_loopback_test(url: &str, timeout: std::time::Duration) -> Result<Self, Error> {
        let parsed = reqwest::Url::parse(url).map_err(|_| Error::Configuration)?;
        if parsed.scheme() != "http"
            || !parsed.host_str().is_some_and(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            })
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(Error::Configuration);
        }
        Self::new(&WebhookUrl(parsed), timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0014_refuses_insecure_embedded_secret_and_unbounded_urls() {
        for value in [
            "",
            "invalid",
            "http://example.com",
            "http://127.0.0.1/hook",
            "ftp://example.com",
            "https://user@example.com",
            "https://u:p@example.com",
            "https://example.com?secret=x",
            "https://example.com#secret",
            "https://example.com\n/hook",
            &"x".repeat(4097),
        ] {
            assert!(WebhookUrl::parse(value).is_err());
        }
        let url = WebhookUrl::parse("https://example.com/exact-hook").expect("synthetic URL");
        assert_eq!(url.as_str(), "https://example.com/exact-hook");
    }
}
