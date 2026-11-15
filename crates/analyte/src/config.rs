//! Bounded program TOML and machine snapshots, with no secret values.
use crate::{Error, invalid};
use analyte_config::{Draft, Validated};
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

/// A machine's installation identity and validated effective settings.
pub struct Machine {
    /// Unique installation identity, independent of the shipped definition name.
    pub(crate) id: analyte_model::Identifier,
    /// Exact reproducible machine configuration.
    pub(crate) snapshot: Validated,
}

/// Validated program settings. Paths are resolved relative to the program file.
pub struct Program {
    pub(crate) signing_rotation: std::collections::BTreeMap<String, Vec<analyte_model::Identifier>>,
    /// Protected directory containing the database and journals.
    pub(crate) data_directory: PathBuf,
    /// Explicit storage budgets.
    pub(crate) store_limits: analyte_store::Limits,
    /// Independently supervised machines.
    pub(crate) machines: Vec<Machine>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    signing_rotation: std::collections::BTreeMap<String, Vec<analyte_model::Identifier>>,
    schema_version: u32,
    data_directory: PathBuf,
    store: Limits,
    machines: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    reserve_bytes: u64,
    database_bytes: u64,
    record_bytes: usize,
    page_rows: usize,
    busy_timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: analyte_model::Identifier,
    definition: PathBuf,
    site: PathBuf,
    overrides: Option<PathBuf>,
}

fn read(path: &Path) -> Result<String, Error> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(1_048_577)
        .read_to_end(&mut bytes)?;
    if bytes.len() > analyte_config::MAX_FILE_BYTES {
        return Err(invalid("configuration exceeds 1 MiB"));
    }
    Ok(String::from_utf8(bytes)?)
}

impl Program {
    /// Reads bounded files and validates every machine through `analyte-config`.
    /// # Errors
    /// Reports path, TOML, bounds, duplicate identity and machine validation failures.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let file: File = toml::from_str(&read(path)?)?;
        if file.schema_version != 1
            || file.machines.is_empty()
            || file.machines.len() > 128
            || file.store.record_bytes == 0
            || file.store.record_bytes > 16_777_216
            || !(1..=65536).contains(&file.store.page_rows)
            || !(1..=1000).contains(&file.store.busy_timeout_ms)
            || file.store.database_bytes <= file.store.reserve_bytes
        {
            return Err(invalid(
                "invalid program version, machine count or storage limits",
            ));
        }
        let base = path
            .parent()
            .ok_or_else(|| invalid("configuration has no parent directory"))?;
        let mut machines: Vec<Machine> = Vec::new();
        for entry in file.machines {
            if machines.iter().any(|m| m.id == entry.id) {
                return Err(invalid("duplicate machine identity"));
            }
            let definition = base.join(entry.definition);
            let site = base.join(entry.site);
            let overrides = entry.overrides.map(|p| base.join(p));
            let replacement = match &overrides {
                Some(p) => read(p)?,
                None => "schema_version = 1".into(),
            };
            let snapshot = Draft::load(
                (&definition.display().to_string(), &read(&definition)?),
                (&site.display().to_string(), &read(&site)?),
                ("overrides", &replacement),
            )?
            .validate()?;
            if matches!(
                snapshot.effective().machine().framing.checksum_hex,
                Some(analyte_config::ChecksumHex::AcceptLowercase)
            ) {
                return Err(invalid(
                    "ASTM receive dialog currently supports strict checksum hex only",
                ));
            }
            if snapshot
                .effective()
                .machine()
                .transport
                .operation_timeout_ms
                > 60_000
            {
                return Err(invalid(
                    "receive transport operation timeout cannot exceed 60000 ms",
                ));
            }
            if snapshot.canonical().len() > file.store.record_bytes
                || snapshot.effective().machine().syntax.bytes > file.store.record_bytes
            {
                return Err(invalid(
                    "store record limit is smaller than machine message or effective config",
                ));
            }
            machines.push(Machine {
                id: entry.id,
                snapshot,
            });
        }
        if file.signing_rotation.len() > 128
            || file.signing_rotation.values().any(|names| names.len() > 7)
            || file.signing_rotation.keys().any(|name| {
                !machines
                    .iter()
                    .any(|m| m.snapshot.effective().site().destination.name.as_str() == name)
            })
        {
            return Err(invalid("invalid signing rotation references or count"));
        }
        for machine in &machines {
            let destination = &machine.snapshot.effective().site().destination;
            if machines.iter().any(|other| {
                let other = &other.snapshot.effective().site().destination;
                other.name == destination.name
                    && (other.base_url != destination.base_url
                        || other.api_key_name != destination.api_key_name
                        || other.signing_key_name != destination.signing_key_name)
            }) {
                return Err(invalid("inconsistent shared destination"));
            }
        }
        Ok(Self {
            signing_rotation: file.signing_rotation,
            data_directory: base.join(file.data_directory),
            store_limits: analyte_store::Limits {
                reserve_bytes: file.store.reserve_bytes,
                database_bytes: file.store.database_bytes,
                record_bytes: file.store.record_bytes,
                page_rows: file.store.page_rows,
                busy_timeout: Duration::from_millis(file.store.busy_timeout_ms),
            },
            machines,
        })
    }
}

impl Program {
    /// Borrows the protected directory without permitting mutation of validated settings.
    #[must_use]
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    /// Borrows validated machine snapshots in their configured order.
    #[must_use]
    pub fn machines(&self) -> &[Machine] {
        &self.machines
    }
}

impl Machine {
    /// Borrows the unique installation identity.
    #[must_use]
    pub fn id(&self) -> &analyte_model::Identifier {
        &self.id
    }

    /// Borrows the immutable validated snapshot.
    #[must_use]
    pub fn snapshot(&self) -> &Validated {
        &self.snapshot
    }
}
