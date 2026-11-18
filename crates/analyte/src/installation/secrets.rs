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

/// Whether an API key value meets the contract's bearer bound: 1 to 4096
/// printable ASCII characters.
#[must_use]
pub fn api_key_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && value.bytes().all(|b| (32..=126).contains(&b))
}

/// Preloaded host credentials. Deliberately has no Debug implementation.
pub struct Environment(BTreeMap<String, Credentials>);

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

impl Environment {
    /// Loads every destination before admission, retaining no values in errors.
    /// An environment variable wins; a name without one is read from the locked
    /// credentials file in the data directory.
    /// # Errors
    /// Names a missing, empty or malformed credential, or refuses an exposed file.
    pub fn load(program: &crate::installation::config::Program) -> Result<Self, SecretError> {
        let stored = super::credentials::Locked::new(program.data_directory()).read()?;
        Self::load_layered(program, |key| std::env::var(key).ok(), &stored)
    }

    /// Loads with `environment` first and `stored` for names it lacks or leaves empty.
    /// # Errors
    /// Names a missing, empty or malformed credential.
    pub fn load_layered(
        program: &crate::installation::config::Program,
        environment: impl Fn(&str) -> Option<String>,
        stored: &super::credentials::Stored,
    ) -> Result<Self, SecretError> {
        Self::load_with(program, |key| {
            environment(key)
                .filter(|value| !value.is_empty())
                .or_else(|| stored.by_variable(key))
        })
    }

    /// Loads from a host lookup; tests can inject credentials without mutating the environment.
    /// # Errors
    /// Names a missing, empty or malformed credential.
    pub fn load_with(
        program: &crate::installation::config::Program,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, SecretError> {
        let read = |name: &Identifier| {
            lookup(&variable(name.as_str()))
                .filter(|s| !s.is_empty())
                .ok_or_else(|| SecretError::Missing(name.as_str().into()))
        };
        let mut credentials = BTreeMap::new();
        for machine in program.machines() {
            let destination = &machine.snapshot().effective().site().destination;
            let api_key = read(&destination.api_key_name)?;
            if !api_key_valid(&api_key) {
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
            credentials.insert(
                destination.name.as_str().into(),
                Credentials {
                    api_key,
                    signing_secrets,
                },
            );
        }
        Ok(Self(credentials))
    }
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

    // ADR 0008
    #[test]
    fn environment_names_are_stable() {
        assert_eq!(super::variable("lab-api.key"), "ANALYTE_SECRET_LAB_API_KEY");
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
