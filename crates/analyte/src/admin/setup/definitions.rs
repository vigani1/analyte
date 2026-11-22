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
use std::{
    io::Read as _,
    path::{Path, PathBuf},
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
            .filter(|name| key_part(name))
    }
    let folder = definition.parent();
    let vendor = folder.and_then(Path::parent);
    let shipped = root.is_some_and(|root| vendor.and_then(Path::parent) == Some(root));
    if let (true, Some(folder), Some(vendor_name), Some(model)) =
        (shipped, folder, key_name(vendor), key_name(folder))
    {
        return Some(display_name(folder, &format!("{vendor_name}/{model}")));
    }
    match table(definition) {
        Ok(document) => document
            .get("name")
            .and_then(toml::Value::as_str)
            .and_then(readable),
        Err(_) => None,
    }
}

/// Whether `name` can be a vendor or model folder: lower-case letters, digits
/// and hyphens, as shipped.
fn key_part(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
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
    /// A loadable `machine.toml` with its name and declared evidence level.
    Ready {
        path: PathBuf,
        name: String,
        level: String,
    },
    /// No `machine.toml`: the README lists what is missing. `record` is its
    /// `record.toml`, when it names the protocol and link, so the model can be
    /// connected to record only (ADR 0034).
    Blocked { record: Option<PathBuf> },
    /// `machine.toml` exists but its name or evidence level could not be read.
    Unreadable(String),
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
        if !key_part(name) || !entry.file_type().map_err(io)?.is_dir() {
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
            let state = if path.is_file() {
                head(&path)
            } else {
                State::Blocked {
                    record: record.is_file().then_some(record),
                }
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

/// A definition file as a TOML table, read up to the loader's size limit; the
/// error says why it could not be read.
fn table(path: &Path) -> Result<toml::Table, String> {
    let mut text = String::new();
    let limit = u64::try_from(analyte_config::MAX_FILE_BYTES).unwrap_or(u64::MAX);
    std::fs::File::open(path)
        .and_then(|file| file.take(limit).read_to_string(&mut text))
        .map_err(|error| error.to_string())?;
    text.parse::<toml::Table>()
        .map_err(|_| "not valid TOML".to_owned())
}

/// Reads only the definition's name and evidence level for display.
fn head(path: &Path) -> State {
    let document = match table(path) {
        Ok(document) => document,
        Err(reason) => return State::Unreadable(reason),
    };
    let name = document.get("name").and_then(toml::Value::as_str);
    let level = document
        .get("evidence")
        .and_then(|evidence| evidence.get("level"))
        .and_then(toml::Value::as_str);
    match (name, level) {
        (Some(name), Some(level)) => State::Ready {
            path: path.to_owned(),
            name: name.to_owned(),
            level: level.to_owned(),
        },
        _ => State::Unreadable("name or evidence level missing".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{State, display_name, list, model_name, tidy};

    // machines/README.md: a model folder without machine.toml is blocked; the
    // shipped tree lists the synthetic example as ready.
    #[test]
    fn shipped_tree_lists_ready_and_blocked_definitions() -> Result<(), crate::Error> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines");
        let definitions = list(&root)?;
        let state = |key: &str| definitions.iter().find(|d| d.key == key).map(|d| &d.state);
        assert!(matches!(
            state("example/synthetic-astm"),
            Some(State::Ready { name, level, .. }) if name == "generic-synthetic" && level == "documented"
        ));
        // Each first-lab model names its protocol and link, so it can record only.
        for key in [
            "biocare/biolyte-2000",
            "biomerieux/vidas",
            "boule/swelab-lumi",
            "dialab/autolyser",
            "roche/cobas-e411",
            "yhlo/iflash-1200",
        ] {
            assert!(
                matches!(state(key), Some(State::Blocked { record: Some(_) })),
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
        std::fs::write(&definition, "name = \"generic-synthetic\"\n")?;
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
