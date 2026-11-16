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
