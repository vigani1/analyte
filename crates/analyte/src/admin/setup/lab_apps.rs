//! Serves the Lab app page: each lab app is set once (address, timing, keys) and machines pick it.
//!
//! Lab apps live in `lab-apps.toml` beside the program file. Older setups also
//! write one inside a machine's site file or keep one in the program file; the
//! page lists those too, and a save moves them into `lab-apps.toml`. Every write
//! goes through the same checked commit as the machine setup (backup, safe
//! write, reload, rollback) and applies after a restart. Keys go only to the
//! locked credentials file and are shown only as `set on <date>`.
use super::{
    Csrf, RELOAD, SetupError, after_write, audit, done, entries, incomplete, needs_login,
    off_thread, page, refused, site, stored,
};
use crate::admin::{
    markup::Markup,
    server::Context,
    sessions::Session,
    view::{self, plain},
};
use crate::installation::{
    config::{lab_apps as file, read},
    credentials::{Locked, Secret, Stored},
    secrets,
};
use analyte_config::{Destination, HttpsUrl, LabApps};
use analyte_model::Identifier;
use axum::{
    Extension, Form,
    extract::{Path, State},
    response::Response,
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

/// Where the page lives; the overview links here too.
pub(in crate::admin) const PAGE: &str = "/setup/lab-apps";

/// Defaults a new lab app starts with, the same as a `[destination]` table's.
const FRESHNESS: u64 = 30;
const REQUEST_TIMEOUT: u64 = 10;

/// One lab app as the installation writes it today, and who uses it.
pub(super) struct Known {
    pub(super) settings: Destination,
    /// Listed in `lab-apps.toml`.
    in_file: bool,
    /// Kept in the program file by an older setup page.
    kept: bool,
    /// Machines whose site file writes it as its own `[destination]`.
    sites: Vec<String>,
    /// Machines that send to it: by name, by their own table, or as the only one.
    pub(super) users: Vec<String>,
}

/// Every lab app of the installation, read leniently for display; the checked
/// commit after any write is what decides.
pub(super) struct Scan {
    program: String,
    pub(super) file: LabApps,
    pub(super) known: Vec<Known>,
    /// One sentence per name written differently in two places.
    conflicts: Vec<String>,
}

enum Place<'a> {
    File,
    Kept,
    Site(&'a str),
}

impl Scan {
    fn note(&mut self, settings: &Destination, place: &Place<'_>) {
        let known = match self
            .known
            .iter_mut()
            .find(|known| known.settings.name == settings.name)
        {
            Some(known) => {
                if known.settings != *settings {
                    self.conflicts.push(format!(
                        "Lab app {} has different settings in two places (one is {}). Analyte cannot start until they are the same.",
                        settings.name.as_str(),
                        match place {
                            Place::File => "this page".into(),
                            Place::Kept => "kept from a removed machine".into(),
                            Place::Site(id) => format!("machine {id}'s own settings"),
                        }
                    ));
                }
                known
            }
            None => {
                self.known.push(Known {
                    settings: settings.clone(),
                    in_file: false,
                    kept: false,
                    sites: Vec::new(),
                    users: Vec::new(),
                });
                let Some(known) = self.known.last_mut() else {
                    return;
                };
                known
            }
        };
        match place {
            Place::File => known.in_file = true,
            Place::Kept => known.kept = true,
            Place::Site(id) => known.sites.push((*id).to_owned()),
        }
    }

    fn uses(&mut self, name: &str, machine: &str) {
        if let Some(known) = self
            .known
            .iter_mut()
            .find(|known| known.settings.name.as_str() == name)
        {
            known.users.push(machine.to_owned());
        }
    }

    /// The settings of every lab app listed, wherever written.
    pub(super) fn settings(&self) -> Vec<Destination> {
        self.known
            .iter()
            .map(|known| known.settings.clone())
            .collect()
    }

    /// The lab app called `name`.
    pub(super) fn get(&self, name: &str) -> Option<&Known> {
        self.known
            .iter()
            .find(|known| known.settings.name.as_str() == name)
    }
}

/// The lab app destinations an older setup page kept in the program file.
fn kept(program: &str) -> Result<Vec<Destination>, SetupError> {
    let table: toml::Table = program.parse().map_err(SetupError::ProgramFile)?;
    match table.get("retired_destinations") {
        Some(listed) => listed.clone().try_into().map_err(SetupError::ProgramFile),
        None => Ok(Vec::new()),
    }
}

/// Reads the program file, `lab-apps.toml` and each machine's site file.
pub(super) fn scan(config: &std::path::Path) -> Result<Scan, SetupError> {
    let (program, listed) = site::entries(config)?;
    let lab_apps = file::read(config)?;
    let kept = kept(&program)?;
    let mut scan = Scan {
        program,
        file: lab_apps.clone(),
        known: Vec::new(),
        conflicts: Vec::new(),
    };
    for settings in lab_apps.listed() {
        scan.note(settings, &Place::File);
    }
    for settings in &kept {
        scan.note(settings, &Place::Kept);
    }
    let base = site::folder(config);
    let mut by_default = Vec::new();
    for entry in &listed {
        let id = entry.id.get_ref();
        let Some(table) = read(&base.join(entry.site.get_ref()))
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
        else {
            continue;
        };
        let own = table
            .get("destination")
            .cloned()
            .and_then(|own| own.try_into::<Destination>().ok());
        if let Some(own) = own {
            scan.note(&own, &Place::Site(id));
            scan.uses(own.name.as_str(), id);
        } else if let Some(name) = table.get("lab_app").and_then(toml::Value::as_str) {
            scan.uses(name, id);
        } else if entry.definition.is_some() {
            by_default.push(id.clone());
        }
    }
    if let [only] = lab_apps.listed() {
        for id in by_default {
            scan.uses(only.name.as_str(), &id);
        }
    }
    Ok(scan)
}

/// A name as one URL path segment; old names may hold any character.
pub(super) fn segment(name: &str) -> String {
    name.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Checks a new lab app's name: it also names its keys and appears in links, so
/// it uses only ASCII letters, digits, `-` and `_`, at most 64 characters.
fn new_name(text: &str) -> Result<Identifier, SetupError> {
    let text = text.trim();
    if text.is_empty()
        || text.len() > 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
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
async fn waiting(state: &Context) -> Result<Vec<(String, u64)>, SetupError> {
    let open = stored(state, |store| store.undelivered_by_destination())
        .await
        .map_err(SetupError::Store)?;
    Ok(open
        .into_iter()
        .map(|open| {
            (
                open.destination.as_str().to_owned(),
                open.sending.saturating_add(open.held),
            )
        })
        .collect())
}

fn count(waiting: &[(String, u64)], name: &str) -> u64 {
    waiting
        .iter()
        .find(|(destination, _)| destination == name)
        .map_or(0, |(_, count)| *count)
}

/// Where a lab app not yet on this page is kept, in words; the exact place on hover.
fn where_written(known: &Known) -> Markup {
    let mut html = Markup::new();
    if !known.sites.is_empty() {
        html.push_fragment(&view::exact(
            &format!("Still kept in the settings of {}", known.sites.join(", ")),
            "[destination] in the machine's site file; saving the machine moves it to lab-apps.toml",
        ));
    }
    if known.kept {
        if !known.sites.is_empty() {
            html.push_markup("; ");
        }
        html.push_fragment(&view::exact(
            "Kept from a removed machine",
            "[[retired_destinations]] in the program file; saving it here moves it to lab-apps.toml",
        ));
    }
    html
}

/// One key's state, never its value: "API key: set on 2026-11-19". The key
/// name shows on hover.
fn key_line(state: &Context, label: &str, name: &Identifier) -> Markup {
    let line = Markup::new()
        .markup("<span title=\"")
        .text(name.as_str())
        .markup("\">")
        .text(label)
        .markup("</span>: ");
    // The service's own rule decides; an unreadable file still lets a set
    // environment variable show, as it wins at startup too.
    let read = Locked::new(&state.setup.data_directory).read();
    let none = Stored::default();
    let stored = read.as_ref().unwrap_or(&none);
    match secrets::source(name.as_str(), |key| std::env::var(key).ok(), stored) {
        secrets::Source::Environment(_) => line
            .text("set in the service environment (")
            .text(secrets::variable(name.as_str()))
            .text("), which wins over a stored key"),
        secrets::Source::Stored { set_at, .. } => {
            let date = jiff::Timestamp::from_second(set_at)
                .map(|t| t.strftime("%Y-%m-%d").to_string())
                .unwrap_or_else(|_| "an unknown date".into());
            line.text(format!("set on {date}"))
        }
        secrets::Source::Missing => match read {
            Ok(_) => line.text("not set"),
            Err(error) => line.text(format!("cannot be checked: {error}")),
        },
    }
}

/// GET /setup/lab-apps: every lab app, who uses it, its keys and what waits for it.
pub(in crate::admin) async fn list(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    if session.filter(|Extension(s)| s.authenticated).is_none() {
        return needs_login();
    }
    let open = waiting(&state).await;
    let rendered = off_thread(&state, move |state| {
        let scan = scan(&state.config_path)?;
        Ok(render_list(state, &scan, open.as_deref().ok()))
    })
    .await;
    match rendered {
        Ok(html) => done(html),
        Err(error) => refused(&error),
    }
}

fn render_list(state: &Context, scan: &Scan, open: Option<&[(String, u64)]>) -> Markup {
    let mut html = page("Lab apps").markup(concat!(
        "<p class=\"lead\">A lab app is the lab software that receives results and gives ",
        "orders. Set each one once here: its address and keys. Each machine then picks one ",
        "on its setup page. Changes apply after a restart.</p>"
    ));
    for conflict in &scan.conflicts {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice bad\">")
                .text(conflict)
                .markup("</p>"),
        );
    }
    html.push_markup(concat!(
        "<section><div class=\"heading\"><h2>Lab apps</h2>",
        "<form method=\"get\" action=\"/setup/lab-apps/new\"><button>Add lab app</button></form></div>"
    ));
    if scan.known.is_empty() {
        html.push_markup(concat!(
            "<p class=\"notice info\">No lab app is set up yet. Add one; machines can then ",
            "send results to it.</p></section>"
        ));
        return html;
    }
    html.push_markup(concat!(
        "<div class=\"table\"><table class=\"stack\"><tr><th>Lab app</th><th>Address</th>",
        "<th>Used by</th><th>Keys</th><th title=\"Results and order messages not yet accepted\">",
        "Waiting to be sent</th><th></th></tr>"
    ));
    for known in &scan.known {
        let name = known.settings.name.as_str();
        let used = if known.users.is_empty() {
            "No machine".to_owned()
        } else {
            known.users.join(", ")
        };
        let waiting = match open {
            Some(open) => view::count(count(open, name)),
            None => Markup::new().text("Unknown"),
        };
        let mut row = Markup::new()
            .markup("<tr><td><strong>")
            .text(name)
            .markup("</strong>");
        if !known.in_file {
            row.push_fragment(
                &Markup::new()
                    .markup("<br><small>")
                    .fragment(&where_written(known))
                    .markup("</small>"),
            );
        }
        row.push_fragment(
            &Markup::new()
                .markup("</td><td data-label=\"Address\">")
                .text(known.settings.base_url.as_str())
                .markup("</td><td data-label=\"Used by\">")
                .text(used)
                .markup("</td><td data-label=\"Keys\">")
                .fragment(&key_line(state, "API key", &known.settings.api_key_name))
                .markup("<br>")
                .fragment(&key_line(
                    state,
                    "Signing key",
                    &known.settings.signing_key_name,
                ))
                .markup("</td><td data-label=\"Waiting to be sent\">")
                .fragment(&waiting)
                .markup("</td><td class=\"actions\"><div class=\"buttons\"><a class=\"button secondary\" href=\"")
                .text(format!("{PAGE}/{}", segment(name)))
                .markup("\">Edit</a><a class=\"button quiet-danger\" href=\"")
                .text(format!("{PAGE}/{}/remove", segment(name)))
                .markup("\">Remove</a></div></td></tr>"),
        );
        html.push_fragment(&row);
    }
    html.push_markup("</table></div></section>");
    html.push_markup("<p><a class=\"button secondary\" href=\"/setup\">Back to setup</a></p>");
    html
}

/// The add or edit form; `existing` is `None` for a new lab app.
fn form(state: &Context, session: &Session, existing: Option<&Known>) -> Markup {
    let (action, title) = match existing {
        Some(known) => (
            format!("{PAGE}/{}", segment(known.settings.name.as_str())),
            "Edit lab app",
        ),
        None => (format!("{PAGE}/new"), "Add a lab app"),
    };
    let settings = existing.map(|known| &known.settings);
    let mut html = page(title)
        .markup("<form method=\"post\" action=\"")
        .text(action)
        .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
        .text(session.csrf.form_value())
        .markup("\"><section><h2>Address</h2>");
    match settings {
        Some(settings) => html.push_fragment(
            &Markup::new()
                .markup("<dl><dt>Name</dt><dd><strong>")
                .text(settings.name.as_str())
                .markup(concat!(
                    "</strong> <span class=\"muted\">(cannot change: stored results ",
                    "wait under this name)</span></dd></dl>"
                )),
        ),
        None => html.push_markup(concat!(
            "<label class=\"field\"><span>Name</span><input name=\"name\" required ",
            "maxlength=\"64\" value=\"lab-app\"></label><p class=\"muted\">Letters, digits, - and _. ",
            "The name cannot change later: stored results wait under it.</p>"
        )),
    }
    if let Some(known) = existing.filter(|known| !known.sites.is_empty()) {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice\">This lab app is still kept in the settings of ")
                .text(known.sites.join(", "))
                .markup(concat!(
                    ". You can enter its keys here. To change its address or timing, first save ",
                    "that machine on its setup page, which moves the lab app to this page.</p>"
                )),
        );
    }
    html.push_fragment(
        &Markup::new()
            .markup("<label class=\"field\"><span>Address</span><input name=\"base_url\" required maxlength=\"4096\" placeholder=\"https://\" value=\"")
            .text(settings.map_or("", |s| s.base_url.as_str()))
            .markup(concat!(
                "\"></label><details class=\"support\"><summary>Timing (advanced)</summary>",
                "<label class=\"field\"><span>Order list counts as current for (seconds)</span>",
                "<input name=\"freshness_seconds\" inputmode=\"numeric\" maxlength=\"10\" value=\""
            ))
            .text(settings.map_or(FRESHNESS, |s| s.freshness_seconds))
            .markup(concat!(
                "\"></label><label class=\"field\"><span>Wait for each request (seconds, 1 to 120)</span>",
                "<input name=\"request_timeout_seconds\" inputmode=\"numeric\" maxlength=\"3\" value=\""
            ))
            .text(settings.map_or(REQUEST_TIMEOUT, |s| s.request_timeout_seconds))
            .markup(concat!(
                "\"></label><p class=\"muted\">The defaults suit most labs: 30 and 10 seconds.</p>",
                "</details></section><section><h2>Keys</h2><p>Keys are stored locked, so only ",
                "Analyte can read them, and are never shown again.</p>"
            )),
    );
    match settings {
        Some(settings) => html.push_fragment(
            &Markup::new()
                .markup("<ul><li>")
                .fragment(&key_line(state, "API key", &settings.api_key_name))
                .markup("</li><li>")
                .fragment(&key_line(state, "Signing key", &settings.signing_key_name))
                .markup(concat!(
                    "</li></ul><p class=\"muted\">Leave a box empty to keep the stored key.</p>",
                    "<label class=\"field\"><span>Replace API key</span>",
                    "<input name=\"api_key\" type=\"password\" autocomplete=\"off\" maxlength=\"4096\"></label>",
                    "<label class=\"field\"><span>Replace signing key</span>",
                    "<input name=\"signing_key\" type=\"password\" autocomplete=\"off\" maxlength=\"4096\"></label>",
                )),
        ),
        None => html.push_markup(concat!(
            "<label class=\"field\"><span>API key</span><input name=\"api_key\" type=\"password\" ",
            "required autocomplete=\"off\" maxlength=\"4096\"></label>",
            "<label class=\"field\"><span>Signing key</span><input name=\"signing_key\" type=\"password\" ",
            "required autocomplete=\"off\" maxlength=\"4096\"></label>",
        )),
    }
    html.push_markup(concat!(
        "</section><div class=\"actions-row\"><button>Save</button>",
        "<a class=\"button secondary\" href=\"/setup/lab-apps\">Cancel</a></div></form>"
    ));
    html
}

/// GET /setup/lab-apps/new: the form for one more lab app.
pub(in crate::admin) async fn add_form(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    done(form(&state, &session, None))
}

/// GET /setup/lab-apps/{name}: the edit form for one lab app.
pub(in crate::admin) async fn edit_form(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(name): Path<String>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let rendered = off_thread(&state, move |state| {
        let scan = scan(&state.config_path)?;
        let known = scan
            .get(&name)
            .ok_or_else(|| SetupError::UnknownLabApp(name.clone()))?;
        Ok(form(state, &session, Some(known)))
    })
    .await;
    match rendered {
        Ok(html) => done(html),
        Err(error) => refused(&error),
    }
}

/// The add or edit form as posted. Keys are moved into [`Secret`] at once.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Posted {
    #[expect(dead_code, reason = "the CSRF middleware has already checked it")]
    csrf: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    freshness_seconds: String,
    #[serde(default)]
    request_timeout_seconds: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    signing_key: String,
}

/// The posted settings for the lab app called `name` with key names `keys`.
fn settings(
    posted: &Posted,
    name: Identifier,
    keys: [Identifier; 2],
) -> Result<Destination, SetupError> {
    let [api_key_name, signing_key_name] = keys;
    Ok(Destination {
        freshness_seconds: seconds(&posted.freshness_seconds, FRESHNESS, "the order list time")?,
        request_timeout_seconds: seconds(
            &posted.request_timeout_seconds,
            REQUEST_TIMEOUT,
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
    posted: &mut Posted,
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
struct Saved {
    settings: Destination,
    replaced: Vec<String>,
    backups: Vec<PathBuf>,
    changed: bool,
    /// Machines that use it after the save.
    users: Vec<String>,
}

/// Writes the files (checked commit), then stores the keys. A refused or rolled
/// back write never changes a key.
fn write(
    state: &Context,
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
        site::commit(&state.config_path, changes)?.backups
    };
    if !updates.is_empty() {
        Locked::new(&state.setup.data_directory)
            .replace(updates, jiff::Timestamp::now().as_second())
            .map_err(SetupError::KeysNotStored)?;
    }
    let users = scan(&state.config_path)
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

fn add_blocking(state: &Context, mut posted: Posted) -> Result<Saved, SetupError> {
    let _writing = state.setup.writing()?;
    let name = new_name(&posted.name)?;
    let scan = scan(&state.config_path)?;
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
    let changes = [(file::path(&state.config_path), after.text()?)];
    write(state, settings, &changes, updates)
}

fn save_blocking(state: &Context, name: &str, mut posted: Posted) -> Result<Saved, SetupError> {
    let _writing = state.setup.writing()?;
    let scan = scan(&state.config_path)?;
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
        changes.push((file::path(&state.config_path), after.text()?));
    }
    // lab-apps.toml now holds it, so an older copy in the program file goes.
    if known.kept {
        changes.push((
            state.config_path.clone(),
            entries::without_retired(&scan.program, name)?,
        ));
    }
    write(state, settings, &changes, updates)
}

fn saved_page(session: &Session, saved: &Saved, audited: Result<(), SetupError>) -> Response {
    let mut html = page("Lab app saved")
        .markup("<section><dl><dt>Lab app</dt><dd><strong>")
        .text(saved.settings.name.as_str())
        .markup("</strong></dd><dt>Address</dt><dd>")
        .text(saved.settings.base_url.as_str())
        .markup("</dd><dt>Used by</dt><dd>")
        .text(if saved.users.is_empty() {
            "no machine yet: pick it on a machine's setup page".to_owned()
        } else {
            saved.users.join(", ")
        })
        .markup("</dd></dl><ul>");
    if !saved.changed {
        html.push_markup("<li>The settings were already the same; no file was written.</li>");
    }
    for name in &saved.replaced {
        html.push_fragment(
            &Markup::new()
                .markup("<li>Key stored: ")
                .text(name)
                .markup("</li>"),
        );
    }
    for backup in &saved.backups {
        html.push_fragment(
            &Markup::new()
                .markup("<li class=\"muted\">Copy of the old settings: ")
                .text(backup.display())
                .markup("</li>"),
        );
    }
    html.push_markup("</ul><p><a href=\"/setup/lab-apps\">Back to lab apps</a></p></section>");
    after_write(html, audited, session)
}

fn reason(action: &str, saved: &Saved) -> String {
    format!(
        "lab app page {action} {}: address {}, order list current for {} s, request wait {} s, keys replaced: {}",
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

/// POST /setup/lab-apps/new: check, write `lab-apps.toml`, store the keys.
pub(in crate::admin) async fn add(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    form: Result<Form<Posted>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(posted)) = form else {
        return incomplete(RELOAD);
    };
    let saved = match off_thread(&state, move |state| add_blocking(state, posted)).await {
        Ok(saved) => saved,
        Err(error) => return refused(&error),
    };
    let name = saved.settings.name.as_str().to_owned();
    let audited = audit(
        &state,
        &session,
        "lab-app-add",
        &name,
        reason("added", &saved),
    )
    .await;
    tracing::info!(lab_app = name.as_str(), "lab app page added a lab app");
    saved_page(&session, &saved, audited)
}

/// POST /setup/lab-apps/{name}: check, write `lab-apps.toml`, store any new keys.
pub(in crate::admin) async fn save(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(name): Path<String>,
    form: Result<Form<Posted>, axum::extract::rejection::FormRejection>,
) -> Response {
    let Ok(Form(posted)) = form else {
        return incomplete(RELOAD);
    };
    let lab_app = name.clone();
    let saved = match off_thread(&state, move |state| save_blocking(state, &lab_app, posted)).await
    {
        Ok(saved) => saved,
        Err(error) => return refused(&error),
    };
    let audited = audit(
        &state,
        &session,
        "lab-app-save",
        &name,
        reason("saved", &saved),
    )
    .await;
    tracing::info!(lab_app = name.as_str(), "lab app page saved a lab app");
    saved_page(&session, &saved, audited)
}

/// Why `known` cannot be removed now: machines use it or results wait for it.
fn blockers(known: &Known, waiting: Result<u64, &SetupError>) -> Vec<String> {
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
            plain::amount(count, "result or order message", "results or order messages")
        )),
        Err(error) => why.push(format!(
            "What still waits for it could not be counted ({error}), so it is kept."
        )),
    }
    why
}

/// GET /setup/lab-apps/{name}/remove: the plain confirmation, or why it is refused.
pub(in crate::admin) async fn confirm_remove(
    State(state): State<Arc<Context>>,
    session: Option<Extension<Arc<Session>>>,
    Path(name): Path<String>,
) -> Response {
    let Some(Extension(session)) = session.filter(|Extension(s)| s.authenticated) else {
        return needs_login();
    };
    let open = waiting(&state).await;
    let lab_app = name.clone();
    let found = off_thread(&state, move |state| {
        let scan = scan(&state.config_path)?;
        let known = scan
            .get(&lab_app)
            .ok_or_else(|| SetupError::UnknownLabApp(lab_app.clone()))?;
        Ok(blockers(
            known,
            open.as_ref().map(|open| count(open, &lab_app)),
        ))
    })
    .await;
    let why = match found {
        Ok(why) => why,
        Err(error) => return refused(&error),
    };
    let mut html = page("Remove a lab app")
        .markup("<p class=\"lead\"><strong>Remove lab app ")
        .text(&name)
        .markup(concat!(
            "?</strong> Results already sent and their history stay. Its keys stay ",
            "stored, locked.</p>"
        ));
    if why.is_empty() {
        html.push_fragment(
            &Markup::new()
                .markup("<p class=\"notice info\">No machine uses it and nothing waits for it.</p><form method=\"post\" action=\"")
                .text(format!("{PAGE}/{}/remove", segment(&name)))
                .markup("\"><input type=\"hidden\" name=\"csrf\" value=\"")
                .text(session.csrf.form_value())
                .markup(concat!(
                    "\"><div class=\"actions-row\"><button class=\"danger\">Remove</button>",
                    "<a class=\"button secondary\" href=\"/setup/lab-apps\">Keep it</a></div></form>"
                )),
        );
    } else {
        html.push_markup(
            "<div class=\"notice bad\"><p><strong>It cannot be removed now:</strong></p><ul>",
        );
        for line in why {
            html.push_fragment(&Markup::new().markup("<li>").text(line).markup("</li>"));
        }
        html.push_markup(
            "</ul></div><p><a class=\"button secondary\" href=\"/setup/lab-apps\">Back to lab apps</a></p>",
        );
    }
    done(html)
}

fn remove_blocking(
    state: &Context,
    name: &str,
    waiting: &Result<u64, SetupError>,
) -> Result<Vec<PathBuf>, SetupError> {
    let _writing = state.setup.writing()?;
    let scan = scan(&state.config_path)?;
    let known = scan
        .get(name)
        .ok_or_else(|| SetupError::UnknownLabApp(name.into()))?;
    let why = blockers(known, waiting.as_ref().copied());
    if !why.is_empty() {
        return Err(SetupError::LabAppInUse(name.into(), why.join(" ")));
    }
    let mut changes = Vec::new();
    if known.in_file {
        changes.push((
            file::path(&state.config_path),
            scan.file.without(name).text()?,
        ));
    }
    if known.kept {
        changes.push((
            state.config_path.clone(),
            entries::without_retired(&scan.program, name)?,
        ));
    }
    Ok(site::commit(&state.config_path, &changes)?.backups)
}

/// POST /setup/lab-apps/{name}/remove: refused while a machine uses it or results wait.
pub(in crate::admin) async fn remove(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Path(name): Path<String>,
    form: Result<Form<Csrf>, axum::extract::rejection::FormRejection>,
) -> Response {
    if form.is_err() {
        return incomplete(RELOAD);
    }
    let open = waiting(&state).await.map(|open| count(&open, &name));
    let lab_app = name.clone();
    let removed = off_thread(&state, move |state| remove_blocking(state, &lab_app, &open)).await;
    let backups = match removed {
        Ok(backups) => backups,
        Err(error) => return refused(&error),
    };
    let audited = audit(
        &state,
        &session,
        "lab-app-remove",
        &name,
        format!("lab app page removed lab app {name}; no machine used it and nothing waited for it; its keys stay in the locked file"),
    )
    .await;
    tracing::info!(lab_app = name.as_str(), "lab app page removed a lab app");
    let mut html = page("Lab app removed")
        .markup("<section><p>Lab app <strong>")
        .text(&name)
        .markup(
            "</strong> is no longer listed. Its keys stay stored, locked.</p><ul class=\"muted\">",
        );
    for backup in &backups {
        html.push_fragment(
            &Markup::new()
                .markup("<li>Copy of the old settings: ")
                .text(backup.display())
                .markup("</li>"),
        );
    }
    html.push_markup("</ul></section>");
    after_write(html, audited, &session)
}

/// The machine form's lab app step: a picker of the known lab apps, with
/// `current` (or the only one) chosen; a link to add one when there is none.
pub(super) fn picker(scan: &Scan, current: Option<&str>) -> Markup {
    if scan.known.is_empty() {
        return Markup::new().markup(concat!(
            "<p class=\"notice info\">No lab app is set up yet. ",
            "<a href=\"/setup/lab-apps/new\">Add a lab app</a> first (its address and keys), ",
            "then come back and choose it here.</p>"
        ));
    }
    let chosen = current
        .filter(|name| scan.get(name).is_some())
        .map(str::to_owned)
        .or_else(|| match scan.known.as_slice() {
            [only] => Some(only.settings.name.as_str().to_owned()),
            _ => None,
        });
    let mut html = Markup::new().markup(
        "<label class=\"field\"><span>Send results to</span><select name=\"lab_app\" required>",
    );
    if chosen.is_none() {
        html.push_markup("<option value=\"\" selected>Choose a lab app</option>");
    }
    for known in &scan.known {
        let name = known.settings.name.as_str();
        html.push_fragment(
            &Markup::new()
                .markup("<option value=\"")
                .text(name)
                .markup(if chosen.as_deref() == Some(name) {
                    "\" selected>"
                } else {
                    "\">"
                })
                .text(name)
                .markup(" (")
                .text(known.settings.base_url.as_str())
                .markup(")</option>"),
        );
    }
    html.markup(concat!(
        "</select></label><p class=\"muted\">Addresses and keys are set once on the ",
        "<a href=\"/setup/lab-apps\">Lab app page</a>.</p>"
    ))
}

#[cfg(test)]
mod tests {
    // Owner request 2026-11-19 (lab app set once): old names may hold any
    // character, so links carry them percent-encoded; new names are plain.
    #[test]
    fn names_are_plain_and_links_encoded() {
        assert_eq!(super::segment("lab app/ü"), "lab%20app%2F%C3%BC");
        assert_eq!(super::segment("lab-app_2"), "lab-app_2");
        for good in ["lab-app", "main_LIS", "A1"] {
            assert!(super::new_name(good).is_ok(), "{good}");
        }
        for bad in ["", "two words", "../up", "a.b", &"x".repeat(65)] {
            assert!(super::new_name(bad).is_err(), "{bad}");
        }
    }
}
