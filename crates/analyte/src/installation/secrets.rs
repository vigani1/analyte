//! Host-owned environment credential loading, separate from configuration checking.
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
}

/// Preloaded host credentials. Deliberately has no Debug implementation.
pub struct EnvironmentSecrets(BTreeMap<String, Credentials>);

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

impl EnvironmentSecrets {
    /// Loads every destination before admission, retaining no values in errors.
    /// # Errors
    /// Names a missing, empty or malformed credential.
    pub fn load(program: &crate::installation::config::Program) -> Result<Self, SecretError> {
        Self::load_with(program, |key| std::env::var(key).ok())
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
            if api_key.len() > 4096 || !api_key.bytes().all(|b| (32..=126).contains(&b)) {
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

impl SecretsSource for EnvironmentSecrets {
    fn credentials(&self, name: &Identifier) -> Result<Credentials, analyte_sink::Error> {
        let stored = self
            .0
            .get(name.as_str())
            .ok_or(analyte_sink::Error::Secrets)?;
        Ok(Credentials {
            api_key: stored.api_key.clone(),
            signing_secrets: stored.signing_secrets.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn adr0008_missing_empty_and_invalid_credentials_name_only() -> Result<(), crate::Error> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/analyte.toml");
        let program = crate::installation::config::Program::load(&path)?;
        for value in [None, Some(String::new())] {
            let error = super::EnvironmentSecrets::load_with(&program, |_| value.clone())
                .err()
                .ok_or("missing error")?;
            assert_eq!(error.to_string(), "credential demo-api is missing or empty");
        }
        let error = super::EnvironmentSecrets::load_with(&program, |name| {
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
        let source = super::EnvironmentSecrets::load_with(&program, |name| {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
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

    #[test]
    fn adr0008_environment_names_are_stable() {
        assert_eq!(super::variable("lab-api.key"), "ANALYTE_SECRET_LAB_API_KEY");
    }
}
