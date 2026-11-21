//! Finds the program file a command reads when `--config` does not name one.
//!
//! The order, first match wins: `--config`; the `ANALYTE_CONFIG` environment
//! variable; `analyte.toml` in the current folder, if it exists; the file the
//! installer or package wrote, if it exists (Windows:
//! `%ProgramData%\Analyte\config\analyte.toml`; Linux:
//! `/etc/analyte/analyte.toml`). An explicit choice (`--config` or the
//! variable) is used as given, so a wrong name fails on that name instead of
//! quietly falling through to another file.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

/// The environment variable that names the program file.
pub const ENVIRONMENT: &str = "ANALYTE_CONFIG";
/// The program file's name in the current folder.
const FILE_NAME: &str = "analyte.toml";

/// How the program file was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `--config` named it.
    Explicit,
    /// `ANALYTE_CONFIG` named it.
    Environment,
    /// `analyte.toml` in the current folder.
    CurrentFolder,
    /// The file the installer or package wrote.
    Installed,
}

impl Source {
    /// Plain words for the note on standard error.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Explicit => "named by --config",
            Self::Environment => "named by ANALYTE_CONFIG",
            Self::CurrentFolder => "in the current folder",
            Self::Installed => "the installed program file",
        }
    }
}

/// The program file a command reads, and how it was chosen.
#[derive(Debug, PartialEq, Eq)]
pub struct Found {
    /// The file, as given or as found.
    pub path: PathBuf,
    /// How it was chosen.
    pub source: Source,
}

/// Why no program file could be chosen.
#[derive(thiserror::Error)]
pub enum LocateError {
    /// Nothing named a file and no usual place holds one.
    #[error(
        "no program file found; looked for {}. Pass --config <file> or set {ENVIRONMENT}",
        looked_at(.looked)
    )]
    NotFound {
        /// Every place looked, in order.
        looked: Vec<PathBuf>,
    },
    /// A usual place holds something this account cannot check.
    #[error(
        "cannot check {}: {source}. The installed program file is readable only by administrators: run the command as administrator (an elevated PowerShell on Windows, sudo on Linux), or pass --config <file>",
        .path.display()
    )]
    Unreadable {
        /// The place that refused.
        path: PathBuf,
        /// The operating system's answer.
        source: io::Error,
    },
}

// The program prints a failed command's error in its debug form; this one is
// read by people looking for their program file, so it is the sentence itself.
impl std::fmt::Debug for LocateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, formatter)
    }
}

fn looked_at(looked: &[PathBuf]) -> String {
    looked
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The file the installer or package writes on this kind of system; none
/// where Analyte has no installer, or when Windows does not name its
/// `ProgramData` folder.
#[must_use]
pub fn installed() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("ProgramData")
            .map(|folder| Path::new(&folder).join(r"Analyte\config\analyte.toml"))
    }
    #[cfg(target_os = "linux")]
    {
        Some(PathBuf::from("/etc/analyte/analyte.toml"))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// Chooses the program file in the documented order.
///
/// # Errors
/// [`LocateError::NotFound`] names every place looked; [`LocateError::Unreadable`]
/// names a place this account may not check.
pub fn locate(explicit: Option<PathBuf>) -> Result<Found, LocateError> {
    choose(
        explicit,
        std::env::var_os(ENVIRONMENT),
        installed(),
        |path: &Path| std::fs::metadata(path).map(|metadata| metadata.is_file()),
    )
}

/// The order itself, with the environment and the file system passed in.
fn choose(
    explicit: Option<PathBuf>,
    environment: Option<OsString>,
    installed: Option<PathBuf>,
    is_file: impl Fn(&Path) -> io::Result<bool>,
) -> Result<Found, LocateError> {
    if let Some(path) = explicit {
        return Ok(Found {
            path,
            source: Source::Explicit,
        });
    }
    if let Some(path) = environment.filter(|value| !value.is_empty()) {
        return Ok(Found {
            path: path.into(),
            source: Source::Environment,
        });
    }
    let mut looked = Vec::new();
    for (path, source) in [
        (Some(PathBuf::from(FILE_NAME)), Source::CurrentFolder),
        (installed, Source::Installed),
    ] {
        let Some(path) = path else { continue };
        match is_file(&path) {
            Ok(true) => return Ok(Found { path, source }),
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(LocateError::Unreadable { path, source }),
        }
        looked.push(path);
    }
    Err(LocateError::NotFound { looked })
}

#[cfg(test)]
mod tests {
    use super::{Found, LocateError, Source, choose};
    use std::{
        io,
        path::{Path, PathBuf},
    };

    fn installed() -> Option<PathBuf> {
        Some(PathBuf::from("installed/analyte.toml"))
    }

    fn only(existing: &'static [&'static str]) -> impl Fn(&Path) -> io::Result<bool> {
        move |path| {
            if existing.iter().any(|name| Path::new(name) == path) {
                Ok(true)
            } else {
                Err(io::Error::from(io::ErrorKind::NotFound))
            }
        }
    }

    const EVERYWHERE: &[&str] = &["analyte.toml", "installed/analyte.toml"];

    // Owner request 2026-11-21 (engine-only install): --config wins over the
    // variable and every usual place, and is used even when it does not exist.
    #[test]
    fn explicit_config_wins() -> Result<(), LocateError> {
        let found = choose(
            Some("chosen.toml".into()),
            Some("variable.toml".into()),
            installed(),
            only(EVERYWHERE),
        )?;
        assert_eq!(
            found,
            Found {
                path: "chosen.toml".into(),
                source: Source::Explicit
            }
        );
        Ok(())
    }

    // The variable comes next, used as given; an empty value counts as unset.
    #[test]
    fn variable_comes_before_the_usual_places() -> Result<(), LocateError> {
        let found = choose(
            None,
            Some("variable.toml".into()),
            installed(),
            only(EVERYWHERE),
        )?;
        assert_eq!(found.path, Path::new("variable.toml"));
        assert_eq!(found.source, Source::Environment);
        let found = choose(None, Some("".into()), installed(), only(EVERYWHERE))?;
        assert_eq!(found.source, Source::CurrentFolder);
        Ok(())
    }

    // The current folder keeps a developer's workflow; the installed file is
    // used only when the current folder has none.
    #[test]
    fn current_folder_comes_before_the_installed_file() -> Result<(), LocateError> {
        let found = choose(None, None, installed(), only(EVERYWHERE))?;
        assert_eq!(found.path, Path::new("analyte.toml"));
        assert_eq!(found.source, Source::CurrentFolder);
        let found = choose(None, None, installed(), only(&["installed/analyte.toml"]))?;
        assert_eq!(found.path, Path::new("installed/analyte.toml"));
        assert_eq!(found.source, Source::Installed);
        Ok(())
    }

    // Nothing found: the error names every place looked, in order. A folder
    // named analyte.toml is not a program file.
    #[test]
    fn nothing_found_names_every_place() {
        let error = choose(None, None, installed(), |_: &Path| Ok(false));
        let Err(LocateError::NotFound { looked }) = error else {
            panic!("expected NotFound, got {error:?}");
        };
        assert_eq!(
            looked,
            [
                PathBuf::from("analyte.toml"),
                PathBuf::from("installed/analyte.toml")
            ]
        );
        let message = LocateError::NotFound { looked }.to_string();
        assert!(message.contains("analyte.toml, installed/analyte.toml"));
        assert!(message.contains("--config <file>"));
        assert!(message.contains("ANALYTE_CONFIG"));
        let error = choose(None, None, None, |_: &Path| Ok(false));
        assert!(matches!(error, Err(LocateError::NotFound { looked }) if looked.len() == 1));
    }

    // A refused check is said as such, never taken as "not there".
    #[test]
    fn a_refused_check_is_its_own_error() {
        let error = choose(None, None, installed(), |path: &Path| {
            if path == Path::new("installed/analyte.toml") {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                Err(io::Error::from(io::ErrorKind::NotFound))
            }
        });
        let Err(LocateError::Unreadable { path, .. }) = &error else {
            panic!("expected Unreadable, got {error:?}");
        };
        assert_eq!(path, Path::new("installed/analyte.toml"));
        let message = error.map(|_| ()).unwrap_err().to_string();
        assert!(message.contains("run the command as administrator"));
    }
}
