//! Reads the app's address and credentials from the environment or a settings file, never from arguments.
use analyte_config::Destination;
use analyte_contract::SigningSecret;
use std::{collections::BTreeMap, path::Path, time::Duration};

/// Base URL of the lab app, the same base Analyte's destination uses.
pub const URL_NAME: &str = "ANALYTE_APP_URL";
/// Bearer API key the app expects from Analyte.
pub const KEY_NAME: &str = "ANALYTE_API_KEY";
/// Standard Webhooks signing secret (`whsec_…`) the app trusts.
pub const SECRET_NAME: &str = "ANALYTE_SIGNING_SECRET";
/// Optional whole-request deadline in seconds; Analyte's default and range apply.
pub const TIMEOUT_NAME: &str = "ANALYTE_REQUEST_TIMEOUT_SECONDS";

const NAMES: [&str; 4] = [URL_NAME, KEY_NAME, SECRET_NAME, TIMEOUT_NAME];
const FILE_LIMIT: u64 = 65_536;

/// Invalid or missing settings; messages name the setting, never its value.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A required setting is in neither the file nor the environment.
    #[error("{0} is not set")]
    Missing(&'static str),
    /// The URL is not HTTPS, or not a literal loopback address for local HTTP.
    #[error("{URL_NAME}: {0}")]
    Url(&'static str),
    /// The key cannot be sent as a bearer header, as in Analyte.
    #[error("{KEY_NAME} must be 1 to 4096 printable ASCII characters")]
    Key,
    /// The secret is not a Standard Webhooks secret.
    #[error("{SECRET_NAME} must be whsec_ followed by standard base64 of 24 to 64 bytes")]
    Secret,
    /// The deadline is outside Analyte's accepted range.
    #[error("{TIMEOUT_NAME} must be a whole number of seconds from 1 to 120")]
    Timeout,
    /// The settings file could not be read.
    #[error("settings file could not be read")]
    File(#[source] std::io::Error),
    /// The settings file is larger than the bound.
    #[error("settings file exceeds {FILE_LIMIT} bytes")]
    FileTooLarge,
    /// A line is not `NAME=value` with one of the known names.
    #[error("settings file line {0} is not NAME=value with a known name")]
    Line(usize),
}

/// Validated app address, credentials and deadline. Deliberately has no Debug output.
pub struct Settings {
    pub(crate) base: url::Url,
    pub(crate) api_key: String,
    pub(crate) secret: SigningSecret,
    pub(crate) timeout: Duration,
}

impl Settings {
    /// Reads each setting from the file when given, otherwise from the environment.
    ///
    /// # Errors
    /// Returns which setting is missing or invalid, without echoing its value.
    pub fn load(file: Option<&Path>) -> Result<Self, Error> {
        let from_file = match file {
            Some(path) => read_file(path)?,
            None => BTreeMap::new(),
        };
        Self::from_lookup(|name| {
            from_file
                .get(name)
                .cloned()
                .or_else(|| std::env::var(name).ok())
        })
    }

    /// Validates settings from any source, with the same rules as Analyte's destination.
    ///
    /// # Errors
    /// Returns which setting is missing or invalid, without echoing its value.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, Error> {
        let url = lookup(URL_NAME).ok_or(Error::Missing(URL_NAME))?;
        let api_key = lookup(KEY_NAME).ok_or(Error::Missing(KEY_NAME))?;
        let secret = lookup(SECRET_NAME).ok_or(Error::Missing(SECRET_NAME))?;
        let mut base = if url.starts_with("https://") {
            analyte_config::HttpsUrl::parse(&url)
                .map_err(Error::Url)?
                .url()
                .clone()
        } else {
            // Analyte's literal-loopback exception, for an app on the developer's own machine.
            analyte_config::loopback_test_url(&url).map_err(Error::Url)?
        };
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        if !analyte_contract::api_key_valid(&api_key) {
            return Err(Error::Key);
        }
        let parsed = SigningSecret::parse(&secret).map_err(|_| Error::Secret)?;
        let timeout = match lookup(TIMEOUT_NAME) {
            None => Duration::from_secs(Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS),
            Some(seconds) => seconds
                .parse::<u64>()
                .ok()
                .filter(|seconds| Destination::REQUEST_TIMEOUT_SECONDS.contains(seconds))
                .map(Duration::from_secs)
                .ok_or(Error::Timeout)?,
        };
        Ok(Self {
            base,
            api_key,
            secret: parsed,
            timeout,
        })
    }
}

fn read_file(path: &Path) -> Result<BTreeMap<String, String>, Error> {
    let bytes = crate::read_bounded(path, FILE_LIMIT).map_err(Error::File)?;
    let text = String::from_utf8(bytes).map_err(|error| {
        Error::File(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })?;
    if u64::try_from(text.len()).map_or(true, |length| length > FILE_LIMIT) {
        return Err(Error::FileTooLarge);
    }
    let mut values = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let number = index.saturating_add(1);
        let (name, value) = line.split_once('=').ok_or(Error::Line(number))?;
        let name = name.trim();
        if !NAMES.contains(&name) {
            return Err(Error::Line(number));
        }
        values.insert(name.to_owned(), value.to_owned());
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    // Contract v1 "HTTP and authentication"; ADR 0004 literal-loopback exception
    #[test]
    fn only_https_or_literal_loopback() {
        for url in [
            "http://example.com",
            "http://localhost:8080",
            "https://user:pass@example.com",
            "https://example.com/?key=x",
        ] {
            let settings = Settings::from_lookup(lookup(&[
                (URL_NAME, url),
                (KEY_NAME, "key"),
                (SECRET_NAME, SECRET),
            ]));
            assert!(matches!(settings, Err(Error::Url(_))), "{url}");
        }
        for url in ["https://app.example/base", "http://127.0.0.1:8080"] {
            let settings = Settings::from_lookup(lookup(&[
                (URL_NAME, url),
                (KEY_NAME, "key"),
                (SECRET_NAME, SECRET),
            ]));
            assert!(
                settings.is_ok_and(|s| s.base.path().ends_with('/')),
                "{url}"
            );
        }
    }

    // Contract v1 "HTTP and authentication"; destination request_timeout_seconds range
    #[test]
    fn settings_errors_name_the_setting_not_the_value() {
        let missing = Settings::from_lookup(lookup(&[(URL_NAME, "http://127.0.0.1:1")]));
        assert!(matches!(missing, Err(Error::Missing(KEY_NAME))));
        let secret = Settings::from_lookup(lookup(&[
            (URL_NAME, "http://127.0.0.1:1"),
            (KEY_NAME, "key"),
            (SECRET_NAME, "whsec_c2hvcnQ="),
        ]));
        let message = secret.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains(SECRET_NAME) && !message.contains("c2hvcnQ"));
        for timeout in ["0", "121", "ten"] {
            let settings = Settings::from_lookup(lookup(&[
                (URL_NAME, "http://127.0.0.1:1"),
                (KEY_NAME, "key"),
                (SECRET_NAME, SECRET),
                (TIMEOUT_NAME, timeout),
            ]));
            assert!(matches!(settings, Err(Error::Timeout)));
        }
    }

    // Contract v1 "HTTP and authentication": the key is 1 to 4096 printable ASCII
    // characters, the same rule Analyte's sink applies; a TAB is refused.
    #[test]
    fn key_follows_the_shared_bearer_rule() {
        let key = |value: &str| {
            Settings::from_lookup(lookup(&[
                (URL_NAME, "http://127.0.0.1:1"),
                (KEY_NAME, value),
                (SECRET_NAME, SECRET),
            ]))
        };
        for refused in ["", "a\tb", "a\nb", "\u{e9}"] {
            assert!(matches!(key(refused), Err(Error::Key)), "{refused:?}");
        }
        assert!(matches!(key(&"x".repeat(4097)), Err(Error::Key)));
        for accepted in ["a b", "k=v~"] {
            assert!(key(accepted).is_ok(), "{accepted:?}");
        }
        assert!(key(&"x".repeat(4096)).is_ok());
    }

    // Contract v1 "HTTP and authentication": credentials never on the command line
    #[test]
    fn settings_file_takes_known_names_only() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("app.env");
        std::fs::write(
            &path,
            format!(
                "# local app\n{URL_NAME}=http://127.0.0.1:9\n{KEY_NAME}=k=v\n{SECRET_NAME}={SECRET}\n"
            ),
        )?;
        let settings = Settings::load(Some(&path))?;
        assert_eq!(settings.api_key, "k=v");
        std::fs::write(&path, "OTHER=1\n")?;
        assert!(matches!(Settings::load(Some(&path)), Err(Error::Line(1))));
        Ok(())
    }
}
