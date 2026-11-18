//! Lists the shipped machine definitions a lab manager can choose on the setup page.
//!
//! Definitions live in `machines/<vendor>/<model>/` beside the program file. A model
//! folder without `machine.toml` is a blocked definition (see `machines/README.md`).
//! This list is for display and choice only; saving validates the chosen file
//! through the real loader.
use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

/// Most model folders listed; more are refused rather than cut.
const LIMIT: usize = 256;

/// One shipped definition, keyed by its `vendor/model` folder.
pub(super) struct Definition {
    pub(super) key: String,
    pub(super) state: State,
}

pub(super) enum State {
    /// A loadable `machine.toml` with its name and declared evidence level.
    Ready {
        path: PathBuf,
        name: String,
        level: String,
    },
    /// No `machine.toml`: the README lists what is missing.
    Blocked,
    /// `machine.toml` exists but its name or evidence level could not be read.
    Unreadable(String),
}

/// Why the definitions folder could not be listed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ListError {
    #[error(
        "the machine definitions folder {0} is missing; copy the machines folder from the release beside the program file"
    )]
    Missing(PathBuf),
    #[error("more than 256 machine definitions are installed")]
    TooMany,
    #[error("listing machine definitions in {path} failed: {source}")]
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
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !entry.file_type().map_err(io)?.is_dir()
        {
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
            let state = if path.is_file() {
                head(&path)
            } else {
                State::Blocked
            };
            definitions.push(Definition {
                key: format!("{vendor}/{model}"),
                state,
            });
        }
    }
    Ok(definitions)
}

/// Reads only the definition's name and evidence level for display.
fn head(path: &Path) -> State {
    let mut text = String::new();
    let limit = u64::try_from(analyte_config::MAX_FILE_BYTES).unwrap_or(u64::MAX);
    if let Err(error) =
        std::fs::File::open(path).and_then(|file| file.take(limit).read_to_string(&mut text))
    {
        return State::Unreadable(error.to_string());
    }
    let Ok(document) = text.parse::<toml::Table>() else {
        return State::Unreadable("not valid TOML".into());
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
    use super::{State, list};

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
        assert!(matches!(state("roche/cobas-e411"), Some(State::Blocked)));
        assert!(definitions.iter().all(|d| !d.key.contains("..")));
        Ok(())
    }
}
