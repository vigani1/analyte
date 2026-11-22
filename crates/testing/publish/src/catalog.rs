//! Reads the machine catalog from the shipped `machines/<vendor>/<model>/` folders (ADR 0033).
//!
//! Every fact comes from a file in the model folder; nothing is guessed. The
//! name is the README's `# ` title and the evidence level its `Level:` line,
//! both required by `machines/README.md`. Protocol and code systems come from
//! `machine.toml`, so a blocked definition (no `machine.toml`) has none. The
//! engine's discovery test checks every `machine.toml` through the real loader
//! and that its README level matches.
use analyte_contract::document::ModelId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

/// Most model folders read; more are refused rather than cut.
const MAX_MODELS: usize = 256;
/// Most bytes read from one README or `machine.toml`.
const MAX_FILE_BYTES: u64 = 1_048_576;

/// The catalog: every shipped machine model, sorted by model id.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "Analyte machine catalog")]
pub struct Catalog {
    /// The catalog format version; always 1.
    #[schemars(range(min = 1, max = 1))]
    pub catalog_version: u8,
    /// One entry per `machines/<vendor>/<model>/` folder, sorted by `model`.
    pub machines: Vec<Machine>,
}

/// One machine model.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Machine {
    /// The model id that result deliveries and order events carry in
    /// `source.model`: the definition's vendor/model folder key.
    pub model: ModelId,
    /// The vendor folder key, the part of `model` before the slash.
    pub vendor: String,
    /// The model's name for people, from its README title.
    pub name: String,
    /// True when a `machine.toml` exists; false for a blocked definition,
    /// whose README lists what is missing.
    pub ready: bool,
    /// The evidence level the README declares.
    pub evidence_level: EvidenceLevel,
    /// The protocol the definition speaks; null for a blocked definition.
    pub protocol: Option<Protocol>,
    /// The machine's own test code system (the namespace of `test` in
    /// results and of order tests, ADR 0029); null when not declared.
    pub test_system: Option<String>,
    /// The machine's own specimen code system (the namespace of an order's
    /// `specimen_type`, ADR 0030); null when not declared.
    pub specimen_system: Option<String>,
}

/// How a definition was checked (ADR 0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    /// Built from documents only.
    Documented,
    /// Tested against an official simulator or vendor sample traces.
    Simulated,
    /// Tested against the real machine with recorded conversations.
    Proven,
}

/// The protocol keys a machine definition can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// ASTM LIS01/LIS02.
    Astm,
    /// HL7 version 2.
    Hl7,
    /// IHE Laboratory Analytical Workflow.
    Law,
    /// POCT1-A.
    Poct,
    /// The VIDAS PC Literal format.
    Vidas,
    /// The Autolyser format.
    Autolyser,
}

/// Why the catalog could not be read; each names the file or folder.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// A folder or file could not be read.
    #[error("{path}: {source}")]
    Io {
        /// The folder or file.
        path: PathBuf,
        /// The I/O failure.
        source: std::io::Error,
    },
    /// A folder name is not a model key part.
    #[error("{path}: {source}")]
    Key {
        /// The model folder.
        path: PathBuf,
        /// Why the name is refused.
        source: analyte_contract::document::InvalidModel,
    },
    /// The README has no `# ` title on its first line.
    #[error("{0}: README.md has no `# ` title on its first line")]
    Title(PathBuf),
    /// The README has no single `Level:` line.
    #[error("{0}: README.md needs exactly one `Level: documented|simulated|proven` line")]
    Level(PathBuf),
    /// `machine.toml` does not have the keys the catalog reads.
    #[error("{path}: {source}")]
    Definition {
        /// The definition file.
        path: PathBuf,
        /// The parse failure.
        source: toml::de::Error,
    },
    /// More model folders than the catalog reads.
    #[error("more than {MAX_MODELS} machine models")]
    TooMany,
}

/// The keys of `machine.toml` the catalog reads; other keys are ignored here
/// and checked by the real loader.
#[derive(Deserialize)]
struct Head {
    protocol: Protocol,
    mappings: Option<Codes>,
    codes: Option<Codes>,
    orders: Option<Orders>,
}

#[derive(Deserialize)]
struct Codes {
    test_system: Option<String>,
}

#[derive(Deserialize)]
struct Orders {
    specimen_system: Option<String>,
}

/// Reads every `<vendor>/<model>/` folder under `root`.
///
/// # Errors
/// Returns the first unreadable folder or file, a folder name that is not a
/// model key, a README without its title or level, or too many models.
pub(crate) fn read(root: &Path) -> Result<Catalog, CatalogError> {
    let mut machines = Vec::new();
    for vendor in folders(root)? {
        for folder in folders(&vendor)? {
            if machines.len() >= MAX_MODELS {
                return Err(CatalogError::TooMany);
            }
            machines.push(machine(&vendor, &folder)?);
        }
    }
    machines.sort_by(|a: &Machine, b| a.model.as_str().cmp(b.model.as_str()));
    Ok(Catalog {
        catalog_version: 1,
        machines,
    })
}

/// A folder's own name; a name that is not UTF-8 is empty, which no model key accepts.
fn name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

fn machine(vendor: &Path, folder: &Path) -> Result<Machine, CatalogError> {
    let model = ModelId::new(name(vendor), name(folder)).map_err(|source| CatalogError::Key {
        path: folder.to_owned(),
        source,
    })?;
    let readme = read_text(&folder.join("README.md"))?;
    let title = readme
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| CatalogError::Title(folder.to_owned()))?;
    let definition = folder.join("machine.toml");
    let head = if definition.is_file() {
        let text = read_text(&definition)?;
        Some(
            toml::from_str::<Head>(&text).map_err(|source| CatalogError::Definition {
                path: definition,
                source,
            })?,
        )
    } else {
        None
    };
    let test_system = head.as_ref().and_then(|head| {
        head.mappings
            .as_ref()
            .or(head.codes.as_ref())
            .and_then(|codes| codes.test_system.clone())
    });
    Ok(Machine {
        vendor: model.vendor().to_owned(),
        name: title.to_owned(),
        ready: head.is_some(),
        evidence_level: level(&readme).ok_or_else(|| CatalogError::Level(folder.to_owned()))?,
        protocol: head.as_ref().map(|head| head.protocol),
        test_system,
        specimen_system: head
            .and_then(|head| head.orders)
            .and_then(|orders| orders.specimen_system),
        model,
    })
}

/// The README's one `Level:` line.
fn level(readme: &str) -> Option<EvidenceLevel> {
    let mut levels = readme
        .lines()
        .filter_map(|line| line.strip_prefix("Level: "))
        .map(|level| match level.trim() {
            "documented" => Some(EvidenceLevel::Documented),
            "simulated" => Some(EvidenceLevel::Simulated),
            "proven" => Some(EvidenceLevel::Proven),
            _ => None,
        });
    match (levels.next(), levels.next()) {
        (Some(level), None) => level,
        _ => None,
    }
}

/// The subfolders of `path`, sorted; files beside them are ignored.
fn folders(path: &Path) -> Result<Vec<PathBuf>, CatalogError> {
    let io = |source| CatalogError::Io {
        path: path.to_owned(),
        source,
    };
    let mut found = Vec::new();
    for entry in std::fs::read_dir(path).map_err(io)? {
        let entry = entry.map_err(io)?;
        if entry.file_type().map_err(io)?.is_dir() {
            if found.len() >= MAX_MODELS {
                return Err(CatalogError::TooMany);
            }
            found.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

/// A text file, read up to [`MAX_FILE_BYTES`], with Windows line ends as LF.
fn read_text(path: &Path) -> Result<String, CatalogError> {
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_FILE_BYTES).read_to_string(&mut text))
        .map_err(|source| CatalogError::Io {
            path: path.to_owned(),
            source,
        })?;
    Ok(text.replace("\r\n", "\n"))
}

/// The JSON Schema of the catalog, as `docs/contract/machines.schema.json` holds it.
#[must_use]
pub(crate) fn schema() -> serde_json::Value {
    let schema = schemars::generate::SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<Catalog>();
    serde_json::Value::from(schema)
}
