//! Reads the lab apps of an installation and refuses a name written differently in two places.
//!
//! A lab app is listed in `lab-apps.toml` beside the program file. Older setups
//! also write one inside a machine's site file (`[destination]`) or keep one in
//! the program file after its machine was removed (`[[retired_destinations]]`).
//! All three are read as they are; nothing is rewritten at startup. The same name
//! may appear in several places only with identical settings: otherwise Analyte
//! refuses to start and names both places, because guessing which address is
//! meant could send results to the wrong lab app.
//!
//! Two different names that share one key environment variable are refused
//! too, wherever they are written: two lab app names, or two key names.
use super::{Clashing, Error};
use crate::installation::secrets;
use analyte_config::{Destination, LAB_APPS_FILE, LabApps};
use analyte_model::Identifier;
use std::path::{Path, PathBuf};

/// A lab app the program knows outside any machine: listed in `lab-apps.toml`,
/// or kept in the program file by an older setup page. Results and order
/// messages waiting for it are delivered while no set-up machine uses it; its
/// orders are then never read.
pub struct LabApp {
    pub(crate) settings: Destination,
    pub(crate) limits: analyte_sink::Limits,
}

impl LabApp {
    /// Borrows the lab app's settings: name, address, key names and deadlines.
    #[must_use]
    pub fn settings(&self) -> &Destination {
        &self.settings
    }
}

/// Where one set of lab app settings is written, for the conflict message.
pub(super) struct Written {
    settings: Destination,
    place: String,
}

/// `lab-apps.toml` beside the program file `program`.
#[must_use]
pub(crate) fn path(program: &Path) -> PathBuf {
    program
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(LAB_APPS_FILE)
}

/// Reads `lab-apps.toml`; a missing file lists no lab apps.
/// # Errors
/// Reports read failures and a file the config crate refuses.
pub(crate) fn read(program: &Path) -> Result<LabApps, Error> {
    let file = path(program);
    let shown = file.display().to_string();
    match super::read(&file) {
        Ok(text) => Ok(LabApps::load(&shown, &text)?),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(LabApps::none(&shown))
        }
        Err(error) => Err(error),
    }
}

/// The lab apps of `lab-apps.toml`, then each kept `[[retired_destinations]]`
/// table not listed there; also returns every place written so far, for
/// [`Places::add`].
pub(super) fn validate(
    listed: &LabApps,
    retired: &[Destination],
    program: &Path,
) -> Result<(Vec<LabApp>, Places), Error> {
    if retired.len() > 128 {
        return Err(Error::TooManyRetiredDestinations);
    }
    let mut places = Places {
        written: Vec::new(),
        keys: Vec::new(),
    };
    for settings in listed.listed() {
        places.add(settings, listed.path().to_owned())?;
    }
    let mut lab_apps: Vec<LabApp> = listed.listed().iter().map(lab_app).collect();
    let kept = format!("{} [[retired_destinations]]", program.display());
    for (index, settings) in retired.iter().enumerate() {
        if retired
            .iter()
            .take(index)
            .any(|earlier| earlier.name == settings.name)
        {
            return Err(Error::DuplicateRetiredDestination(
                settings.name.as_str().into(),
            ));
        }
        settings.check(&program.display().to_string(), "retired_destinations")?;
        places.add(settings, kept.clone())?;
        if !lab_apps
            .iter()
            .any(|known| known.settings.name == settings.name)
        {
            lab_apps.push(lab_app(settings));
        }
    }
    Ok((lab_apps, places))
}

fn lab_app(settings: &Destination) -> LabApp {
    LabApp {
        limits: crate::runtime::delivery::limits(settings),
        settings: settings.clone(),
    }
}

/// Refuses lab app `name` when it clashes with one of `earlier` ([`secrets::clash`]).
/// # Errors
/// Names both lab apps and the key variable they would share.
pub(crate) fn refuse_lab_app_clash<'a>(
    earlier: impl IntoIterator<Item = &'a Identifier>,
    name: &Identifier,
) -> Result<(), Error> {
    refuse_clash(Clashing::LabApps, earlier, name)
}

fn refuse_clash<'a>(
    what: Clashing,
    earlier: impl IntoIterator<Item = &'a Identifier>,
    name: &Identifier,
) -> Result<(), Error> {
    let Some(first) = earlier
        .into_iter()
        .find(|earlier| secrets::clash(earlier.as_str(), name.as_str()))
    else {
        return Ok(());
    };
    let variable = match what {
        // The key name the Lab app page gives a lab app's API key.
        Clashing::LabApps => secrets::variable(&format!("{}-api", name.as_str())),
        Clashing::Keys => secrets::variable(name.as_str()),
    };
    Err(Error::NameClash {
        what,
        first: first.as_str().into(),
        second: name.as_str().into(),
        variable,
    })
}

/// Every place a lab app is written and every key name, checked as each is added.
pub(super) struct Places {
    written: Vec<Written>,
    keys: Vec<Identifier>,
}

impl Places {
    /// Records `settings` written at `place`.
    /// # Errors
    /// Refuses the same name written with other settings elsewhere, naming both
    /// places, and a lab app or key name that clashes with one added before.
    pub(super) fn add(&mut self, settings: &Destination, place: String) -> Result<(), Error> {
        if let Some(earlier) = self
            .written
            .iter()
            .find(|earlier| earlier.settings.name == settings.name)
        {
            if earlier.settings != *settings {
                return Err(Error::ConflictingLabApp {
                    name: settings.name.as_str().into(),
                    first: earlier.place.clone(),
                    second: place,
                });
            }
            return Ok(());
        }
        refuse_lab_app_clash(
            self.written.iter().map(|earlier| &earlier.settings.name),
            &settings.name,
        )?;
        self.key(&settings.api_key_name)?;
        self.key(&settings.signing_key_name)?;
        self.written.push(Written {
            settings: settings.clone(),
            place,
        });
        Ok(())
    }

    /// Records a key name.
    /// # Errors
    /// Refuses a key name that clashes with one added before.
    pub(super) fn key(&mut self, name: &Identifier) -> Result<(), Error> {
        refuse_clash(Clashing::Keys, &self.keys, name)?;
        if !self.keys.contains(name) {
            self.keys.push(name.clone());
        }
        Ok(())
    }
}
