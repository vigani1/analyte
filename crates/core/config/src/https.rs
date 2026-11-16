//! Parses bounded HTTPS destinations once at the configuration boundary.
use serde::{Deserialize, Serialize};

/// An HTTPS URL without credentials, whitespace, query or fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HttpsUrl {
    original: String,
    parsed: reqwest::Url,
}

impl HttpsUrl {
    /// Parses a destination within the configuration's 2048-byte ceiling.
    /// # Errors
    /// Rejects malformed or insecure destinations without echoing their contents.
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        if value.len() > 2048
            || value.chars().any(|c| c.is_whitespace() || c.is_control())
            || value.contains(['@', '?', '#', '\\'])
            || !value.starts_with("https://")
        {
            return Err("use an HTTPS URL with a host, without credentials, query or fragment");
        }
        let url = reqwest::Url::parse(value).map_err(|_| "malformed HTTPS URL")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("use an HTTPS URL with a host, without credentials, query or fragment");
        }
        Ok(Self {
            original: value.into(),
            parsed: url,
        })
    }

    /// Borrows the parsed URL for host-owned HTTPS requests.
    #[must_use]
    pub fn url(&self) -> &reqwest::Url {
        &self.parsed
    }

    /// Borrows its URL spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.original
    }
}
impl TryFrom<String> for HttpsUrl {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<HttpsUrl> for String {
    fn from(value: HttpsUrl) -> Self {
        value.original
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0002_destination_keeps_exact_provenance_spelling() {
        let original = "https://EXAMPLE.com";
        let url = HttpsUrl::parse(original).expect("synthetic HTTPS URL");
        assert_eq!(url.as_str(), original);
        assert_eq!(String::from(url), original);
    }
    #[test]
    fn adr0002_destination_refuses_unsafe_and_malformed_urls() {
        for value in [
            "http://example.com",
            "https://user@example.com",
            "https://example.com?q=1",
            "https://example.com#fragment",
            "https://example.com/has space",
            "https://[bad]",
            "https://:443",
        ] {
            assert!(HttpsUrl::parse(value).is_err());
        }
        assert!(HttpsUrl::parse(&format!("https://example.com/{}", "x".repeat(2048))).is_err());
    }
}
