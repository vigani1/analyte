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
use analyte_i18n::{message as tr_args, text as tr};
use analyte_model::Identifier;
use jiff::Timestamp;
use std::sync::Arc;

/// One of a lab app's two keys as a form shows it. The prompt and the
/// replacing field's label are their own messages, so no language's label is
/// changed in code.
#[derive(Clone, Copy)]
pub(in crate::admin::setup) struct KeyForm {
    /// "API key" or "Signing key".
    pub(in crate::admin::setup) label: &'static str,
    /// The empty field's prompt: "Paste the API key".
    pub(super) placeholder: &'static str,
    /// The replacing field's label: "New API key".
    pub(super) replacement: &'static str,
    /// The form field that takes a new value.
    pub(super) field: &'static str,
    /// The field's element id on the page.
    pub(super) input: &'static str,
}

/// A lab app's API key: its label, prompt, form field and field id.
pub(in crate::admin::setup) fn api_key() -> KeyForm {
    KeyForm {
        label: tr("setup-api-key"),
        placeholder: tr("setup-paste-the-api-key"),
        replacement: tr("setup-new-api-key"),
        field: "api_key",
        input: "la-api",
    }
}

/// A lab app's signing key: its label, prompt, form field and field id.
pub(in crate::admin::setup) fn signing_key() -> KeyForm {
    KeyForm {
        label: tr("setup-signing-key"),
        placeholder: tr("setup-paste-the-signing-key"),
        replacement: tr("setup-new-signing-key"),
        field: "signing_key",
        input: "la-signing",
    }
}

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
            .map(|error| tr_args("setup-keys-file", [("error", error.to_string().into())]))
    }

    /// One key's state, never its value. An unreadable file still lets a set
    /// environment variable show, as it wins at startup too.
    pub(in crate::admin::setup) fn key(
        &self,
        form: KeyForm,
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
                        hint: tr("setup-key-hint-environment-wins"),
                    },
                ),
                secrets::Source::Stored { set_at, .. } => (
                    Tone::Good,
                    format::set_on(set_at, now),
                    tr_args(
                        "setup-key-stored-at",
                        [
                            ("name", named.into()),
                            ("time", format::moment_unix(set_at, now).exact.into()),
                        ],
                    ),
                    Entry::Replace {
                        hint: tr("setup-key-hint-leave-empty"),
                    },
                ),
                secrets::Source::Missing => match self.stored {
                    Ok(_) => (
                        Tone::Problem,
                        tr("setup-key-missing").into(),
                        named,
                        Entry::Needed(format::key_needed()),
                    ),
                    Err(_) => (
                        Tone::Attention,
                        tr("setup-key-cannot-be-checked").into(),
                        named,
                        Entry::Needed(format::key_unchecked()),
                    ),
                },
            };
        Key {
            label: form.label,
            placeholder: form.placeholder,
            replacement: form.replacement,
            field: form.field,
            input: form.input,
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
                tr("setup-keys-kept-locked")
            }
            Ok(_) => tr("setup-keys-none-kept"),
            Err(_) => tr("setup-keys-kept-unknown"),
        }
    }

    /// A lab app's two keys.
    pub(super) fn keys(&self, settings: &Destination, now: Timestamp) -> Vec<Key> {
        vec![
            self.key(api_key(), &settings.api_key_name, now),
            self.key(signing_key(), &settings.signing_key_name, now),
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
            // Only the state is read; which key form names it does not matter.
            let key = source.key(api_key(), name, now);
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
                    (api_key().label, &settings.api_key_name),
                    (signing_key().label, &settings.signing_key_name),
                ] {
                    found.push(Found {
                        owner: tr_args(
                            "setup-key-owner-lab-app",
                            [("name", settings.name.as_str().into())],
                        ),
                        label,
                        status: status(name),
                    });
                }
            }
        });
        for (target, name) in &alerts {
            found.push(Found {
                owner: tr_args(
                    "setup-key-owner-alert-target",
                    [("target", target.as_str().into())],
                ),
                label: signing_key().label,
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
        (tr("doctor-overview-api-key"), &lab_app.api_key_name),
        (tr("doctor-overview-signing-key"), &lab_app.signing_key_name),
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
    (!missing.is_empty()).then(|| {
        format!(
            "{} {}",
            format::missing_keys(&missing),
            format::keys_apply()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use analyte_i18n::Language;

    // A placeholder once lowercased the translated label, so Spanish showed
    // "clave de api". Each prompt is now its own message.
    #[test]
    fn key_prompts_keep_each_language_s_own_wording() {
        let spanish = Language::select(["es"]);
        let (placeholder, replacement) =
            spanish.sync_scope(|| (api_key().placeholder, api_key().replacement));
        assert_eq!(placeholder, "Pegue la clave de API");
        assert_eq!(replacement, "Nueva clave de API");
        assert_eq!(api_key().placeholder, "Paste the API key");
        assert_eq!(signing_key().replacement, "New signing key");
    }
}
