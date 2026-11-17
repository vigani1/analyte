//! Loads bounded program TOML and machine snapshots without secret values.
//! Kept whole: Program configuration types and boundary validation express one installation contract.
#[cfg(test)]
use analyte_fixtures::ConfigEdit as _;
mod alerts;
mod hooks;
pub(crate) use crate::machine::hooks::MachineHooks;
pub(crate) use hooks::ProgramHooks;
mod error;
mod webhooks;
pub(crate) use alerts::Condition;
pub use alerts::{Alert, AlertConfigError, AlertKind};
use analyte_config::{Draft, Validated};
pub use error::Error;
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
pub use webhooks::{AlertDelivery, AlertEndpoint, WebhookConfigError};

/// A machine's installation identity and validated effective settings.
pub struct Machine {
    pub(crate) capture: bool,
    pub(crate) destination_limits: analyte_sink::Limits,

    pub(crate) hooks: MachineHooks,
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
    alert_delivery: AlertDelivery,
    admin_address: Option<std::net::SocketAddrV4>,

    pub(crate) hooks: ProgramHooks,
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
    alert_delivery: webhooks::DraftDelivery,
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
            return Err(Error::AdminAddressNotLoopback);
        }
        if self.port == 0 {
            return Err(Error::AdminPortOutOfRange);
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
        return Err(Error::ConfigurationTooLarge);
    }
    Ok(String::from_utf8(bytes)?)
}

impl Program {
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
            return Err(Error::InvalidProgramLimits);
        }
        let base = path
            .parent()
            .ok_or_else(|| Error::ConfigurationHasNoParentDirectory)?;
        let mut machines: Vec<Machine> = Vec::new();
        for entry in file.machines {
            if machines.iter().any(|m| m.id == entry.id) {
                return Err(Error::DuplicateMachineIdentity);
            }
            machines.push(load_machine(entry, base, file.store.record_bytes)?);
        }
        if file.signing_rotation.len() > 128
            || file.signing_rotation.values().any(|names| names.len() > 7)
            || file.signing_rotation.keys().any(|name| {
                !machines
                    .iter()
                    .any(|m| m.snapshot.effective().site().destination.name.as_str() == name)
            })
        {
            return Err(Error::InvalidSigningRotation);
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
                return Err(Error::InconsistentSharedDestination);
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
            alert_delivery: file.alert_delivery.validate()?,
            alert_interval: Duration::from_secs(file.alert_interval_seconds),
            admin_address,

            hooks: ProgramHooks::default(),
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
    /// Borrows validated external alert delivery settings.
    #[must_use]
    pub fn alert_delivery(&self) -> &AlertDelivery {
        &self.alert_delivery
    }

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
    use crate::Error;

    #[test]
    #[cfg(feature = "test-hooks")]
    fn plan11_loopback_poll_override_keeps_production_defaults_and_refuses_unbounded_waits()
    -> Result<(), Error> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/analyte.toml");
        let mut program = Program::load(&path)?;
        let interval = |program: &Program| -> Result<Duration, Error> {
            Ok(program
                .machines
                .first()
                .ok_or("machine absent")?
                .destination_limits
                .poll_interval)
        };
        assert_eq!(interval(&program)?, Duration::from_secs(5));
        assert!(
            program
                .set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))
                .is_err()
        );
        assert_eq!(
            program.machines[0].destination_limits.store_timeout,
            Duration::from_secs(2)
        );
        assert!(
            program
                .set_loopback_store_budget_for_test("demo", analyte_fixtures::STORE_BUDGET)
                .is_err()
        );
        program.set_loopback_destination_for_test("demo", "http://127.0.0.1:12345".into())?;
        for invalid in [Duration::ZERO, Duration::from_secs(61)] {
            assert!(
                program
                    .set_loopback_store_budget_for_test("demo", invalid)
                    .is_err()
            );
            assert_eq!(
                program.machines[0].destination_limits.store_timeout,
                Duration::from_secs(2)
            );
        }
        program.set_loopback_store_budget_for_test("demo", analyte_fixtures::STORE_BUDGET)?;
        assert_eq!(
            program.machines[0].destination_limits.store_timeout,
            analyte_fixtures::STORE_BUDGET
        );
        assert_eq!(
            Program::load(&path)?.machines[0]
                .destination_limits
                .store_timeout,
            Duration::from_secs(2)
        );

        for invalid in [Duration::ZERO, Duration::from_secs(6)] {
            assert!(
                program
                    .set_loopback_poll_interval_for_test("demo", invalid)
                    .is_err()
            );
            assert_eq!(interval(&program)?, Duration::from_secs(5));
        }
        program.set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))?;
        assert_eq!(interval(&program)?, Duration::from_millis(100));
        assert!(
            crate::runtime::delivery::destinations(
                &program,
                crate::runtime::hooks::ServiceMode::Production
            )
            .is_err()
        );
        assert_eq!(interval(&Program::load(&path)?)?, Duration::from_secs(5));
        Ok(())
    }

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
        let file: File = toml::from_str(include_str!("../../../examples/analyte.toml"))?;
        assert!(file.admin.is_none());
        assert!(file.alerts.is_empty());
        assert_eq!(file.alert_interval_seconds, 30);
        Ok(())
    }
}

#[cfg(test)]
mod alert_program_tests {
    use super::*;
    use crate::Error;

    // Plan section 11: policies identify installed machines and have unique identities.
    #[test]
    fn plan11_alert_program_validation() -> Result<(), Error> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let definition = root
            .join("../core/config/examples/hl7.toml")
            .canonicalize()?;
        let site = root
            .join("../core/config/examples/site.toml")
            .canonicalize()?;
        let original = include_str!("../../../examples/analyte.toml")
            .rewrite(
                "../../core/config/examples/hl7.toml",
                &definition.to_string_lossy().replace('\\', "/"),
            )
            .rewrite(
                "../../core/config/examples/site.toml",
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
    use crate::Error;
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

fn load_machine(entry: Entry, base: &Path, record_bytes: usize) -> Result<Machine, Error> {
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
        analyte_config::Stack::Astm(analyte_config::RecordTransport::Stream(settings)) if settings.checksum_hex()
            == analyte_config::ChecksumHex::AcceptLowercase
    ) {
        return Err(Error::UnsupportedChecksumMode);
    }
    if snapshot.effective().stack().operation_timeout_ms() > 60_000 {
        return Err(Error::ReceiveTimeoutTooLong);
    }
    if snapshot.canonical().len() > record_bytes
        || snapshot.effective().stack().limits().bytes() > record_bytes
    {
        return Err(Error::StoreRecordLimitTooSmall);
    }
    if entry.capture
        && (!entry
            .id
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            || matches!(entry.id.as_str(), "." | ".."))
    {
        return Err(Error::InvalidCaptureMachineId);
    }
    Ok(Machine {
        capture: entry.capture,

        hooks: MachineHooks::default(),
        destination_limits: crate::runtime::delivery::limits(
            snapshot.effective().site().destination.freshness_seconds,
        ),
        id: entry.id,
        snapshot,
        listener: None,
    })
}
