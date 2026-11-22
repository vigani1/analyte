//! Builds the machine-readable files published in docs/contract/ and with each release: the contract v1 schema, its OpenAPI document and the machine catalog (ADR 0032).
#![forbid(unsafe_code)]
use std::path::{Path, PathBuf};

mod catalog;
pub use catalog::{Catalog, CatalogError, EvidenceLevel, Machine, Protocol};
mod openapi;

/// The folder, relative to the repository root, that holds the published files.
pub const FOLDER: &str = "docs/contract";

/// The published file names in [`FOLDER`], in the order [`files`] builds them.
pub const NAMES: [&str; 4] = [
    "contract-v1.schema.json",
    "openapi.json",
    "machines.json",
    "machines.schema.json",
];

/// One published file: its name in [`FOLDER`] and its exact text.
#[derive(Debug)]
pub struct Published {
    /// The file name, one of [`NAMES`].
    pub name: &'static str,
    /// The exact text: pretty JSON with sorted keys and a final newline.
    pub text: String,
}

/// The repository root this crate was built from.
#[must_use]
pub fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Why the published files could not be built.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The catalog could not be read from the `machines` folder.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// A file's value could not be written as JSON.
    #[error("{name}: {source}")]
    Json {
        /// The file name.
        name: &'static str,
        /// The encoding failure.
        source: serde_json::Error,
    },
}

/// Builds every published file: the schema and OpenAPI document from the
/// contract types, and the catalog from the `machines` folder `machines`.
///
/// # Errors
/// Returns why the catalog could not be read or a file not encoded.
pub fn files(machines: &Path) -> Result<Vec<Published>, Error> {
    let [schema, openapi, catalog, catalog_schema] = NAMES;
    let machines =
        serde_json::to_value(catalog::read(machines)?).map_err(|source| Error::Json {
            name: catalog,
            source,
        })?;
    [
        (schema, analyte_contract::schema::bundle()),
        (openapi, openapi::document()),
        (catalog, machines),
        (catalog_schema, catalog::schema()),
    ]
    .into_iter()
    .map(|(name, value)| published(name, value))
    .collect()
}

fn published(name: &'static str, mut value: serde_json::Value) -> Result<Published, Error> {
    sort(&mut value);
    let mut text =
        serde_json::to_string_pretty(&value).map_err(|source| Error::Json { name, source })?;
    text.push('\n');
    Ok(Published { name, text })
}

/// Sorts every object's keys, so the bytes do not depend on whether some
/// crate in the build turns on `serde_json`'s insertion order.
fn sort(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.sort_keys();
            map.values_mut().for_each(sort);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(sort),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}
