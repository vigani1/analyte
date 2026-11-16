//! Validates external alert endpoint references and bounded delivery policies.
use analyte_model::Identifier;
use serde::Deserialize;
use std::time::Duration;

/// Installation failures never include endpoint URLs or credential values.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WebhookConfigError {
    /// Endpoint cardinality exceeds the host worker bound.
    #[error("at most 8 alert endpoints are supported")]
    TooMany,
    /// Two endpoints would share durable progress.
    #[error("duplicate alert endpoint identity")]
    Duplicate,
    /// An endpoint URL failed the HTTPS boundary.
    #[error("invalid alert endpoint HTTPS URL")]
    Url,
    /// The reminder cadence is unsafe or unbounded.
    #[error("alert reminder interval must be between 60 and 86400 seconds")]
    Reminder,
    /// Pending work must have a finite nonzero capacity.
    #[error("alert queue capacity must be between 1 and 4096")]
    Capacity,
    /// An HTTP operation must finish within the host budget.
    #[error("alert request timeout must be between 1 and 300 seconds")]
    Timeout,
    /// Retry bounds must be ordered and finite.
    #[error("alert retry delays must satisfy 1 <= minimum <= maximum <= 86400 seconds")]
    Backoff,
    /// Every event has a finite retry budget.
    #[error("alert attempt budget must be between 1 and 100")]
    Attempts,
}

/// Validated endpoint. Private fields prevent callers bypassing installation validation.
#[derive(Clone)]
pub struct AlertEndpoint {
    id: Identifier,
    url: analyte_sink::WebhookUrl,
    secret: Identifier,
}
impl AlertEndpoint {
    /// Stable endpoint identity used for durable delivery progress.
    #[must_use]
    pub fn id(&self) -> &Identifier {
        &self.id
    }
    /// Exact validated HTTPS destination; never include it in logs or events.
    #[must_use]
    pub fn url(&self) -> &analyte_sink::WebhookUrl {
        &self.url
    }
    /// Installation secret reference, never a credential value.
    #[must_use]
    pub fn secret(&self) -> &Identifier {
        &self.secret
    }
}

/// Bounded external delivery settings, independent of clinical destinations.
pub struct AlertDelivery {
    endpoints: Vec<AlertEndpoint>,
    reminder: Duration,
    capacity: usize,
    timeout: Duration,
    backoff_min: Duration,
    backoff_max: Duration,
    attempts: u32,
}
impl AlertDelivery {
    /// Configured endpoints; empty means external delivery is disabled.
    #[must_use]
    pub fn endpoints(&self) -> &[AlertEndpoint] {
        &self.endpoints
    }
    /// Minimum time between still-active reminders.
    #[must_use]
    pub fn reminder(&self) -> Duration {
        self.reminder
    }
    /// Maximum pending events per endpoint, including exhausted events.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Entire HTTP attempt deadline.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    /// Initial retry delay.
    #[must_use]
    pub fn backoff_min(&self) -> Duration {
        self.backoff_min
    }
    /// Maximum retry delay.
    #[must_use]
    pub fn backoff_max(&self) -> Duration {
        self.backoff_max
    }
    /// Finite attempts per event.
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftEndpoint {
    id: Identifier,
    url: String,
    secret: Identifier,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct DraftDelivery {
    endpoints: Vec<DraftEndpoint>,
    reminder_interval_seconds: u64,
    queue_capacity: usize,
    request_timeout_seconds: u64,
    backoff_min_seconds: u64,
    backoff_max_seconds: u64,
    max_attempts: u32,
}
impl Default for DraftDelivery {
    fn default() -> Self {
        Self {
            endpoints: vec![],
            reminder_interval_seconds: 3600,
            queue_capacity: 256,
            request_timeout_seconds: 10,
            backoff_min_seconds: 5,
            backoff_max_seconds: 300,
            max_attempts: 10,
        }
    }
}
impl DraftDelivery {
    pub(super) fn validate(self) -> Result<AlertDelivery, WebhookConfigError> {
        use WebhookConfigError as E;
        if self.endpoints.len() > 8 {
            return Err(E::TooMany);
        }
        if !(60..=86400).contains(&self.reminder_interval_seconds) {
            return Err(E::Reminder);
        }
        if !(1..=4096).contains(&self.queue_capacity) {
            return Err(E::Capacity);
        }
        if !(1..=300).contains(&self.request_timeout_seconds) {
            return Err(E::Timeout);
        }
        if self.backoff_min_seconds == 0
            || self.backoff_min_seconds > self.backoff_max_seconds
            || self.backoff_max_seconds > 86400
        {
            return Err(E::Backoff);
        }
        if !(1..=100).contains(&self.max_attempts) {
            return Err(E::Attempts);
        }
        let mut endpoints: Vec<AlertEndpoint> = Vec::new();
        for endpoint in self.endpoints {
            if endpoints.iter().any(|previous| previous.id == endpoint.id) {
                return Err(E::Duplicate);
            }
            endpoints.push(AlertEndpoint {
                id: endpoint.id,
                url: analyte_sink::WebhookUrl::parse(&endpoint.url).map_err(|_| E::Url)?,
                secret: endpoint.secret,
            });
        }
        Ok(AlertDelivery {
            endpoints,
            reminder: Duration::from_secs(self.reminder_interval_seconds),
            capacity: self.queue_capacity,
            timeout: Duration::from_secs(self.request_timeout_seconds),
            backoff_min: Duration::from_secs(self.backoff_min_seconds),
            backoff_max: Duration::from_secs(self.backoff_max_seconds),
            attempts: self.max_attempts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> Result<AlertDelivery, crate::Error> {
        Ok(toml::from_str::<DraftDelivery>(text)?.validate()?)
    }
    #[test]
    fn adr0014_every_delivery_bound_refuses_invalid_values() {
        for (field, values, expected) in [
            (
                "reminder_interval_seconds",
                vec![0, 59, 86401],
                WebhookConfigError::Reminder,
            ),
            (
                "queue_capacity",
                vec![0, 4097],
                WebhookConfigError::Capacity,
            ),
            (
                "request_timeout_seconds",
                vec![0, 301],
                WebhookConfigError::Timeout,
            ),
            (
                "backoff_min_seconds",
                vec![0, 301],
                WebhookConfigError::Backoff,
            ),
            (
                "backoff_max_seconds",
                vec![0, 4, 86401],
                WebhookConfigError::Backoff,
            ),
            ("max_attempts", vec![0, 101], WebhookConfigError::Attempts),
        ] {
            for value in values {
                let error = parse(&format!("{field}={value}"))
                    .err()
                    .expect("invalid bound");
                assert_eq!(error.downcast_ref::<WebhookConfigError>(), Some(&expected));
            }
        }
    }
    #[test]
    fn adr0014_endpoint_references_cardinality_and_schema_are_checked() -> Result<(), crate::Error>
    {
        let endpoint =
            "[[endpoints]]\nid='relay'\nurl='https://example.com/hook'\nsecret='alert-key'\n";
        assert!(parse("")?.endpoints().is_empty());
        let config = parse(endpoint)?;
        assert_eq!(config.endpoints()[0].id().as_str(), "relay");
        assert_eq!(config.endpoints()[0].secret().as_str(), "alert-key");
        for (text, expected) in [
            (endpoint.repeat(2), WebhookConfigError::Duplicate),
            (endpoint.repeat(9), WebhookConfigError::TooMany),
            (endpoint.replace("https:", "http:"), WebhookConfigError::Url),
        ] {
            assert_eq!(
                parse(&text)
                    .err()
                    .expect("invalid endpoint")
                    .downcast_ref::<WebhookConfigError>(),
                Some(&expected)
            );
        }
        for text in [
            endpoint.replace("id='relay'", "id=''"),
            endpoint.replace("secret='alert-key'", "secret=''"),
            endpoint.replace("secret='alert-key'", ""),
            endpoint.replace("url='https://example.com/hook'", ""),
            format!("{endpoint}password='forbidden'"),
            "typo=1".into(),
        ] {
            assert!(parse(&text).is_err());
        }
        for text in [
            "reminder_interval_seconds=60\nqueue_capacity=1\nrequest_timeout_seconds=1\nbackoff_min_seconds=1\nbackoff_max_seconds=1\nmax_attempts=1",
            "reminder_interval_seconds=86400\nqueue_capacity=4096\nrequest_timeout_seconds=300\nbackoff_min_seconds=86400\nbackoff_max_seconds=86400\nmax_attempts=100",
        ] {
            assert!(parse(text).is_ok());
        }
        Ok(())
    }
}
