//! Parses bounded HTTPS destinations once at the configuration boundary.
use serde::{Deserialize, Serialize};

/// An HTTPS URL without credentials, whitespace, query or fragment.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HttpsUrl {
    original: String,
    parsed: url::Url,
}

impl HttpsUrl {
    /// Parses an HTTPS endpoint within the shared 4096-byte endpoint ceiling.
    /// # Errors
    /// Rejects malformed or insecure destinations without echoing their contents.
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        Self::parse_bounded(value, 4096)
    }

    fn parse_bounded(value: &str, limit: usize) -> Result<Self, &'static str> {
        let parsed = parse_url(value, limit, false)?;
        Ok(Self {
            original: value.into(),
            parsed,
        })
    }

    /// Borrows the parsed URL for host-owned HTTPS requests.
    #[must_use]
    pub fn url(&self) -> &url::Url {
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
        Self::parse_bounded(&value, 2048)
    }
}
impl From<HttpsUrl> for String {
    fn from(value: HttpsUrl) -> Self {
        value.original
    }
}

impl std::fmt::Debug for HttpsUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpsUrl([redacted])")
    }
}

/// Parses a literal-loopback HTTP URL for synthetic reference peers only.
/// # Errors
/// Refuses non-loopback hosts, credentials, query, fragment and oversized input.
pub fn loopback_test_url(value: &str) -> Result<url::Url, &'static str> {
    parse_url(value, 4096, true)
}

fn parse_url(value: &str, limit: usize, loopback: bool) -> Result<url::Url, &'static str> {
    if value.len() > limit
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
        || value.contains(['@', '?', '#', '\\'])
    {
        return Err("use a bounded URL without credentials, whitespace, query or fragment");
    }
    let url = url::Url::parse(value).map_err(|_| "malformed URL")?;
    let allowed_scheme = if loopback {
        url.scheme() == "http"
            && url.host_str().is_some_and(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            })
    } else {
        value.starts_with("https://") && url.scheme() == "https"
    };
    if !allowed_scheme
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("use an HTTPS host or an explicitly permitted literal HTTP loopback test peer");
    }
    Ok(url)
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
        assert!(HttpsUrl::try_from(format!("https://example.com/{}", "x".repeat(2048))).is_err());
    }
}
