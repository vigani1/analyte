//! Validates HTTP destinations, credentials and limits for delivery, order and health requests.
use crate::Error;
use analyte_contract::SigningSecret;
use analyte_model::Identifier;
use std::time::Duration;

/// Host-resolved credentials. No Debug implementation exposes secrets.
pub struct Credentials {
    /// Bearer API key, resolved at attempt time.
    pub api_key: String,
    /// One to eight trusted secrets used together during rotation.
    pub signing_secrets: Vec<SigningSecret>,
}

/// Host boundary for named secrets; implementations must return promptly without I/O.
/// Load credentials with host-owned, bounded I/O before making them available here.
pub trait SecretsSource: Send + Sync {
    /// Resolves the current credentials without logging their contents.
    /// # Errors
    /// Returns `Error::Secrets` when the named secret is unavailable.
    fn credentials(&self, name: &Identifier) -> Result<Credentials, Error>;
}

/// Explicit operational bounds, validated by Destination construction.
#[derive(Clone)]
pub struct Limits {
    /// Deadline for the entire HTTP operation including body reads.
    pub request_timeout: Duration,
    /// Deadline for each local store operation (claim, attempt record, order
    /// page commit). Kept separate from `request_timeout`: a short HTTP budget
    /// must never cut off recording an outcome the app already accepted.
    pub store_timeout: Duration,
    /// Initial retry delay; at least one second because store schedules Unix seconds.
    pub backoff_min: Duration,
    /// Maximum retry delay, including Retry-After and jitter.
    pub backoff_max: Duration,
    /// Maximum age of an order cache before lookups must fail closed.
    pub freshness: Duration,
    /// Delay between completed order polls or failed polls.
    pub poll_interval: Duration,
    /// Maximum orders requested in a page (1 through 100).
    pub page_size: u16,
    /// Maximum pages in a single poll, bounding even a malicious feed.
    pub max_pages: u16,
}

/// Validated endpoint and policy. Secrets are referenced by name only.
#[derive(Clone)]
pub struct Destination {
    pub(crate) id: Identifier,
    pub(crate) base: reqwest::Url,
    pub(crate) secrets: Identifier,
    pub(crate) limits: Limits,
    pub(crate) client: reqwest::Client,
}

impl Destination {
    /// Constructs an HTTPS destination with certificate verification and no redirects.
    /// # Errors
    /// Rejects credentials in URLs, queries/fragments, insecure URLs and invalid bounds.
    pub fn new(
        id: Identifier,
        base: &str,
        secrets: Identifier,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::construct(id, base, secrets, limits, false)
    }

    /// Accepts an HTTPS endpoint already parsed by configuration.
    /// # Errors
    /// Rejects invalid operational bounds or TLS initialization failures.
    pub fn from_url(
        id: Identifier,
        base: &analyte_config::HttpsUrl,
        secrets: Identifier,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::from_parsed(id, base.url().clone(), secrets, limits)
    }

    /// Allows literal loopback HTTP solely for local synthetic reference-peer tests.
    /// This explicit opt-in never disables TLS verification for HTTPS.
    /// # Errors
    /// Rejects non-loopback HTTP and the same invalid bounds as `new`.
    pub fn for_loopback_test(
        id: Identifier,
        base: &str,
        secrets: Identifier,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::construct(id, base, secrets, limits, true)
    }

    fn construct(
        id: Identifier,
        base: &str,
        secrets: Identifier,
        limits: Limits,
        test: bool,
    ) -> Result<Self, Error> {
        if base.len() > 4096 {
            return Err(Error::Configuration);
        }
        let base = reqwest::Url::parse(base).map_err(|_| Error::Configuration)?;
        let loopback = base.host_str().is_some_and(|h| {
            h.trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        if base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || !(base.scheme() == "https" || (test && base.scheme() == "http" && loopback))
        {
            return Err(Error::Configuration);
        }
        Self::from_parsed(id, base, secrets, limits)
    }

    fn from_parsed(
        id: Identifier,
        mut base: reqwest::Url,
        secrets: Identifier,
        limits: Limits,
    ) -> Result<Self, Error> {
        if limits.request_timeout.is_zero()
            || limits.request_timeout > Duration::from_secs(300)
            || limits.store_timeout.is_zero()
            || limits.store_timeout > Duration::from_secs(300)
            || limits.backoff_min.as_secs() == 0
            || limits.backoff_max < limits.backoff_min
            || limits.backoff_max > Duration::from_secs(86_400)
            || limits.freshness.as_secs() == 0
            || limits.poll_interval.is_zero()
            || limits.poll_interval > Duration::from_secs(86_400)
            || !(1..=100).contains(&limits.page_size)
            || limits.max_pages == 0
        {
            return Err(Error::Configuration);
        }
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let client = crate::http_client(limits.request_timeout)?;
        Ok(Self {
            id,
            base,
            secrets,
            limits,
            client,
        })
    }

    pub(crate) fn url(&self, path: &str) -> Result<reqwest::Url, Error> {
        self.base.join(path).map_err(|_| Error::Configuration)
    }

    pub(crate) fn credentials(&self, source: &dyn SecretsSource) -> Result<Credentials, Error> {
        let credentials = source.credentials(&self.secrets)?;
        if credentials.api_key.is_empty()
            || credentials.api_key.len() > 4096
            || !credentials.api_key.is_ascii()
            || credentials.signing_secrets.is_empty()
            || credentials.signing_secrets.len() > 8
            || reqwest::header::HeaderValue::from_str(&format!("Bearer {}", credentials.api_key))
                .is_err()
        {
            return Err(Error::Secrets);
        }
        Ok(credentials)
    }

    /// Returns the configured identity without exposing secrets.
    #[must_use]
    pub fn id(&self) -> &Identifier {
        &self.id
    }

    /// Looks up cached orders under this destination's freshness policy.
    /// Unknown, incomplete, stale or future-clock data remains `Lookup::NotTrusted`.
    /// # Errors
    /// Returns storage failures, never substituting an empty successful lookup.
    pub async fn orders(
        &self,
        store: &analyte_store::Handle,
        sample: &analyte_model::SampleId,
        now: i64,
    ) -> Result<analyte_store::Lookup, Error> {
        let id = self.id.clone();
        let sample = sample.clone();
        let freshness = self.limits.freshness.as_secs();
        Ok(store
            .request(crate::deadline(self.limits.store_timeout)?, move |s| {
                s.orders(&id, &sample, now, freshness)
            })
            .await?)
    }

    /// Reads durable health and applies this destination's freshness policy.
    /// # Errors
    /// Returns store failures; cancelled reads do not alter state.
    pub async fn health(
        &self,
        store: &analyte_store::Handle,
        now: i64,
    ) -> Result<analyte_store::Health, Error> {
        let id = self.id.clone();
        let freshness = self.limits.freshness.as_secs();
        let health = store
            .request(crate::deadline(self.limits.store_timeout)?, move |s| {
                s.health(&id, now, freshness)
            })
            .await?;
        Ok(health)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            request_timeout: Duration::from_secs(1),
            store_timeout: Duration::from_secs(30),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            freshness: Duration::from_secs(10),
            poll_interval: Duration::from_secs(1),
            page_size: 100,
            max_pages: 100,
        }
    }

    #[test]
    fn adr0004_https_and_explicit_literal_loopback_only() -> Result<(), Box<dyn std::error::Error>>
    {
        for url in [
            "http://example.com",
            "http://127.0.0.1:1234",
            "https://user:password@example.com",
            "https://example.com/?secret=x",
            "https://example.com/#fragment",
        ] {
            assert!(
                Destination::new(
                    Identifier::new("app")?,
                    url,
                    Identifier::new("key")?,
                    limits()
                )
                .is_err()
            );
        }
        for url in [
            "http://example.com",
            "http://localhost:1234",
            "http://127.0.0.1.example.com",
        ] {
            assert!(
                Destination::for_loopback_test(
                    Identifier::new("app")?,
                    url,
                    Identifier::new("key")?,
                    limits()
                )
                .is_err()
            );
        }
        assert!(
            Destination::for_loopback_test(
                Identifier::new("app")?,
                "http://127.0.0.1:1234",
                Identifier::new("key")?,
                limits()
            )
            .is_ok()
        );
        let destination = Destination::new(
            Identifier::new("app")?,
            "https://example.com/prefix",
            Identifier::new("key")?,
            limits(),
        )?;
        assert_eq!(
            destination.url("results")?.as_str(),
            "https://example.com/prefix/results"
        );
        Ok(())
    }
}
