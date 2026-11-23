//! Reads lab app settings and their machine assignments.

use crate::admin::setup::{SetupError, site};
use crate::installation::config::{
    folder,
    lab_apps::{self as file, Conflict, Written},
};
use analyte_config::{Destination, LabApps};

/// One lab app as the installation writes it today, and who uses it.
pub(in crate::admin::setup) struct Known {
    pub(in crate::admin::setup) settings: Destination,
    /// Listed in `lab-apps.toml`.
    pub(super) in_file: bool,
    /// Kept in the program file by an older setup page.
    pub(super) kept: bool,
    /// Machines whose site file writes it as its own `[destination]`.
    pub(super) sites: Vec<String>,
    /// Machines that send to it: by name, by their own table, or as the only one.
    pub(in crate::admin::setup) users: Vec<String>,
}

/// Every lab app of the installation, read leniently for display; the checked
/// commit after any write is what decides.
pub(in crate::admin::setup) struct Scan {
    pub(super) program: String,
    pub(in crate::admin::setup) file: LabApps,
    pub(in crate::admin::setup) known: Vec<Known>,
    /// Where each name was first written, by the loader's rule.
    written: Written<Place>,
    /// Every name written differently in two places.
    pub(in crate::admin::setup) conflicts: Vec<Conflict<Place>>,
}

/// Where the page found a lab app written.
#[derive(Clone)]
pub(in crate::admin::setup) enum Place {
    File,
    Kept,
    /// The site file of this machine.
    Site(String),
}

/// The page's sentence for a name written differently in two places; it names
/// the place read later.
pub(in crate::admin::setup) fn conflict_sentence(conflict: &Conflict<Place>) -> String {
    format!(
        "Lab app {} has different settings in two places (one is {}). Analyte cannot start until they are the same.",
        conflict.name.as_str(),
        match &conflict.second {
            Place::File => "this page".into(),
            Place::Kept => "kept from a removed machine".into(),
            Place::Site(id) => format!("machine {id}'s own settings"),
        }
    )
}

impl Scan {
    fn note(&mut self, settings: &Destination, place: Place) {
        match self.written.note(settings, place.clone()) {
            Ok(true) => self.known.push(Known {
                settings: settings.clone(),
                in_file: false,
                kept: false,
                sites: Vec::new(),
                users: Vec::new(),
            }),
            Ok(false) => {}
            Err(conflict) => self.conflicts.push(conflict),
        }
        let Some(known) = self
            .known
            .iter_mut()
            .find(|known| known.settings.name == settings.name)
        else {
            return;
        };
        match place {
            Place::File => known.in_file = true,
            Place::Kept => known.kept = true,
            Place::Site(id) => known.sites.push(id),
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
    pub(in crate::admin::setup) fn settings(&self) -> Vec<Destination> {
        self.known
            .iter()
            .map(|known| known.settings.clone())
            .collect()
    }

    /// The lab app called `name`.
    pub(in crate::admin::setup) fn get(&self, name: &str) -> Option<&Known> {
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

/// The lab app a site file's settings choose, read leniently for display: its
/// own complete `[destination]`, else the one `lab_app` names, else none
/// named (then the only lab app listed, if there is exactly one). The loader
/// refuses what this lets through, such as both written.
pub(in crate::admin::setup) enum SiteLabApp<'a> {
    Own(Destination),
    Named(&'a str),
    Unnamed,
}

/// Reads which lab app the site file settings `site` choose.
pub(in crate::admin::setup) fn site_lab_app(site: &toml::Table) -> SiteLabApp<'_> {
    let own = site
        .get("destination")
        .cloned()
        .and_then(|own| own.try_into::<Destination>().ok());
    match (own, site.get("lab_app").and_then(toml::Value::as_str)) {
        (Some(own), _) => SiteLabApp::Own(own),
        (None, Some(name)) => SiteLabApp::Named(name),
        (None, None) => SiteLabApp::Unnamed,
    }
}

/// Reads the program file, `lab-apps.toml` and each machine's site file.
pub(in crate::admin::setup) fn scan(config: &std::path::Path) -> Result<Scan, SetupError> {
    scan_with_site(config, None)
}

/// Scans all site assignments, borrowing the parsed site when the caller already loaded it.
pub(in crate::admin::setup) fn scan_with_site(
    config: &std::path::Path,
    current: Option<(&std::path::Path, &toml::Table)>,
) -> Result<Scan, SetupError> {
    let (program, listed) = site::entries(config)?;
    let lab_apps = file::read(config)?;
    let kept = kept(&program)?;
    let mut scan = Scan {
        program,
        file: lab_apps.clone(),
        known: Vec::new(),
        written: Written::new(),
        conflicts: Vec::new(),
    };
    for settings in lab_apps.listed() {
        scan.note(settings, Place::File);
    }
    for settings in &kept {
        scan.note(settings, Place::Kept);
    }
    let base = folder(config);
    let mut by_default = Vec::new();
    for entry in &listed {
        let id = entry.id.get_ref();
        let path = base.join(entry.site.get_ref());
        let loaded;
        let table = match current {
            Some((same, table)) if same == path => table,
            _ => {
                loaded = site::read_table(&path)?;
                &loaded
            }
        };
        match site_lab_app(table) {
            SiteLabApp::Own(own) => {
                scan.note(&own, Place::Site(id.clone()));
                scan.uses(own.name.as_str(), id);
            }
            SiteLabApp::Named(name) => scan.uses(name, id),
            SiteLabApp::Unnamed if entry.definition.is_some() => by_default.push(id.clone()),
            SiteLabApp::Unnamed => {}
        }
    }
    if let [only] = lab_apps.listed() {
        for id in by_default {
            scan.uses(only.name.as_str(), &id);
        }
    }
    Ok(scan)
}

/// Where the settings still write a lab app that is not in `lab-apps.toml`,
/// in words, with the exact place on hover.
pub(in crate::admin::setup) fn where_written(known: &Known) -> Vec<(String, &'static str)> {
    let mut places = Vec::new();
    if !known.sites.is_empty() {
        places.push((
            format!("Still kept in the settings of {}", known.sites.join(", ")),
            "[destination] in the machine's site file; saving the machine moves it to lab-apps.toml",
        ));
    }
    if known.kept {
        places.push((
            "Kept from a removed machine".to_owned(),
            "[[retired_destinations]] in the program file; saving it here moves it to lab-apps.toml",
        ));
    }
    places
}
