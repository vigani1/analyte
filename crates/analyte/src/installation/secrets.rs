//! Loads host credentials from environment variables, then from the locked credentials file.
use analyte_contract::SigningSecret;
use analyte_model::Identifier;
use analyte_sink::{Credentials, SecretsSource};
use std::collections::BTreeMap;

/// Specific startup failure; only the credential name is retained.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The named variable is missing, empty or not Unicode.
    #[error("credential {0} is missing or empty")]
    Missing(String),
    /// A present credential violates its encoding or length bounds.
    #[error("credential {0} is invalid")]
    Invalid(String),
    /// The locked credentials file is exposed, malformed or unreadable.
    #[error(transparent)]
    File(#[from] super::credentials::CredentialError),
}

/// Preloaded host credentials. Deliberately has no Debug implementation.
pub struct Environment(BTreeMap<String, Credentials>);

/// Where a credential's value is found. The service loader and the setup page
/// both ask [`source`], so the page never reports a key the service does not use.
/// Deliberately has no Debug implementation: it may hold a value.
pub enum Source {
    /// A non-empty environment variable named by [`variable`]; it wins over the file.
    Environment(String),
    /// The locked credentials file's entry of exactly this name.
    Stored {
        /// The stored value.
        value: String,
        /// Unix second the value was stored.
        set_at: i64,
    },
    /// Neither holds a non-empty value.
    Missing,
}

impl Source {
    /// The value found, if any.
    #[must_use]
    pub fn value(self) -> Option<String> {
        match self {
            Self::Environment(value) | Self::Stored { value, .. } => Some(value),
            Self::Missing => None,
        }
    }
}

/// Finds credential `name`: its environment variable when that is set and not
/// empty, otherwise the locked file's entry of exactly `name`. The file is read
/// by name, never by variable, so two names that share a variable cannot
/// receive each other's stored key.
#[must_use]
pub fn source(
    name: &str,
    environment: impl Fn(&str) -> Option<String>,
    stored: &super::credentials::Stored,
) -> Source {
    if let Some(value) = environment(&variable(name)).filter(|value| !value.is_empty()) {
        return Source::Environment(value);
    }
    match (stored.value(name), stored.set_at(name)) {
        (Some(value), Some(set_at)) if !value.is_empty() => Source::Stored {
            value: value.to_owned(),
            set_at,
        },
        _ => Source::Missing,
    }
}

/// Returns the environment variable for a configured credential name.
#[must_use]
pub fn variable(name: &str) -> String {
    let suffix: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("ANALYTE_SECRET_{suffix}")
}

/// Whether two different names would share one environment variable: they
/// differ only by letter case or by `-` versus `_` (or another sign). Such
/// names are refused, because one could be given the other's key.
#[must_use]
pub fn clash(left: &str, right: &str) -> bool {
    left != right && variable(left) == variable(right)
}

impl Environment {
    /// Whether keys for this destination were loaded. Only a kept destination
    /// can lack them: a set-up machine's missing keys stop startup instead.
    #[must_use]
    pub fn has_keys(&self, destination: &Identifier) -> bool {
        self.0.contains_key(destination.as_str())
    }

    /// Loads every destination before admission, retaining no values in errors.
    /// An environment variable wins; a name without one is read from the locked
    /// credentials file in the data directory.
    /// # Errors
    /// Names a missing, empty or malformed credential, or refuses an exposed file.
    pub fn load(program: &crate::installation::config::Program) -> Result<Self, SecretError> {
        let stored = super::credentials::Locked::new(program.data_directory()).read()?;
        Self::load_layered(program, |key| std::env::var(key).ok(), &stored)
    }

    /// Loads each credential from where [`source`] finds it: `environment`
    /// (keyed by variable) first, then `stored` by exact credential name.
    /// # Errors
    /// Names a missing, empty or malformed credential.
    pub fn load_layered(
        program: &crate::installation::config::Program,
        environment: impl Fn(&str) -> Option<String>,
        stored: &super::credentials::Stored,
    ) -> Result<Self, SecretError> {
        Self::load_named(program, |name| source(name, &environment, stored).value())
    }

    /// Loads from a host lookup keyed by variable, with no locked file; tests can
    /// inject credentials without mutating the environment.
    /// # Errors
    /// Names a missing, empty or malformed credential of a set-up machine's destination.
    pub fn load_with(
        program: &crate::installation::config::Program,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, SecretError> {
        Self::load_layered(program, lookup, &super::credentials::Stored::default())
    }

    /// Loads from a lookup keyed by credential name.
    /// A lab app no set-up machine uses never stops startup: when
    /// its keys are missing or invalid it is left out, its deliveries pause with a
    /// visible "credentials require operator repair" reason, and the key name is logged.
    fn load_named(
        program: &crate::installation::config::Program,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, SecretError> {
        let mut credentials = BTreeMap::new();
        for machine in program.machines() {
            // A record-only machine has no lab app and so no keys (ADR 0034).
            let Some(destination) = &machine.snapshot().effective().site().destination else {
                continue;
            };
            credentials.insert(
                destination.name.as_str().into(),
                read(program, destination, &lookup)?,
            );
        }
        for kept in program.unused_lab_apps() {
            let destination = kept.settings();
            match read(program, destination, &lookup) {
                Ok(read) => {
                    credentials.insert(destination.name.as_str().into(), read);
                }
                Err(error) => tracing::error!(
                    destination = destination.name.as_str(),
                    %error,
                    "lab app no machine uses has no usable keys; its deliveries pause until they are set"
                ),
            }
        }
        Ok(Self(credentials))
    }
}

/// Reads one destination's API key and signing keys, rotation included, from a
/// lookup keyed by credential name.
fn read(
    program: &crate::installation::config::Program,
    destination: &analyte_config::Destination,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Credentials, SecretError> {
    let read = |name: &Identifier| {
        lookup(name.as_str()).ok_or_else(|| SecretError::Missing(name.as_str().into()))
    };
    let api_key = read(&destination.api_key_name)?;
    if !analyte_contract::api_key_valid(&api_key) {
        return Err(SecretError::Invalid(
            destination.api_key_name.as_str().into(),
        ));
    }
    let mut names = vec![&destination.signing_key_name];
    if let Some(rotation) = program.signing_rotation.get(destination.name.as_str()) {
        names.extend(rotation);
    }
    let mut signing_secrets = Vec::new();
    for name in names {
        signing_secrets.push(
            SigningSecret::parse(&read(name)?)
                .map_err(|_| SecretError::Invalid(name.as_str().into()))?,
        );
    }
    Ok(Credentials {
        api_key,
        signing_secrets,
    })
}

impl SecretsSource for Environment {
    fn credentials(
        &self,
        name: &Identifier,
    ) -> Result<Credentials, analyte_sink::DestinationError> {
        let stored = self
            .0
            .get(name.as_str())
            .ok_or(analyte_sink::DestinationError::Secrets)?;
        Ok(Credentials {
            api_key: stored.api_key.clone(),
            signing_secrets: stored.signing_secrets.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    // ADR 0008
    #[test]
    fn credential_errors_name_only_the_credential() -> Result<(), crate::Error> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/analyte.toml");
        let program = crate::installation::config::Program::load(&path)?;
        for value in [None, Some(String::new())] {
            let error = super::Environment::load_with(&program, |_| value.clone())
                .err()
                .ok_or("missing error")?;
            assert_eq!(error.to_string(), "credential demo-api is missing or empty");
        }
        let error = super::Environment::load_with(&program, |name| {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    "sensitive-invalid-secret"
                }
                .into(),
            )
        })
        .err()
        .ok_or("missing error")?;
        assert_eq!(error.to_string(), "credential demo-signing is invalid");
        let source = super::Environment::load_with(&program, |name| {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    analyte_fixtures::SECRET
                }
                .into(),
            )
        })?;
        let credentials = analyte_sink::SecretsSource::credentials(
            &source,
            &analyte_model::Identifier::new("demo")?,
        )?;
        assert_eq!(credentials.api_key, "synthetic-api");
        assert_eq!(credentials.signing_secrets.len(), 1);
        Ok(())
    }

    // Owner decision 2026-11-18 (locked file): an environment variable wins over
    // the file, and a name without one is read from the file.
    #[test]
    fn environment_overrides_the_locked_file() -> Result<(), crate::Error> {
        use crate::installation::credentials::{Locked, Secret};
        use analyte_model::Identifier;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/analyte.toml");
        let program = crate::installation::config::Program::load(&path)?;
        let directory = crate::admin::private_tempdir()?;
        let locked = Locked::new(directory.path());
        let api = Identifier::new("demo-api")?;
        let signing = Identifier::new("demo-signing")?;
        locked.replace(
            vec![
                (api.clone(), Secret::new(&api, "file-api-key".into())?),
                (
                    signing.clone(),
                    Secret::new(&signing, analyte_fixtures::SECRET.into())?,
                ),
            ],
            1,
        )?;
        let stored = locked.read()?;
        let demo = Identifier::new("demo")?;
        let from_file = super::Environment::load_layered(&program, |_| None, &stored)?;
        let credentials = analyte_sink::SecretsSource::credentials(&from_file, &demo)?;
        assert_eq!(credentials.api_key, "file-api-key");
        assert_eq!(credentials.signing_secrets.len(), 1);
        let environment =
            |name: &str| (name == "ANALYTE_SECRET_DEMO_API").then(|| "env-api-key".into());
        let layered = super::Environment::load_layered(&program, environment, &stored)?;
        let credentials = analyte_sink::SecretsSource::credentials(&layered, &demo)?;
        assert_eq!(credentials.api_key, "env-api-key");
        let empty = crate::installation::credentials::Stored::default();
        let error = super::Environment::load_layered(&program, environment, &empty)
            .err()
            .ok_or("missing signing key accepted")?;
        assert_eq!(
            error.to_string(),
            "credential demo-signing is missing or empty"
        );
        Ok(())
    }

    // Owner decision 2026-11-19 (key clash): the locked file is read by exact
    // credential name, so lab_1 never receives lab-1's key although both names
    // share ANALYTE_SECRET_LAB_1_API; and an empty environment variable does not
    // win, for the service and the setup page alike.
    #[test]
    fn stored_keys_are_found_by_exact_name() -> Result<(), crate::Error> {
        use super::{Source, source};
        use crate::installation::credentials::{Locked, Secret};
        use analyte_model::Identifier;
        let directory = crate::admin::private_tempdir()?;
        let locked = Locked::new(directory.path());
        let (dash, underscore) = (Identifier::new("lab-1-api")?, Identifier::new("lab_1-api")?);
        locked.replace(
            vec![
                (dash.clone(), Secret::new(&dash, "dash-key".into())?),
                (
                    underscore.clone(),
                    Secret::new(&underscore, "underscore-key".into())?,
                ),
            ],
            7,
        )?;
        let stored = locked.read()?;
        let found = |name: &str, environment: Option<&str>| {
            source(name, |_| environment.map(str::to_owned), &stored)
        };
        assert!(matches!(
            found("lab_1-api", None),
            Source::Stored { value, set_at: 7 } if value == "underscore-key"
        ));
        assert!(matches!(
            found("lab-1-api", Some("")),
            Source::Stored { value, .. } if value == "dash-key"
        ));
        assert!(matches!(
            found("lab-1-api", Some("env-key")),
            Source::Environment(value) if value == "env-key"
        ));
        assert!(matches!(found("lab-2-api", Some("")), Source::Missing));
        Ok(())
    }

    // ADR 0008
    #[test]
    fn environment_names_are_stable() {
        assert_eq!(super::variable("lab-api.key"), "ANALYTE_SECRET_LAB_API_KEY");
    }

    // Owner decision 2026-11-19 (key clash): names clash when they differ only
    // by letter case or - versus _; the same name is no clash.
    #[test]
    fn names_differing_only_by_case_or_dash_clash() {
        assert!(super::clash("lab-1", "lab_1"));
        assert!(super::clash("Lab-1", "lab-1"));
        assert!(!super::clash("lab-1", "lab-1"));
        assert!(!super::clash("lab-1", "lab-12"));
    }
}

/// Loads the retention credential through the installation environment path.
/// The value is exactly 64 hexadecimal characters encoding 32 random bytes.
/// # Errors
/// Reports missing required or malformed credentials without exposing their values.
pub fn retention_key(required: bool) -> Result<Option<analyte_store::DigestKey>, SecretError> {
    parse_retention_key(std::env::var(variable("retention-digest")).ok(), required)
}
fn parse_retention_key(
    value: Option<String>,
    required: bool,
) -> Result<Option<analyte_store::DigestKey>, SecretError> {
    let Some(value) = value else {
        return if required {
            Err(SecretError::Missing("retention-digest".into()))
        } else {
            Ok(None)
        };
    };
    if value.len() != 64 {
        return Err(SecretError::Invalid("retention-digest".into()));
    }
    let mut bytes = [0u8; 32];
    for (pair, byte) in value.as_bytes().as_chunks::<2>().0.iter().zip(&mut bytes) {
        let text = std::str::from_utf8(pair)
            .map_err(|_| SecretError::Invalid("retention-digest".into()))?;
        *byte = u8::from_str_radix(text, 16)
            .map_err(|_| SecretError::Invalid("retention-digest".into()))?;
    }
    Ok(Some(analyte_store::DigestKey::new(bytes)))
}

#[cfg(test)]
mod retention_tests {
    // ADR 0013 item 6
    #[test]
    fn retention_secret_required_only_for_finite_policy() -> Result<(), super::SecretError> {
        assert!(super::parse_retention_key(None, true).is_err());
        assert!(super::parse_retention_key(None, false)?.is_none());
        assert!(super::parse_retention_key(Some("sensitive-invalid".into()), true).is_err());
        assert!(super::parse_retention_key(Some("ab".repeat(32)), true)?.is_some());
        Ok(())
    }
}
