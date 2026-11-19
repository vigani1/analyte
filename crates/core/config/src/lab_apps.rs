//! Reads `lab-apps.toml`, which lists each lab app once, and resolves the lab app a site file names.
//!
//! Each `[[lab_apps]]` table is written exactly like a site file's
//! `[destination]` table and is checked by the same rules. A site file picks one
//! with `lab_app = "<name>"`; one that names none uses the only lab app, and is
//! refused when there are several. The effective snapshot always holds the
//! resolved settings under `destination`, so moving a lab app from a site file
//! into this file leaves the snapshot bytes and their digest unchanged.
use crate::{Destination, Error};
use analyte_model::Identifier;
use serde::{Deserialize, Serialize};

/// The file name, beside the program file.
pub const LAB_APPS_FILE: &str = "lab-apps.toml";

/// Most lab apps one installation lists, matching the machine bound.
pub const MAX_LAB_APPS: usize = 128;

/// Written above the tables when the admin page writes the file.
const HEADER: &str = concat!(
    "# Lab apps: where Analyte sends results and reads orders. Each is listed once;\n",
    "# each machine's site file picks one with lab_app = \"<name>\". Written by the\n",
    "# Lab app page. Keys are never kept here: only their names, which point into the\n",
    "# locked credentials file or the service environment.\n",
);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema_version: u32,
    #[serde(default)]
    lab_apps: Vec<Destination>,
}

/// Why a lab app could not be chosen or listed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabAppProblem {
    /// The site file names a lab app the file does not list.
    #[error(
        "lab app {name} is not listed in {file}; add it on the Lab app page or correct the name"
    )]
    Unknown {
        /// The name written in the site file.
        name: String,
        /// Where lab apps are listed.
        file: String,
    },
    /// Several lab apps exist and the site file does not say which one.
    #[error(
        "{file} lists several lab apps; choose one in this site file with lab_app = \"<name>\""
    )]
    Choose {
        /// Where lab apps are listed.
        file: String,
    },
    /// No lab app is written anywhere this site file can use.
    #[error(
        "no lab app is set: add one on the Lab app page, or write lab_app = \"<name>\" or a [destination] table"
    )]
    Missing,
    /// The site file names a lab app and also writes its own.
    #[error("write either lab_app or a [destination] table, not both")]
    Both,
    /// One name is listed twice, so which settings to use is unknown.
    #[error("lab app {0} is listed twice; keep one [[lab_apps]] table per name")]
    Duplicate(String),
    /// More lab apps than the bound.
    #[error("at most 128 lab apps are supported")]
    TooMany,
}

/// Where a machine's effective lab app settings were written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabAppSource {
    /// The site file's own `[destination]` table.
    Site,
    /// `lab-apps.toml`, named by `lab_app` or as the only one.
    LabApps,
    /// The overrides file's `[destination]` table.
    Overrides,
}

/// The checked lab apps of one installation, each name once, in written order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabApps {
    path: String,
    listed: Vec<Destination>,
}

impl LabApps {
    /// No lab apps; `path` is where they would be listed, for messages.
    #[must_use]
    pub fn none(path: &str) -> Self {
        Self {
            path: path.into(),
            listed: Vec::new(),
        }
    }

    /// Parses and checks the file text: schema version 1, known keys only, each
    /// lab app by the same rules as a site file's `[destination]`, each name once.
    /// # Errors
    /// Refuses oversized or malformed text, invalid settings, a repeated name and
    /// more than 128 lab apps.
    pub fn load(path: &str, text: &str) -> Result<Self, Error> {
        let file: File = crate::load(path, text)?;
        Self::checked(path, file.lab_apps)
    }

    fn checked(path: &str, listed: Vec<Destination>) -> Result<Self, Error> {
        let problem = |problem| Error::LabApp {
            path: path.into(),
            problem,
        };
        if listed.len() > MAX_LAB_APPS {
            return Err(problem(LabAppProblem::TooMany));
        }
        for (index, lab_app) in listed.iter().enumerate() {
            lab_app.check(path, "lab_apps")?;
            if listed
                .iter()
                .take(index)
                .any(|earlier| earlier.name == lab_app.name)
            {
                return Err(problem(LabAppProblem::Duplicate(
                    lab_app.name.as_str().into(),
                )));
            }
        }
        Ok(Self {
            path: path.into(),
            listed,
        })
    }

    /// Where these lab apps are listed.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Every lab app, in written order.
    #[must_use]
    pub fn listed(&self) -> &[Destination] {
        &self.listed
    }

    /// The lab app called `name`, if listed.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Destination> {
        self.listed.iter().find(|app| app.name.as_str() == name)
    }

    /// These lab apps with `lab_app` added, or replacing the one of the same name
    /// in place.
    /// # Errors
    /// Refuses settings outside the `[destination]` bounds and more than 128 lab apps.
    pub fn with(&self, lab_app: Destination) -> Result<Self, Error> {
        let mut listed = self.listed.clone();
        match listed.iter_mut().find(|app| app.name == lab_app.name) {
            Some(slot) => *slot = lab_app,
            None => listed.push(lab_app),
        }
        Self::checked(&self.path, listed)
    }

    /// These lab apps without the one called `name`.
    #[must_use]
    pub fn without(&self, name: &str) -> Self {
        Self {
            path: self.path.clone(),
            listed: self
                .listed
                .iter()
                .filter(|app| app.name.as_str() != name)
                .cloned()
                .collect(),
        }
    }

    /// The file text: a short header, then one `[[lab_apps]]` table per lab app.
    /// Loading it gives these lab apps back.
    /// # Errors
    /// Reports a TOML serialization failure.
    pub fn text(&self) -> Result<String, Error> {
        let body = toml::to_string(&File {
            schema_version: 1,
            lab_apps: self.listed.clone(),
        })?;
        Ok(format!("{HEADER}{body}"))
    }

    /// The lab app a site file uses: its own `[destination]`, the one `lab_app`
    /// names, or the only one listed. Never guesses between several.
    pub(crate) fn resolve(
        &self,
        site: &str,
        own: Option<Destination>,
        named: Option<&Identifier>,
    ) -> Result<(Destination, LabAppSource), Error> {
        let refuse = |problem| Error::LabApp {
            path: site.into(),
            problem,
        };
        match (own, named) {
            (Some(_), Some(_)) => Err(refuse(LabAppProblem::Both)),
            (Some(own), None) => Ok((own, LabAppSource::Site)),
            (None, Some(name)) => self
                .get(name.as_str())
                .cloned()
                .map(|app| (app, LabAppSource::LabApps))
                .ok_or_else(|| {
                    refuse(LabAppProblem::Unknown {
                        name: name.as_str().into(),
                        file: self.path.clone(),
                    })
                }),
            (None, None) => match self.listed.as_slice() {
                [only] => Ok((only.clone(), LabAppSource::LabApps)),
                [] => Err(refuse(LabAppProblem::Missing)),
                [_, _, ..] => Err(refuse(LabAppProblem::Choose {
                    file: self.path.clone(),
                })),
            },
        }
    }
}
