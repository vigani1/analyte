//! Reads the lab apps of an installation and refuses a name written differently in two places.
//!
//! A lab app is listed in `lab-apps.toml` beside the program file. Older setups
//! also write one inside a machine's site file (`[destination]`) or keep one in
//! the program file after its machine was removed (`[[retired_destinations]]`).
//! All three are read as they are; nothing is rewritten at startup. The same name
//! may appear in several places only with identical settings: otherwise Analyte
//! refuses to start and names both places, because guessing which address is
//! meant could send results to the wrong lab app.
use super::Error;
use analyte_config::{Destination, LAB_APPS_FILE, LabApps};
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
    let mut places = Places(Vec::new());
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

/// Every place a lab app is written, checked as each is added.
pub(super) struct Places(Vec<Written>);

impl Places {
    /// Records `settings` written at `place`.
    /// # Errors
    /// Refuses the same name written with other settings elsewhere, naming both places.
    pub(super) fn add(&mut self, settings: &Destination, place: String) -> Result<(), Error> {
        if let Some(earlier) = self
            .0
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
        self.0.push(Written {
            settings: settings.clone(),
            place,
        });
        Ok(())
    }
}
