//! Lists the shipped machine definitions a lab manager can choose on the setup page.
//!
//! Definitions live in `machines/<vendor>/<model>/` beside the program file. A model
//! folder without `machine.toml` is a blocked definition (see `machines/README.md`).
//! This list is for display and choice only; saving validates the chosen file
//! through the real loader. A definition's display name is the `# ` title on the
//! first line of its README (for example "Roche cobas e 411"); without one, the
//! page tidies the folder key instead. A machine's model name ([`model_name`])
//! never tidies folder names: outside a shipped model folder they say nothing
//! about the model.
use analyte_config::{DefinitionDetails, EvidenceLevel};
use analyte_contract::document::ModelId;
use analyte_model::Identifier;
use std::{
    io::Read as _,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Most model folders listed; more are refused rather than cut.
const LIMIT: usize = 256;

/// One shipped definition, keyed by its `vendor/model` folder.
pub(super) struct Definition {
    pub(super) key: String,
    /// The model's name for people, such as "Roche cobas e 411".
    pub(super) display: String,
    pub(super) state: State,
}

/// Most README bytes read to find the title line.
const TITLE_BYTES: u64 = 4096;
/// Longest title shown; a longer first line is not a title.
const TITLE_CHARS: usize = 80;

/// The model's name for people: the README's `# ` title, or the tidied key.
pub(super) fn display_name(folder: &Path, key: &str) -> String {
    title(folder).unwrap_or_else(|| tidy(key))
}

/// The `# ` title on the first line of `folder`'s README, when there is one
/// people can read.
fn title(folder: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(folder.join("README.md"))
        .and_then(|file| file.take(TITLE_BYTES).read_to_string(&mut text))
        .ok()?;
    text.lines()
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .and_then(readable)
}

/// `text` trimmed, when it is short, single-line and not empty.
fn readable(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= TITLE_CHARS && !text.chars().any(char::is_control))
        .then(|| text.to_owned())
}

/// The model name of the machine definition file `definition`. In a shipped
/// model folder (`<root>/<vendor>/<model>/`, where `root` is the shipped
/// `machines` folder) it is the name the setup list shows: the README title,
/// or the tidied `vendor/model` key. Anywhere else it is the definition's own
/// `name`, or none: folder names outside the shipped tree say nothing about
/// the model. Reads files, so call it off the async workers.
pub(in crate::admin) fn model_name(definition: &Path, root: Option<&Path>) -> Option<String> {
    fn key_name(part: Option<&Path>) -> Option<&str> {
        part.and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .filter(|name| ModelId::valid_part(name))
    }
    let folder = definition.parent();
    let vendor = folder.and_then(Path::parent);
    let shipped = root.is_some_and(|root| vendor.and_then(Path::parent) == Some(root));
    if let (true, Some(folder), Some(vendor_name), Some(model)) =
        (shipped, folder, key_name(vendor), key_name(folder))
    {
        return Some(display_name(folder, &format!("{vendor_name}/{model}")));
    }
    read(definition)
        .ok()?
        .identity
        .as_ref()
        .and_then(|(name, _)| readable(name.as_str()))
}

/// "roche/cobas-e411" becomes "Roche cobas e411": vendor capitalised, hyphens as spaces.
fn tidy(key: &str) -> String {
    let (vendor, model) = key.split_once('/').unwrap_or((key, ""));
    let mut vendor = vendor.chars();
    let mut name: String = vendor
        .next()
        .map(|first| first.to_uppercase().chain(vendor).collect())
        .unwrap_or_default();
    if !model.is_empty() {
        name.push(' ');
        name.push_str(&model.replace('-', " "));
    }
    name
}

pub(super) enum State {
    /// A machine definition and its parsed setup facts.
    Ready {
        source: Source,
        name: Identifier,
        level: EvidenceLevel,
    },
    /// No `machine.toml`; its capture profile can be chosen to record only (ADR 0034).
    Blocked { record: Option<Source> },
    /// A definition or capture profile that could not be read.
    Unreadable {
        path: PathBuf,
        error: Arc<FileError>,
    },
}

/// The file to use for a model and the setup facts it declares.
pub(super) struct Source {
    pub(super) path: PathBuf,
    pub(super) details: Arc<DefinitionDetails>,
}

/// Why the setup facts could not be read from a definition or capture profile.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FileError {
    #[error("reading the machine definition {path} failed: {source}")]
    Read {
        path: PathBuf,
        source: Box<crate::installation::config::Error>,
    },
    #[error("the machine definition is invalid: {0}")]
    Parse(#[from] analyte_config::Error),
    #[error("{0} is a capture profile; use it as record.toml, not machine.toml")]
    CaptureProfile(PathBuf),
    #[error("{0} is a complete definition; use it as machine.toml, not record.toml")]
    CompleteDefinition(PathBuf),
}

/// Reads one bounded machine definition or capture profile.
pub(super) fn read(path: &Path) -> Result<DefinitionDetails, FileError> {
    let text = crate::installation::config::read(path).map_err(|source| FileError::Read {
        path: path.to_owned(),
        source: Box::new(source),
    })?;
    Ok(DefinitionDetails::parse(
        &path.display().to_string(),
        &text,
    )?)
}

/// Why the definitions folder could not be listed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ListError {
    #[error(
        "the machine models folder {0} is missing; copy the machines folder from the release next to Analyte's settings file"
    )]
    Missing(PathBuf),
    #[error("more than 256 machine models are installed")]
    TooMany,
    #[error("listing the machine models in {path} failed: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

fn folders(path: &Path) -> Result<Vec<(String, PathBuf)>, ListError> {
    let io = |source| ListError::Io {
        path: path.to_owned(),
        source,
    };
    let mut found = Vec::new();
    for entry in std::fs::read_dir(path).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // Folder names are lower-case and hyphenated; anything else is not a definition.
        if !ModelId::valid_part(name) || !entry.file_type().map_err(io)?.is_dir() {
            continue;
        }
        if found.len() >= LIMIT {
            return Err(ListError::TooMany);
        }
        found.push((name.to_owned(), entry.path()));
    }
    found.sort();
    Ok(found)
}

/// Lists every `vendor/model` folder under `root`, sorted by key.
pub(super) fn list(root: &Path) -> Result<Vec<Definition>, ListError> {
    if !root.is_dir() {
        return Err(ListError::Missing(root.to_owned()));
    }
    let mut definitions = Vec::new();
    for (vendor, vendor_path) in folders(root)? {
        for (model, model_path) in folders(&vendor_path)? {
            if definitions.len() >= LIMIT {
                return Err(ListError::TooMany);
            }
            let path = model_path.join("machine.toml");
            let record = model_path.join(analyte_config::RECORD_FILE);
            let state = match path.try_exists() {
                Ok(true) => load(path, true),
                Ok(false) => match record.try_exists() {
                    Ok(true) => load(record, false),
                    Ok(false) => State::Blocked { record: None },
                    Err(error) => unreadable(record, error),
                },
                Err(error) => unreadable(path, error),
            };
            let key = format!("{vendor}/{model}");
            definitions.push(Definition {
                display: display_name(&model_path, &key),
                key,
                state,
            });
        }
    }
    Ok(definitions)
}

fn unreadable(path: PathBuf, error: std::io::Error) -> State {
    State::Unreadable {
        error: Arc::new(FileError::Read {
            path: path.clone(),
            source: Box::new(error.into()),
        }),
        path,
    }
}

fn load(path: PathBuf, complete: bool) -> State {
    let read = read(&path).and_then(|details| match (complete, &details.identity) {
        (true, None) => Err(FileError::CaptureProfile(path.clone())),
        (false, Some(_)) => Err(FileError::CompleteDefinition(path.clone())),
        (true, Some(_)) | (false, None) => Ok(details),
    });
    match read {
        Ok(details) => {
            let identity = details.identity.clone();
            let source = Source {
                path,
                details: Arc::new(details),
            };
            match identity {
                Some((name, level)) => State::Ready {
                    source,
                    name,
                    level,
                },
                None => State::Blocked {
                    record: Some(source),
                },
            }
        }
        Err(error) => State::Unreadable {
            path,
            error: Arc::new(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{State, display_name, list, model_name, tidy};

    fn example() -> &'static str {
        include_str!("../../../../../machines/example/synthetic-astm/machine.toml")
    }

    // machines/README.md: a model folder without machine.toml is blocked; the
    // shipped tree lists the synthetic example and documented Roche model as ready.
    #[test]
    fn shipped_tree_lists_ready_and_blocked_definitions() -> Result<(), crate::Error> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines");
        let definitions = list(&root)?;
        let state = |key: &str| definitions.iter().find(|d| d.key == key).map(|d| &d.state);
        assert!(matches!(
            state("example/synthetic-astm"),
            Some(State::Ready { name, level, .. })
                if name.as_str() == "generic-synthetic" && *level == super::EvidenceLevel::Documented
        ));
        assert!(matches!(
            state("roche/cobas-e411"),
            Some(State::Ready { name, level, .. })
                if name.as_str() == "cobas-e411" && *level == super::EvidenceLevel::Documented
        ));
        assert!(matches!(
            state("biomerieux/vidas"),
            Some(State::Ready { name, level, .. })
                if name.as_str() == "biomerieux-vidas" && *level == super::EvidenceLevel::Documented
        ));
        // First-lab models still awaiting definitions can record.
        for key in [
            "biocare/biolyte-2000",
            "biomerieux/vidas",
            "boule/swelab-lumi",
            "dialab/autolyser",
            "yhlo/iflash-1200",
        ] {
            assert!(
                matches!(
                    state(key),
                    Some(State::Ready { .. } | State::Blocked { record: Some(_) })
                ),
                "{key}"
            );
        }
        assert!(definitions.iter().all(|d| !d.key.contains("..")));
        let display = |key: &str| {
            definitions
                .iter()
                .find(|d| d.key == key)
                .map(|d| d.display.as_str())
        };
        assert_eq!(display("roche/cobas-e411"), Some("Roche cobas e 411"));
        assert_eq!(display("yhlo/iflash-1200"), Some("YHLO iFlash 1200"));
        assert_eq!(
            display("example/synthetic-astm"),
            Some("Synthetic ASTM example")
        );
        Ok(())
    }

    // AGENTS.md: evidence levels are documented, simulated or proven.
    #[test]
    fn unknown_evidence_level_is_reported() -> Result<(), crate::Error> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("machine.toml");
        let definition = example().replace("level = \"documented\"", "level = \"unknown\"");
        std::fs::write(&path, definition)?;
        assert!(matches!(
            super::load(path, true),
            State::Unreadable { error, .. } if error.to_string().contains("unknown variant")
                && error.to_string().contains("unknown")
        ));
        Ok(())
    }

    // ADR 0033: the catalog lists only valid vendor/model folder keys.
    #[test]
    fn setup_excludes_overlong_model_folders() -> Result<(), crate::Error> {
        let root = tempfile::tempdir()?;
        let folder = root.path().join("vendor").join("a".repeat(65));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("machine.toml"), example())?;
        assert!(list(root.path())?.is_empty());
        Ok(())
    }

    // Owner request 2026-11-19 (overview fixes): a machine's model name is the
    // setup list's name (README title, or the tidied shipped key) only inside a
    // shipped model folder, otherwise the definition's own name, otherwise
    // nothing; never words made from the folders a definition happens to sit in.
    #[test]
    fn model_name_never_comes_from_unrelated_folder_names() -> Result<(), crate::Error> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("machines");
        let shipped = root.join("roche").join("cobas-e411");
        std::fs::create_dir_all(&shipped)?;
        std::fs::write(shipped.join("README.md"), "# Roche cobas e 411\n")?;
        std::fs::write(shipped.join("machine.toml"), "name = \"cobas-e411\"\n")?;
        let definition = shipped.join("machine.toml");
        assert_eq!(
            model_name(&definition, Some(&root)).as_deref(),
            Some("Roche cobas e 411")
        );
        // A shipped folder whose README has no title reads as the setup list does.
        std::fs::write(shipped.join("README.md"), "Missing: settings\n")?;
        assert_eq!(
            model_name(&definition, Some(&root)).as_deref(),
            Some("Roche cobas e411")
        );

        // Out of the tree: a README title beside it and folder names are not used.
        let outside = temp.path().join("Tryapp").join("defs");
        std::fs::create_dir_all(&outside)?;
        std::fs::write(outside.join("README.md"), "# Some other title\n")?;
        let definition = outside.join("machine.toml");
        std::fs::write(&definition, example())?;
        assert_eq!(
            model_name(&definition, Some(&root)).as_deref(),
            Some("generic-synthetic")
        );
        assert_eq!(
            model_name(&definition, None).as_deref(),
            Some("generic-synthetic")
        );
        for unreadable in ["schema_version = 1\n", "name = \"\"\n", "name = [\n"] {
            std::fs::write(&definition, unreadable)?;
            assert_eq!(model_name(&definition, Some(&root)), None, "{unreadable}");
        }
        std::fs::remove_file(&definition)?;
        assert_eq!(model_name(&definition, Some(&root)), None);
        Ok(())
    }

    // Owner request 2026-11-19 (lab-manager admin pages, item 8): models show a
    // name for people from the README title, and fall back to a tidied key.
    #[test]
    fn display_name_reads_the_readme_title_or_tidies_the_key() -> Result<(), crate::Error> {
        let folder = tempfile::tempdir()?;
        assert_eq!(
            display_name(folder.path(), "roche/cobas-e411"),
            "Roche cobas e411"
        );
        let readme = folder.path().join("README.md");
        std::fs::write(&readme, "# Roche cobas e 411\n\n## Machine\n")?;
        assert_eq!(
            display_name(folder.path(), "roche/cobas-e411"),
            "Roche cobas e 411"
        );
        std::fs::write(&readme, "## Machine\n\nRoche cobas e 411\n")?;
        assert_eq!(
            display_name(folder.path(), "roche/cobas-e411"),
            "Roche cobas e411"
        );
        std::fs::write(&readme, "#   \n")?;
        assert_eq!(
            display_name(folder.path(), "boule/swelab-lumi"),
            "Boule swelab lumi"
        );
        std::fs::write(&readme, format!("# {}\n", "x".repeat(81)))?;
        assert_eq!(
            display_name(folder.path(), "yhlo/iflash-1200"),
            "Yhlo iflash 1200"
        );
        assert_eq!(tidy("example"), "Example");
        assert_eq!(tidy(""), "");
        Ok(())
    }
}
