//! Bounded program TOML and machine snapshots, with no secret values.
mod alerts;
use crate::{Error, invalid};
pub(crate) use alerts::Condition;
pub use alerts::{Alert, AlertConfigError, AlertKind};
use analyte_config::{Draft, Validated};
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

/// A machine's installation identity and validated effective settings.
pub struct Machine {
    pub(crate) capture: bool,
    pub(crate) destination_limits: analyte_sink::Limits,
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
    pub(crate) source_path: PathBuf,
    alerts: Vec<Alert>,
    alert_interval: Duration,
    admin_address: Option<std::net::SocketAddrV4>,
    #[cfg(feature = "test-hooks")]
    pub(crate) stall_store_on_shutdown: bool,
    pub(crate) loopback_destinations: std::collections::BTreeMap<String, String>,
    pub(crate) signing_rotation: std::collections::BTreeMap<String, Vec<analyte_model::Identifier>>,
    /// Protected directory containing the database and journals.
    pub(crate) data_directory: PathBuf,
    /// Explicit storage budgets.
    pub(crate) store_limits: analyte_store::Limits,
    store_startup_timeout: Duration,
    /// Independently supervised machines.
    pub(crate) machines: Vec<Machine>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    alerts: Vec<alerts::DraftAlert>,
    #[serde(default = "alerts::interval_default")]
    alert_interval_seconds: u64,
    admin: Option<Admin>,
    #[serde(default)]
    signing_rotation: std::collections::BTreeMap<String, Vec<analyte_model::Identifier>>,
    schema_version: u32,
    data_directory: PathBuf,
    store: Limits,
    machines: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Admin {
    port: u16,
    #[serde(default = "admin_loopback")]
    address: std::net::Ipv4Addr,
}

fn admin_loopback() -> std::net::Ipv4Addr {
    std::net::Ipv4Addr::LOCALHOST
}

impl Admin {
    fn address(&self) -> Result<std::net::SocketAddrV4, Error> {
        if self.address != std::net::Ipv4Addr::LOCALHOST {
            return Err(invalid("admin address must be 127.0.0.1"));
        }
        if self.port == 0 {
            return Err(invalid("admin port must be between 1 and 65535"));
        }
        Ok(std::net::SocketAddrV4::new(self.address, self.port))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    #[serde(
        rename = "startup_timeout_seconds",
        default = "default_startup_timeout",
        deserialize_with = "startup_timeout"
    )]
    startup_timeout: Duration,
    reserve_bytes: u64,
    database_bytes: u64,
    record_bytes: usize,
    page_rows: usize,
    busy_timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    #[serde(default)]
    capture: bool,
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
    /// Installs a one-shot LAW raw-commit/pre-response pause on an injected loopback machine.
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
            || !matches!(
                machine.snapshot.effective().stack(),
                analyte_config::Stack::Law(_)
            )
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
            || !matches!(
                machine.snapshot.effective().stack(),
                analyte_config::Stack::Astm(_)
            )
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
            || machine.snapshot.effective().stack().transport_kind()
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
        let admin_address = file.admin.as_ref().map(Admin::address).transpose()?;
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
                snapshot.effective().stack(),
                analyte_config::Stack::Astm(settings) if settings.checksum_hex == analyte_config::ChecksumHex::AcceptLowercase
            ) {
                return Err(invalid(
                    "ASTM receive dialog currently supports strict checksum hex only",
                ));
            }
            if snapshot.effective().stack().operation_timeout_ms() > 60_000 {
                return Err(invalid(
                    "receive transport operation timeout cannot exceed 60000 ms",
                ));
            }
            if snapshot.canonical().len() > file.store.record_bytes
                || snapshot.effective().stack().limits().bytes > file.store.record_bytes
            {
                return Err(invalid(
                    "store record limit is smaller than machine message or effective config",
                ));
            }
            if entry.capture
                && (!entry
                    .id
                    .as_str()
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                    || matches!(entry.id.as_str(), "." | ".."))
            {
                return Err(invalid(
                    "capture machine ID must be one directory component",
                ));
            }
            machines.push(Machine {
                capture: entry.capture,
                #[cfg(feature = "test-hooks")]
                law_committed: None,
                #[cfg(feature = "test-hooks")]
                astm_committed: None,
                destination_limits: crate::delivery::limits(
                    snapshot.effective().site().destination.freshness_seconds,
                ),
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
                        || other.signing_key_name != destination.signing_key_name
                        || other.freshness_seconds != destination.freshness_seconds)
            }) {
                return Err(invalid("inconsistent shared destination"));
            }
        }
        if !(1..=3600).contains(&file.alert_interval_seconds) {
            return Err(AlertConfigError::Interval.into());
        }
        if file.alerts.len() > 128 {
            return Err(AlertConfigError::TooMany.into());
        }
        let alerts = file
            .alerts
            .into_iter()
            .map(|alert| alert.validate(file.store.reserve_bytes))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, alert) in alerts.iter().enumerate() {
            if let Some(machine) = alert.machine()
                && !machines.iter().any(|installed| &installed.id == machine)
            {
                return Err(AlertConfigError::UnknownMachine.into());
            }
            if alerts.iter().take(index).any(|previous| {
                previous.kind() == alert.kind() && previous.machine() == alert.machine()
            }) {
                return Err(AlertConfigError::Duplicate.into());
            }
        }
        Ok(Self {
            source_path: std::path::absolute(path)?,
            alerts,
            alert_interval: Duration::from_secs(file.alert_interval_seconds),
            admin_address,
            #[cfg(feature = "test-hooks")]
            stall_store_on_shutdown: false,
            loopback_destinations: std::collections::BTreeMap::new(),
            signing_rotation: file.signing_rotation,
            data_directory: base.join(file.data_directory),
            store_startup_timeout: file.store.startup_timeout,
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
    /// Total bounded store-opening, integrity-check and migration budget.
    #[must_use]
    pub fn store_startup_timeout(&self) -> Duration {
        self.store_startup_timeout
    }

    /// Returns the bounded evaluation interval.
    #[must_use]
    pub fn alert_interval(&self) -> Duration {
        self.alert_interval
    }

    /// Borrows opt-in alert policies; an omitted list is empty.
    #[must_use]
    pub fn alerts(&self) -> &[Alert] {
        &self.alerts
    }

    /// Returns the configured IPv4 loopback admin endpoint, absent when disabled.
    #[must_use]
    pub fn admin_address(&self) -> Option<std::net::SocketAddrV4> {
        self.admin_address
    }

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

#[cfg(test)]
mod admin_tests {
    use super::*;

    // Plan section 11: admission is restricted to the local operator interface.
    #[test]
    fn plan11_admin_refuses_non_loopback_configuration() -> Result<(), Error> {
        for address in ["0.0.0.0", "192.0.2.1", "127.0.0.2"] {
            let admin: Admin = toml::from_str(&format!("port = 8080\naddress = '{address}'"))?;
            assert!(admin.address().is_err());
        }
        assert!(toml::from_str::<Admin>("port = 8080\naddress = '::'").is_err());
        Ok(())
    }

    // Plan section 11: the optional port selects only 127.0.0.1, never a wildcard bind.
    #[test]
    fn plan11_admin_port_defaults_to_ipv4_loopback() -> Result<(), Error> {
        let admin: Admin = toml::from_str("port = 8080")?;
        assert_eq!(admin.address()?.to_string(), "127.0.0.1:8080");
        assert!(toml::from_str::<Admin>("port = 65536").is_err());
        assert!(toml::from_str::<Admin>("port = 0")?.address().is_err());
        Ok(())
    }

    // Plan section 11: existing installations have no admin listener by default.
    #[test]
    fn plan11_admin_is_absent_by_default() -> Result<(), Error> {
        let file: File = toml::from_str(include_str!("../examples/analyte.toml"))?;
        assert!(file.admin.is_none());
        assert!(file.alerts.is_empty());
        assert_eq!(file.alert_interval_seconds, 30);
        Ok(())
    }
}

#[cfg(test)]
mod alert_program_tests {
    use super::*;

    // Plan section 11: policies identify installed machines and have unique identities.
    #[test]
    fn plan11_alert_program_validation() -> Result<(), Error> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let definition = root.join("../config/examples/hl7.toml").canonicalize()?;
        let site = root.join("../config/examples/site.toml").canonicalize()?;
        let original = include_str!("../examples/analyte.toml")
            .replace(
                "../../config/examples/hl7.toml",
                &definition.to_string_lossy().replace('\\', "/"),
            )
            .replace(
                "../../config/examples/site.toml",
                &site.to_string_lossy().replace('\\', "/"),
            );
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("program.toml");
        std::fs::write(&path, &original)?;
        assert!(Program::load(&path)?.alerts().is_empty());
        let policy = "\n[[alerts]]\nkind='restore_paused'\nresponder='lab duty officer'\n";
        std::fs::write(&path, format!("{original}{policy}"))?;
        assert_eq!(Program::load(&path)?.alerts().len(), 1);
        std::fs::write(&path, format!("{original}{policy}{policy}"))?;
        let error = Program::load(&path)
            .err()
            .ok_or("duplicate policy accepted")?;
        assert_eq!(
            error.downcast_ref::<AlertConfigError>(),
            Some(&AlertConfigError::Duplicate)
        );
        let silent = "\n[[alerts]]\nkind='machine_silent'\nthreshold=60\nresponder='lab duty officer'\nmachine='missing'\n";
        std::fs::write(&path, format!("{original}{silent}"))?;
        let error = Program::load(&path)
            .err()
            .ok_or("unknown machine accepted")?;
        assert_eq!(
            error.downcast_ref::<AlertConfigError>(),
            Some(&AlertConfigError::UnknownMachine)
        );
        std::fs::write(&path, format!("alert_interval_seconds=0\n{original}"))?;
        let error = Program::load(&path).err().ok_or("zero interval accepted")?;
        assert_eq!(
            error.downcast_ref::<AlertConfigError>(),
            Some(&AlertConfigError::Interval)
        );
        Ok(())
    }
}

fn default_startup_timeout() -> Duration {
    analyte_store::DEFAULT_STARTUP_TIMEOUT
}
fn startup_timeout<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Duration, D::Error> {
    let seconds = u64::deserialize(deserializer)?;
    if !(30..=3600).contains(&seconds) {
        return Err(serde::de::Error::custom(
            "store.startup_timeout_seconds must be between 30 and 3600 seconds",
        ));
    }
    Ok(Duration::from_secs(seconds))
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    const BASE: &str = "reserve_bytes=1\ndatabase_bytes=100000000\nrecord_bytes=65536\npage_rows=100\nbusy_timeout_ms=1000\n";
    #[test]
    fn startup_budget_is_typed_defaulted_and_bounded() -> Result<(), Error> {
        let limits: Limits = toml::from_str(BASE)?;
        assert_eq!(limits.startup_timeout, Duration::from_secs(600));
        for seconds in [30, 3600] {
            let limits: Limits =
                toml::from_str(&format!("{BASE}startup_timeout_seconds={seconds}"))?;
            assert_eq!(limits.startup_timeout, Duration::from_secs(seconds));
        }
        for seconds in [0, 29, 3601] {
            let error =
                toml::from_str::<Limits>(&format!("{BASE}startup_timeout_seconds={seconds}"))
                    .err()
                    .ok_or("invalid startup budget accepted")?;
            assert!(error.to_string().contains("between 30 and 3600 seconds"));
        }
        Ok(())
    }
}
