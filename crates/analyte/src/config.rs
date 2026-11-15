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
    #[cfg(feature = "test-hooks")]
    pub(crate) law_committed: Option<std::sync::Arc<crate::test_hooks::Committed>>,
    #[cfg(feature = "test-hooks")]
    pub(crate) astm_committed: Option<std::sync::Arc<crate::test_hooks::AstmCommitted>>,
    /// Unique installation identity, independent of the shipped definition name.
    pub(crate) id: analyte_model::Identifier,
    /// Exact reproducible machine configuration.
    pub(crate) snapshot: Validated,
    pub(crate) listener: Option<std::net::TcpListener>,
}

/// Validated program settings. Paths are resolved relative to the program file.
pub struct Program {
    #[cfg(feature = "test-hooks")]
    pub(crate) stall_store_on_shutdown: bool,
    pub(crate) loopback_destinations: std::collections::BTreeMap<String, String>,
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
    /// Installs a one-shot LAB-29 raw-commit/pre-ACK pause on an injected loopback machine.
    /// # Errors
    /// Rejects unknown machines, non-LAW stacks and production listeners.
    #[cfg(feature = "test-hooks")]
    pub fn pause_law_committed_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::test_hooks::Committed>,
    ) -> Result<(), Error> {
        let machine = self
            .machines
            .iter_mut()
            .find(|m| m.id.as_str() == id)
            .ok_or_else(|| invalid("unknown test machine"))?;
        if machine.listener.is_none()
            || machine.snapshot.effective().machine().dialog.kind
                != analyte_config::DialogKind::LawManager
        {
            return Err(invalid(
                "commit hook requires injected LAW loopback listener",
            ));
        }
        machine.law_committed = Some(hook);
        Ok(())
    }
    /// Stalls the store at shutdown with a 500 ms process deadline, for child-process tests only.
    /// This deliberately never returns from a storage operation; the watchdog must exit 3.
    /// # Errors
    /// Requires injected loopback listeners for every machine; production entry points refuse it.
    #[cfg(feature = "test-hooks")]
    pub fn stall_store_on_shutdown_for_test(&mut self) -> Result<(), Error> {
        if self.machines.is_empty()
            || self
                .machines
                .iter()
                .any(|machine| machine.listener.is_none())
        {
            return Err(invalid(
                "stuck shutdown hook requires injected loopback listeners",
            ));
        }
        self.stall_store_on_shutdown = true;
        Ok(())
    }

    /// Installs a one-shot ASTM durable-frame/pre-ACK pause for a loopback test machine.
    /// # Errors
    /// Rejects unknown machines, non-ASTM stacks, or machines without an injected loopback listener.
    #[cfg(feature = "test-hooks")]
    pub fn pause_astm_committed_for_test(
        &mut self,
        id: &str,
        hook: std::sync::Arc<crate::test_hooks::AstmCommitted>,
    ) -> Result<(), Error> {
        let machine = self
            .machines
            .iter_mut()
            .find(|m| m.id.as_str() == id)
            .ok_or_else(|| invalid("unknown test machine"))?;
        if machine.listener.is_none()
            || machine.snapshot.effective().machine().dialog.kind
                != analyte_config::DialogKind::AstmLink
        {
            return Err(invalid(
                "commit hook requires injected ASTM loopback listener",
            ));
        }
        machine.astm_committed = Some(hook);
        Ok(())
    }

    /// Supplies an already bound loopback listener for deterministic integration tests.
    /// The caller can retain a clone to reserve the same address across restarts.
    /// # Errors
    /// Rejects unknown machines, non-loopback addresses and non-server configs.
    pub fn set_loopback_listener_for_test(
        &mut self,
        id: &str,
        listener: std::net::TcpListener,
    ) -> Result<(), Error> {
        let machine = self
            .machines
            .iter_mut()
            .find(|m| m.id.as_str() == id)
            .ok_or_else(|| invalid("unknown test machine"))?;
        if !listener.local_addr()?.ip().is_loopback()
            || machine.snapshot.effective().machine().transport.kind
                != analyte_config::TransportKind::TcpServer
            || listener.local_addr()?.to_string() != machine.snapshot.effective().site().endpoint
        {
            return Err(invalid(
                "test listener does not match configured loopback server",
            ));
        }
        listener.set_nonblocking(true)?;
        machine.listener = Some(listener);
        Ok(())
    }

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
            if snapshot.effective().machine().dialog.kind == analyte_config::DialogKind::LawManager
                && snapshot.effective().machine().transport.kind
                    != analyte_config::TransportKind::TcpServer
            {
                return Err(invalid(
                    "LAW result-only runtime requires an analyzer-initiated TCP server link",
                ));
            }
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
                #[cfg(feature = "test-hooks")]
                law_committed: None,
                #[cfg(feature = "test-hooks")]
                astm_committed: None,
                id: entry.id,
                snapshot,
                listener: None,
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
            #[cfg(feature = "test-hooks")]
            stall_store_on_shutdown: false,
            loopback_destinations: std::collections::BTreeMap::new(),
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
    /// Overrides a known destination for synthetic loopback HTTP tests only.
    /// Production `serve` and `run` reject programs containing these overrides.
    /// # Errors
    /// Rejects unknown destinations; the test service validates literal loopback URLs.
    pub fn set_loopback_destination_for_test(
        &mut self,
        name: &str,
        url: String,
    ) -> Result<(), Error> {
        if !self
            .machines
            .iter()
            .any(|m| m.snapshot.effective().site().destination.name.as_str() == name)
        {
            return Err(invalid("unknown test destination"));
        }
        self.loopback_destinations.insert(name.into(), url);
        Ok(())
    }

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
