//! Reports where the service finds lab app keys and which keys are missing.

use super::{
    page::{Entry, Key},
    scan::scan,
};
use crate::admin::{
    server::Context,
    setup::{Setup, SetupError, off_thread},
    view::{Tone, format, report::Status},
};
use crate::installation::{
    credentials::{CredentialError, Locked, Stored},
    secrets,
};
use analyte_config::Destination;
use analyte_model::Identifier;
use jiff::Timestamp;
use std::sync::Arc;

/// The two keys of a lab app: their label, form field and field id.
pub(in crate::admin::setup) const API_KEY: (&str, &str, &str) = ("API key", "api_key", "la-api");
pub(in crate::admin::setup) const SIGNING_KEY: (&str, &str, &str) =
    ("Signing key", "signing_key", "la-signing");

/// Where a page finds keys, by the service's own rule (`secrets::source`):
/// the service environment, then the locked keys file, read once per page.
pub(in crate::admin::setup) struct KeySource {
    pub(super) environment: fn(&str) -> Option<String>,
    pub(super) stored: Result<Stored, CredentialError>,
}

impl KeySource {
    /// This service's environment and locked keys file.
    pub(in crate::admin::setup) fn read(setup: &Setup) -> Self {
        Self {
            environment: |variable| std::env::var(variable).ok(),
            stored: Locked::new(&setup.data_directory).read(),
        }
    }

    /// Why the locked keys file could not be read, for support.
    pub(super) fn support(&self) -> Option<String> {
        self.stored
            .as_ref()
            .err()
            .map(|error| format!("Keys file: {error}"))
    }

    /// One key's state, never its value. An unreadable file still lets a set
    /// environment variable show, as it wins at startup too.
    pub(in crate::admin::setup) fn key(
        &self,
        (label, field, input): (&'static str, &'static str, &'static str),
        name: &Identifier,
        now: Timestamp,
    ) -> Key {
        let none = Stored::default();
        let file = self.stored.as_ref().unwrap_or(&none);
        let named = name.as_str().to_owned();
        let (tone, word, exact, entry) =
            match secrets::source(name.as_str(), self.environment, file) {
                secrets::Source::Environment(_) => (
                    Tone::Good,
                    format::set_in_environment(&secrets::variable(name.as_str())),
                    named,
                    Entry::Replace {
                        hint: "The service environment’s key wins over one stored here.",
                    },
                ),
                secrets::Source::Stored { set_at, .. } => (
                    Tone::Good,
                    format::set_on(set_at, now),
                    format!("{named}, stored {}", format::moment_unix(set_at, now).exact),
                    Entry::Replace {
                        hint: "Leave it empty to keep the stored key.",
                    },
                ),
                secrets::Source::Missing => match self.stored {
                    Ok(_) => (
                        Tone::Problem,
                        "Missing".into(),
                        named,
                        Entry::Needed(format::KEY_NEEDED),
                    ),
                    Err(_) => (
                        Tone::Attention,
                        "Cannot be checked".into(),
                        named,
                        Entry::Needed(format::KEY_UNCHECKED),
                    ),
                },
            };
        Key {
            label,
            field,
            input,
            tone,
            word,
            exact,
            entry,
        }
    }

    /// What removing a lab app does to its keys, from what the locked file holds.
    pub(super) fn kept(&self, settings: &Destination) -> &'static str {
        match &self.stored {
            Ok(stored)
                if stored.value(settings.api_key_name.as_str()).is_some()
                    || stored.value(settings.signing_key_name.as_str()).is_some() =>
            {
                "Its keys stay stored, locked."
            }
            Ok(_) => "No key is stored for it, so none is kept.",
            Err(_) => "Whether keys are stored for it cannot be checked; any that are stay locked.",
        }
    }

    /// A lab app's two keys.
    pub(super) fn keys(&self, settings: &Destination, now: Timestamp) -> Vec<Key> {
        vec![
            self.key(API_KEY, &settings.api_key_name, now),
            self.key(SIGNING_KEY, &settings.signing_key_name, now),
        ]
    }
}

/// One key on the Settings page: whose it is, which one and where it is
/// found, never its value.
pub(in crate::admin) struct Found {
    pub(in crate::admin) owner: String,
    pub(in crate::admin) label: &'static str,
    pub(in crate::admin) status: Status,
}

/// Where Analyte finds its keys, by the service's own rule and read once, off
/// the async threads: each lab app's two keys, then each of `alerts` (an alert
/// target and its key's name). Also why the lab apps could not be listed and
/// why the locked keys file could not be read, both for support.
pub(in crate::admin) async fn key_sources(
    state: &Arc<Context>,
    alerts: Vec<(Identifier, Identifier)>,
    now: Timestamp,
) -> Result<(Vec<Found>, Option<String>, Option<String>), SetupError> {
    off_thread(state, move |state| {
        let source = KeySource::read(&state.setup);
        let status = |name: &Identifier| {
            let key = source.key(("", "", ""), name, now);
            Status {
                tone: key.tone,
                icon: key.tone.icon(),
                word: key.word,
                exact: key.exact,
                note: None,
            }
        };
        let mut found = Vec::new();
        let listed = scan(state.setup.config()).map(|scan| {
            for settings in scan.settings() {
                for (label, name) in [
                    (API_KEY.0, &settings.api_key_name),
                    (SIGNING_KEY.0, &settings.signing_key_name),
                ] {
                    found.push(Found {
                        owner: format!("Lab app {}", settings.name.as_str()),
                        label,
                        status: status(name),
                    });
                }
            }
        });
        for (target, name) in &alerts {
            found.push(Found {
                owner: format!("Alert target {}", target.as_str()),
                label: "Signing key",
                status: status(name),
            });
        }
        Ok((
            found,
            listed.err().map(|error| error.to_string()),
            source.support(),
        ))
    })
    .await
}

/// Which of `lab_app`'s keys neither the service environment nor the locked
/// keys file holds, said with when keys apply; none when both are found or
/// the keys file cannot be read, since then it cannot be told. Reads the keys
/// file, so call it off the async workers. Never reads out a value.
pub(in crate::admin::setup) fn missing_keys(
    setup: &Setup,
    lab_app: &Destination,
) -> Option<String> {
    let source = KeySource::read(setup);
    let stored = source.stored.as_ref().ok()?;
    let missing: Vec<&str> = [
        ("API key", &lab_app.api_key_name),
        ("signing key", &lab_app.signing_key_name),
    ]
    .into_iter()
    .filter(|(_, name)| {
        matches!(
            secrets::source(name.as_str(), source.environment, stored),
            secrets::Source::Missing
        )
    })
    .map(|(label, _)| label)
    .collect();
    (!missing.is_empty())
        .then(|| format!("{} {}", format::missing_keys(&missing), format::KEYS_APPLY))
}
