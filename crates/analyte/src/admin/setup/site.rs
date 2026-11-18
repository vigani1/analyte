//! Builds, validates and writes a machine's site file, keeping the previous files.
//!
//! Validation runs first, in memory, through the same loader `check-config` uses.
//! Only then are files written: each old file is copied to `<name>.<time>.bak`, the
//! new text goes to `<name>.pending`, is flushed and renamed over the old one. The
//! whole program is then loaded again exactly as `check-config` does; if it fails,
//! the previous files are put back. Saving, adding and removing a machine all
//! write through [`commit`].
use super::SetupError;
use crate::installation::config::{Program, read};
use analyte_config::{Draft, Endpoint, HttpsUrl, UsbSerial, Validated};
use analyte_model::Identifier;
use std::{
    io::Write as _,
    ops::Range,
    path::{Path, PathBuf},
};

/// One `[[machines]]` entry of the program file, with paths resolved.
pub(super) struct Installed {
    /// Absent while the machine waits for setup.
    pub(super) definition: Option<PathBuf>,
    placement: Placement,
    pub(super) site: PathBuf,
    overrides: Option<PathBuf>,
}

/// Where a chosen definition goes in the program file's text.
enum Placement {
    /// The bytes of the current `definition` value.
    Replace(Range<usize>),
    /// The start of the `site` line, for an entry still waiting for setup.
    Insert(usize),
}

#[derive(serde::Deserialize)]
pub(super) struct ProgramSpans {
    #[serde(default)]
    pub(super) machines: Vec<EntrySpans>,
}

/// One `[[machines]]` entry as written, with the byte positions of its values.
#[derive(serde::Deserialize)]
pub(super) struct EntrySpans {
    pub(super) id: toml::Spanned<String>,
    pub(super) definition: Option<toml::Spanned<String>>,
    pub(super) site: toml::Spanned<String>,
    overrides: Option<String>,
}

/// Reads the program file and its entries as written.
pub(super) fn entries(config: &Path) -> Result<(String, Vec<EntrySpans>), SetupError> {
    let text = read(config)?;
    let spans: ProgramSpans =
        toml::from_str(&text).map_err(|error| SetupError::ProgramFile(error.to_string()))?;
    Ok((text, spans.machines))
}

/// The folder that holds the program file; entry paths are relative to it.
pub(super) fn folder(config: &Path) -> &Path {
    config.parent().unwrap_or_else(|| Path::new("."))
}

/// Reads the program file and finds the entry for machine `id`.
pub(super) fn installed(config: &Path, id: &str) -> Result<(String, Installed), SetupError> {
    let (text, listed) = entries(config)?;
    let base = folder(config);
    let entry = listed
        .into_iter()
        .find(|entry| entry.id.get_ref() == id)
        .ok_or_else(|| SetupError::UnknownMachine(id.into()))?;
    let placement = match &entry.definition {
        Some(definition) => Placement::Replace(definition.span()),
        None => Placement::Insert(site_line(&text, entry.site.span().start)?),
    };
    let installed = Installed {
        definition: entry.definition.map(|d| base.join(d.get_ref())),
        placement,
        site: base.join(entry.site.get_ref()),
        overrides: entry.overrides.map(|path| base.join(path)),
    };
    Ok((text, installed))
}

/// The start of the line holding the `site` value at `value`; the definition of
/// a waiting machine is inserted there, so it must be a plain `site = ...` line.
fn site_line(text: &str, value: usize) -> Result<usize, SetupError> {
    let start = text
        .get(..value)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline.saturating_add(1));
    match text.get(start..value) {
        Some(line) if line.trim_start().starts_with("site") => Ok(start),
        _ => Err(SetupError::ProgramFile(
            "write this machine's site = ... on its own line so setup can add the definition"
                .into(),
        )),
    }
}

/// How the site file names the machine's link.
pub(super) enum Connection {
    /// The USB adapter's factory serial number.
    UsbSerial(UsbSerial),
    /// A port name, socket address or folder, used exactly as written.
    Endpoint(String),
}

impl Connection {
    /// The site-file line this choice becomes, for display.
    pub(super) fn describe(&self) -> String {
        match self {
            Self::UsbSerial(serial) => format!("usb_serial = {}", serial.as_str()),
            Self::Endpoint(endpoint) => format!("endpoint = {endpoint}"),
        }
    }

    /// The same choice as the loader's endpoint type.
    pub(super) fn endpoint(&self) -> Endpoint {
        match self {
            Self::UsbSerial(serial) => Endpoint::UsbSerial(serial.clone()),
            Self::Endpoint(endpoint) => Endpoint::Fixed(endpoint.clone()),
        }
    }
}

/// The current site file with the page's choices applied; every other key
/// (retention, time zone, credential names, limits) is kept as it was.
pub(super) fn candidate(
    current: &str,
    connection: &Connection,
    base_url: &HttpsUrl,
    destination: &Identifier,
) -> Result<String, SetupError> {
    let mut table: toml::Table = current
        .parse()
        .map_err(|error: toml::de::Error| SetupError::SiteFile(error.to_string()))?;
    table.remove("endpoint");
    table.remove("usb_serial");
    let (key, value) = match connection {
        Connection::UsbSerial(serial) => ("usb_serial", serial.as_str()),
        Connection::Endpoint(endpoint) => ("endpoint", endpoint.as_str()),
    };
    table.insert(key.into(), toml::Value::String(value.into()));
    let section = table
        .entry("destination")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let toml::Value::Table(section) = section else {
        return Err(SetupError::SiteFile("destination is not a table".into()));
    };
    section.insert(
        "name".into(),
        toml::Value::String(destination.as_str().into()),
    );
    section.insert(
        "base_url".into(),
        toml::Value::String(base_url.as_str().into()),
    );
    let text = toml::to_string(&table).map_err(|error| SetupError::SiteFile(error.to_string()))?;
    Ok(format!("# Written by the Analyte setup page.\n{text}"))
}

/// Validates `definition` with `site_text` and the installed overrides, exactly as
/// the program loader does for one machine; nothing is written.
pub(super) fn validate(
    installed: &Installed,
    definition: &Path,
    site_text: &str,
) -> Result<Validated, SetupError> {
    let overrides = match &installed.overrides {
        Some(path) => read(path)?,
        None => "schema_version = 1".into(),
    };
    Ok(Draft::load(
        (&definition.display().to_string(), &read(definition)?),
        (&installed.site.display().to_string(), site_text),
        ("overrides", &overrides),
    )?
    .validate()?)
}

/// The program text with this entry's `definition` replaced, or added on its own
/// line before `site` for a waiting machine; comments and every other byte stay
/// as they were. A definition inside the program file's folder is
/// written relative to it, with `/`, as the shipped examples are.
fn with_definition(
    program: &str,
    config: &Path,
    installed: &Installed,
    definition: &Path,
) -> Result<String, SetupError> {
    let relative = config
        .parent()
        .and_then(|folder| definition.strip_prefix(folder).ok())
        .and_then(|path| {
            path.components()
                .map(|part| match part {
                    std::path::Component::Normal(name) => name.to_str(),
                    std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::CurDir
                    | std::path::Component::ParentDir => None,
                })
                .collect::<Option<Vec<_>>>()
        })
        .map(|parts| parts.join("/"));
    let text = relative.unwrap_or_else(|| definition.display().to_string());
    let value = toml::Value::String(text).to_string();
    let (range, line) = match &installed.placement {
        Placement::Replace(range) => (range.clone(), false),
        Placement::Insert(at) => (*at..*at, true),
    };
    let (Some(before), Some(after)) = (program.get(..range.start), program.get(range.end..)) else {
        return Err(SetupError::ProgramFile(
            "definition position changed".into(),
        ));
    };
    Ok(if line {
        format!("{before}definition = {value}\n{after}")
    } else {
        format!("{before}{value}{after}")
    })
}

/// Files written by a save.
pub(super) struct Written {
    pub(super) backups: Vec<PathBuf>,
}

/// Writes the site file (and the program entry's definition when it changed),
/// keeping backups; puts the old files back if the program no longer loads.
pub(super) fn apply(
    config: &Path,
    program: &str,
    installed: &Installed,
    definition: &Path,
    site_text: &str,
) -> Result<Written, SetupError> {
    let mut changes = vec![(installed.site.clone(), site_text.to_owned())];
    if installed
        .definition
        .as_deref()
        .is_none_or(|current| !same_file(definition, current))
    {
        changes.push((
            config.to_owned(),
            with_definition(program, config, installed, definition)?,
        ));
    }
    commit(config, &changes)
}

/// The time label of backups made now.
fn label() -> String {
    jiff::Timestamp::now()
        .strftime("%Y%m%dT%H%M%SZ")
        .to_string()
}

/// Writes each `(path, text)`, keeping a dated backup of every file that existed,
/// then loads the whole program as `check-config` does; if writing or loading
/// fails, every file is put back as it was.
pub(super) fn commit(config: &Path, changes: &[(PathBuf, String)]) -> Result<Written, SetupError> {
    let label = label();
    let mut backups = Vec::new();
    for (path, _) in changes {
        backups.push(backup(path, &label)?);
    }
    for (path, text) in changes {
        if let Err(error) = replace(path, text.as_bytes()) {
            restore(changes, &backups);
            return Err(error);
        }
    }
    if let Err(error) = Program::load(config) {
        restore(changes, &backups);
        return Err(SetupError::RolledBack(error.to_string()));
    }
    Ok(Written {
        backups: backups.into_iter().flatten().collect(),
    })
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn io(operation: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> SetupError {
    let path = path.to_owned();
    move |source| SetupError::Io {
        operation,
        path,
        source,
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    path.with_file_name(name)
}

/// Copies the current file to `<name>.<label>.bak` (or `<name>.<label>-<n>.bak`
/// when saved twice in one second); a missing file needs none. Never overwrites.
fn backup(path: &Path, label: &str) -> Result<Option<PathBuf>, SetupError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(io("read for backup", path))?;
    let mut attempt = 0_u8;
    let (target, mut file) = loop {
        let suffix = if attempt == 0 {
            format!(".{label}.bak")
        } else {
            format!(".{label}-{attempt}.bak")
        };
        let target = sibling(path, &suffix);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => break (target, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 99 => {
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(io("create backup", &target)(error)),
        }
    };
    file.write_all(&bytes)
        .map_err(io("write backup", &target))?;
    file.sync_all().map_err(io("flush backup", &target))?;
    Ok(Some(target))
}

/// Writes `<name>.pending`, flushes it and renames it over `path`.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    let pending = sibling(path, ".pending");
    let mut file = std::fs::File::create(&pending).map_err(io("create", &pending))?;
    file.write_all(bytes).map_err(io("write", &pending))?;
    file.sync_all().map_err(io("flush", &pending))?;
    drop(file);
    std::fs::rename(&pending, path).map_err(io("replace", path))
}

/// Moves a file the program no longer names to `<name>.<time>.bak`: a durable
/// copy first, then the original goes. A missing file needs nothing.
pub(super) fn retire(path: &Path) -> Result<Option<PathBuf>, SetupError> {
    let Some(kept) = backup(path, &label())? else {
        return Ok(None);
    };
    std::fs::remove_file(path).map_err(io("move to the backup", path))?;
    Ok(Some(kept))
}

/// Puts each backup back over its file. A file that did not exist before (it has
/// no backup) was written only by this change, so it is removed again.
fn restore(changes: &[(PathBuf, String)], backups: &[Option<PathBuf>]) {
    for ((path, _), backup) in changes.iter().zip(backups) {
        let Some(backup) = backup else {
            // Not found: the write failed before this file was reached.
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::error!(%error, "setup could not remove a file it had just written");
            }
            continue;
        };
        match std::fs::read(backup) {
            Ok(bytes) => {
                if let Err(error) = replace(path, &bytes) {
                    tracing::error!(%error, "setup could not restore a previous file");
                }
            }
            Err(error) => {
                tracing::error!(%error, "setup could not read a backup to restore it");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Owner request 2026-11-18 (setup page): changing the definition edits only
    // that value in the program file and keeps comments.
    #[test]
    fn definition_edit_keeps_the_rest_of_the_program_file() -> Result<(), crate::Error> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("analyte.toml");
        let program = "# keep me\nschema_version = 1\n[[machines]]\nid = 'a'\ndefinition = 'old.toml' # old\nsite = 's.toml'\n";
        std::fs::write(&config, program)?;
        let (text, installed) = super::installed(&config, "a")?;
        let target = std::path::Path::new(r"C:\Program Files\Analyte\machines\x\y\machine.toml");
        let edited = super::with_definition(&text, &config, &installed, target)?;
        assert!(edited.starts_with("# keep me\n"));
        assert!(edited.ends_with(" # old\nsite = 's.toml'\n"));
        let parsed: toml::Table = edited.parse()?;
        let definition = parsed
            .get("machines")
            .and_then(|m| m.get(0))
            .and_then(|m| m.get("definition"))
            .and_then(toml::Value::as_str);
        assert_eq!(definition, Some(target.to_str().ok_or("path")?));
        let inside = directory
            .path()
            .join("machines")
            .join("x")
            .join("machine.toml");
        let edited = super::with_definition(&text, &config, &installed, &inside)?;
        assert!(
            edited.contains("definition = \"machines/x/machine.toml\" # old"),
            "{edited}"
        );
        Ok(())
    }

    // Owner request 2026-11-18 (installers): a machine waiting for setup gets its
    // definition as a new line before `site`; nothing else changes.
    #[test]
    fn waiting_machine_gets_a_definition_line() -> Result<(), crate::Error> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("analyte.toml");
        let program = "schema_version = 1\n[[machines]]\nid = 'a'\n  site = 's.toml' # s\n";
        std::fs::write(&config, program)?;
        let (text, installed) = super::installed(&config, "a")?;
        assert!(installed.definition.is_none());
        let inside = directory
            .path()
            .join("machines")
            .join("x")
            .join("machine.toml");
        let edited = super::with_definition(&text, &config, &installed, &inside)?;
        assert_eq!(
            edited,
            "schema_version = 1\n[[machines]]\nid = 'a'\ndefinition = \"machines/x/machine.toml\"\n  site = 's.toml' # s\n"
        );
        // An inline entry has no `site` line to insert before; refuse rather than guess.
        std::fs::write(&config, "machines = [{ id = 'a', site = 's.toml' }]\n")?;
        assert!(super::installed(&config, "a").is_err());
        Ok(())
    }
}
