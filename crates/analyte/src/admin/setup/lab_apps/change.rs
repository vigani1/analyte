//! Writes lab app settings and keys, checks removals and reports what changed.

use super::{
    keys::KeySource,
    scan::{Known, scan},
};
use crate::admin::{
    setup::{SETTINGS_UNCHANGED, Setup, SetupError, Via, copies, entries, site, stored},
    view::format,
};
use crate::installation::{
    config::{lab_apps as file, plain_name},
    credentials::{Locked, Secret},
};
use analyte_config::{Destination, HttpsUrl};
use analyte_model::Identifier;
use jiff::Timestamp;
use std::path::PathBuf;

/// Checks a new lab app's name: it also names its keys and appears in links, so
/// it uses only ASCII letters, digits, `-` and `_`, at most 64 characters.
pub(super) fn new_name(text: &str) -> Result<Identifier, SetupError> {
    let text = text.trim();
    if text.is_empty() || text.len() > 64 || !plain_name(text) {
        return Err(SetupError::LabAppName);
    }
    Identifier::new(text).map_err(|_| SetupError::LabAppName)
}

fn seconds(text: &str, default: u64, what: &'static str) -> Result<u64, SetupError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(default);
    }
    text.parse().map_err(|_| SetupError::InvalidNumber(what))
}

/// Results and order messages the lab app has not accepted yet, by lab app name.
pub(in crate::admin::setup) async fn waiting(
    store: &analyte_store::Handle,
) -> Result<Vec<(String, u64)>, SetupError> {
    let open = stored(store, analyte_store::Store::undelivered_by_destination)
        .await
        .map_err(SetupError::Store)?;
    Ok(open
        .into_iter()
        .map(|open| (open.destination.as_str().to_owned(), open.waiting()))
        .collect())
}

pub(in crate::admin::setup) fn count(waiting: &[(String, u64)], name: &str) -> u64 {
    waiting
        .iter()
        .find(|(destination, _)| destination == name)
        .map_or(0, |(_, count)| *count)
}

/// A lab app's settings and keys as a person entered them, on the page or at
/// the command line; every value is checked by [`add_blocking`] or
/// [`save_blocking`]. An empty number means the default; an empty key keeps
/// the stored one. Deliberately has no `Debug`: it may hold keys.
#[derive(Default)]
pub(in crate::admin::setup) struct Entered {
    pub(in crate::admin::setup) name: String,
    pub(in crate::admin::setup) base_url: String,
    pub(in crate::admin::setup) freshness_seconds: String,
    pub(in crate::admin::setup) request_timeout_seconds: String,
    pub(in crate::admin::setup) api_key: String,
    pub(in crate::admin::setup) signing_key: String,
}

impl Entered {
    /// The current settings of `known` as entered values, with no keys, so a
    /// change of one value keeps the others.
    pub(in crate::admin::setup) fn of(known: &Destination) -> Self {
        Self {
            name: known.name.as_str().to_owned(),
            base_url: known.base_url.as_str().to_owned(),
            freshness_seconds: known.freshness_seconds.to_string(),
            request_timeout_seconds: known.request_timeout_seconds.to_string(),
            api_key: String::new(),
            signing_key: String::new(),
        }
    }
}

/// The posted settings for the lab app called `name` with key names `keys`.
fn settings(
    posted: &Entered,
    name: Identifier,
    keys: [Identifier; 2],
) -> Result<Destination, SetupError> {
    let [api_key_name, signing_key_name] = keys;
    Ok(Destination {
        freshness_seconds: seconds(
            &posted.freshness_seconds,
            Destination::DEFAULT_FRESHNESS_SECONDS,
            "the order list time",
        )?,
        request_timeout_seconds: seconds(
            &posted.request_timeout_seconds,
            Destination::DEFAULT_REQUEST_TIMEOUT_SECONDS,
            "the request wait",
        )?,
        name,
        base_url: HttpsUrl::parse(posted.base_url.trim()).map_err(SetupError::InvalidUrl)?,
        api_key_name,
        signing_key_name,
    })
}

/// Turns the posted keys into checked values; errors name only the key.
fn keys(
    settings: &Destination,
    posted: &mut Entered,
    required: bool,
) -> Result<Vec<(Identifier, Secret)>, SetupError> {
    let api = std::mem::take(&mut posted.api_key);
    let signing = std::mem::take(&mut posted.signing_key);
    if required && (api.is_empty() || signing.is_empty()) {
        return Err(SetupError::KeysRequired);
    }
    let mut updates = Vec::new();
    if !api.is_empty() {
        let name = settings.api_key_name.clone();
        if !analyte_contract::api_key_valid(&api) {
            return Err(SetupError::InvalidKey(name.as_str().into()));
        }
        let secret = Secret::new(&name, api)?;
        updates.push((name, secret));
    }
    if !signing.is_empty() {
        let name = settings.signing_key_name.clone();
        if analyte_contract::SigningSecret::parse(&signing).is_err() {
            return Err(SetupError::InvalidKey(name.as_str().into()));
        }
        let secret = Secret::new(&name, signing)?;
        updates.push((name, secret));
    }
    Ok(updates)
}

/// What a save wrote, for the page and the audit row; never a key value.
pub(in crate::admin::setup) struct Saved {
    pub(in crate::admin::setup) settings: Destination,
    /// The names of the keys stored.
    pub(in crate::admin::setup) replaced: Vec<String>,
    pub(in crate::admin::setup) backups: Vec<PathBuf>,
    /// A settings file was written.
    pub(in crate::admin::setup) changed: bool,
    /// Machines that use it after the save.
    pub(in crate::admin::setup) users: Vec<String>,
}

/// Writes the files (checked commit), then stores the keys. A refused or rolled
/// back write never changes a key.
fn write(
    setup: &Setup,
    settings: Destination,
    changes: &[(PathBuf, String)],
    updates: Vec<(Identifier, Secret)>,
) -> Result<Saved, SetupError> {
    let replaced = updates
        .iter()
        .map(|(name, _)| name.as_str().to_owned())
        .collect();
    let backups = if changes.is_empty() {
        Vec::new()
    } else {
        site::commit(setup.config(), changes)?.backups
    };
    if !updates.is_empty() {
        Locked::new(&setup.data_directory)
            .replace(updates, Timestamp::now().as_second())
            .map_err(SetupError::KeysNotStored)?;
    }
    let users = scan(setup.config())
        .ok()
        .and_then(|scan| {
            scan.get(settings.name.as_str())
                .map(|known| known.users.clone())
        })
        .unwrap_or_default();
    Ok(Saved {
        settings,
        replaced,
        backups,
        changed: !changes.is_empty(),
        users,
    })
}

pub(in crate::admin::setup) fn add_blocking(
    setup: &Setup,
    mut posted: Entered,
) -> Result<Saved, SetupError> {
    let _writing = setup.writing()?;
    let name = new_name(&posted.name)?;
    let scan = scan(setup.config())?;
    if scan.get(name.as_str()).is_some() {
        return Err(SetupError::LabAppTaken(name.as_str().into()));
    }
    file::refuse_lab_app_clash(scan.known.iter().map(|known| &known.settings.name), &name)?;
    let keys_named = [
        Identifier::new(&format!("{}-api", name.as_str())).map_err(|_| SetupError::LabAppName)?,
        Identifier::new(&format!("{}-signing", name.as_str()))
            .map_err(|_| SetupError::LabAppName)?,
    ];
    let settings = settings(&posted, name, keys_named)?;
    let after = scan.file.with(settings.clone())?;
    let updates = keys(&settings, &mut posted, true)?;
    let changes = [(file::path(setup.config()), after.text()?)];
    write(setup, settings, &changes, updates)
}

pub(in crate::admin::setup) fn save_blocking(
    setup: &Setup,
    name: &str,
    mut posted: Entered,
) -> Result<Saved, SetupError> {
    let _writing = setup.writing()?;
    let scan = scan(setup.config())?;
    let known = scan
        .get(name)
        .ok_or_else(|| SetupError::UnknownLabApp(name.into()))?;
    let current = &known.settings;
    let settings = settings(
        &posted,
        current.name.clone(),
        [
            current.api_key_name.clone(),
            current.signing_key_name.clone(),
        ],
    )?;
    if settings != *current && !known.sites.is_empty() {
        return Err(SetupError::LabAppInSiteFile(
            name.into(),
            known.sites.join(", "),
        ));
    }
    let after = scan.file.with(settings.clone())?;
    let updates = keys(&settings, &mut posted, false)?;
    let mut changes = Vec::new();
    if after != scan.file {
        changes.push((file::path(setup.config()), after.text()?));
    }
    // lab-apps.toml now holds it, so an older copy in the program file goes.
    if known.kept {
        changes.push((
            setup.config().to_owned(),
            entries::without_retired(&scan.program, name)?,
        ));
    }
    write(setup, settings, &changes, updates)
}

/// What a lab app form did, for the audit row and the log; the command line
/// names it when the engine makes the change (ADR 0028).
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::admin::setup) enum Change {
    Added,
    Saved,
}

impl Change {
    pub(in crate::admin::setup) const fn action(self) -> &'static str {
        match self {
            Self::Added => "lab-app-add",
            Self::Saved => "lab-app-save",
        }
    }

    pub(in crate::admin::setup) const fn verb(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Saved => "saved",
        }
    }
}

/// The audit reason for a lab app added or saved through `via`.
pub(in crate::admin::setup) fn reason(via: Via, change: Change, saved: &Saved) -> String {
    format!(
        "{} {} {}: address {}, order list current for {} s, request wait {} s, keys replaced: {}",
        via.lab_apps(),
        change.verb(),
        saved.settings.name.as_str(),
        saved.settings.base_url.as_str(),
        saved.settings.freshness_seconds,
        saved.settings.request_timeout_seconds,
        if saved.replaced.is_empty() {
            "none".into()
        } else {
            saved.replaced.join(", ")
        }
    )
}

/// Why `known` cannot be removed now: machines use it or results wait for it.
pub(super) fn blockers(known: &Known, waiting: Result<u64, &SetupError>) -> Vec<String> {
    let mut why = Vec::new();
    if !known.users.is_empty() {
        why.push(format!(
            "Machines send results to it: {}. Pick another lab app on their setup pages, or remove them, first.",
            known.users.join(", ")
        ));
    }
    match waiting {
        Ok(0) => {}
        Ok(count) => why.push(format!(
            "{} still wait for it. They are delivered first; remove it when the Overview shows nothing waiting.",
            format::amount(count, "result or order message", "results or order messages")
        )),
        Err(error) => why.push(format!(
            "What still waits for it could not be counted ({error}), so it is kept."
        )),
    }
    why
}

pub(in crate::admin::setup) fn remove_blocking(
    setup: &Setup,
    name: &str,
    waiting: &Result<u64, SetupError>,
) -> Result<(Vec<PathBuf>, &'static str), SetupError> {
    let _writing = setup.writing()?;
    let scan = scan(setup.config())?;
    let known = scan
        .get(name)
        .ok_or_else(|| SetupError::UnknownLabApp(name.into()))?;
    let why = blockers(known, waiting.as_ref().copied());
    if !why.is_empty() {
        return Err(SetupError::LabAppInUse(name.into(), why.join(" ")));
    }
    let mut changes = Vec::new();
    if known.in_file {
        changes.push((file::path(setup.config()), scan.file.without(name).text()?));
    }
    if known.kept {
        changes.push((
            setup.config().to_owned(),
            entries::without_retired(&scan.program, name)?,
        ));
    }
    let keys = KeySource::read(setup).kept(&known.settings);
    Ok((site::commit(setup.config(), &changes)?.backups, keys))
}

/// The audit reason for lab app `name` removed through `via`.
pub(in crate::admin::setup) fn removed_reason(via: Via, name: &str) -> String {
    format!(
        "{} removed lab app {name}; no machine used it and nothing waited for it; its keys stay in the locked file",
        via.lab_apps()
    )
}

/// What a save wrote, as the page's saved page says it, one line each; never
/// a key value. The command prints these whether it saved itself or the
/// engine did.
pub(in crate::admin::setup) fn saved_lines(change: Change, saved: &Saved) -> Vec<String> {
    let mut lines = vec![
        format!("{} is {}.", saved.settings.name.as_str(), change.verb()),
        format!("Address: {}", saved.settings.base_url.as_str()),
        if saved.users.is_empty() {
            "Used by no machine yet: pick it with analyte machines set <machine> --lab-app <name>."
                .to_owned()
        } else {
            format!("Used by {}.", saved.users.join(", "))
        },
    ];
    if !saved.changed {
        lines.push(SETTINGS_UNCHANGED.to_owned());
    }
    for key in &saved.replaced {
        lines.push(format!("Key stored: {key}"));
    }
    if !saved.replaced.is_empty() {
        lines.push(format::KEYS_APPLY.to_owned());
    }
    lines.extend(copies(&saved.backups));
    lines
}
