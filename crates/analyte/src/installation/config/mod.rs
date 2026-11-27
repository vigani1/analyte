//! Loads bounded program TOML and machine snapshots without secret values.
mod alerts;
pub(crate) mod connections;
mod hooks;
#[cfg(test)]
mod tests;
pub(crate) use crate::machine::hooks::MachineHooks;
pub(crate) use hooks::ProgramHooks;
mod error;
pub(crate) mod lab_apps;
mod webhooks;
pub(crate) use alerts::Condition;
pub use alerts::{Alert, AlertConfigError, AlertKind};
use analyte_config::Validated;
pub use error::{Clashing, Error};
pub use lab_apps::LabApp;
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
    /// The machine definition file it was loaded from; the admin page reads its
    /// model name from the definition's folder.
    pub(crate) definition: PathBuf,
    /// The model its results and order events name: the definition's
    /// `vendor/model` folder key (ADR 0033).
    pub(crate) model: analyte_contract::document::ModelId,
    /// The link light this machine's runtime writes and the admin page reads.
    pub(crate) status: crate::machine::link::status::Reporter,
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
    /// Machines listed without a definition: they wait for the setup page and do not run.
    awaiting_setup: Vec<analyte_model::Identifier>,
    /// Lab apps from `lab-apps.toml` and older kept destinations; those no
    /// machine uses are served for delivery only.
    pub(crate) lab_apps: Vec<LabApp>,
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
    /// May be empty or absent while the setup page is on: machines are added there.
    #[serde(default)]
    machines: Vec<Entry>,
    /// Lab app destinations kept by older setup pages when a machine using them
    /// was removed, written like a site file's `[destination]` table. Still read
    /// as lab apps; nothing writes new ones.
    #[serde(default)]
    retired_destinations: Vec<analyte_config::Destination>,
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
    /// Absent until the setup page chooses one; such a machine does not run.
    definition: Option<PathBuf>,
    site: PathBuf,
    overrides: Option<PathBuf>,
}

/// Reads one bounded, UTF-8 configuration file.
/// # Errors
/// Reports I/O failures, files over the size bound and invalid UTF-8.
pub(crate) fn read(path: &Path) -> Result<String, Error> {
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
            || file.machines.len() > 128
            || file.store.record_bytes == 0
            || file.store.record_bytes > 16_777_216
            || !(1..=65536).contains(&file.store.page_rows)
            || !(1..=1000).contains(&file.store.busy_timeout_ms)
            || file.store.database_bytes <= file.store.reserve_bytes
        {
            return Err(Error::InvalidProgramLimits);
        }
        // With no machine and no setup page the program could never do anything.
        if file.machines.is_empty() && admin_address.is_none() {
            return Err(Error::NoMachines);
        }
        let base = path
            .parent()
            .ok_or_else(|| Error::ConfigurationHasNoParentDirectory)?;
        let listed = lab_apps::read(path)?;
        let (lab_apps, mut places) = lab_apps::validate(&listed, &file.retired_destinations, path)?;
        let (machines, awaiting_setup) = entries(
            file.machines,
            &Loading {
                base,
                record_bytes: file.store.record_bytes,
                admin: admin_address.is_some(),
                lab_apps: &listed,
            },
            &mut places,
        )?;
        connections::check(&machines)?;
        if file.signing_rotation.len() > 128
            || file.signing_rotation.values().any(|names| names.len() > 7)
            || file
                .signing_rotation
                .keys()
                .any(|name| !knows_destination(&machines, &lab_apps, name))
        {
            return Err(Error::InvalidSigningRotation);
        }
        for name in file.signing_rotation.values().flatten() {
            places.key(name)?;
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
            awaiting_setup,
            lab_apps,
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

    /// Machines listed without a definition yet, in configured order; they do not run.
    #[must_use]
    pub fn awaiting_setup(&self) -> &[analyte_model::Identifier] {
        &self.awaiting_setup
    }

    /// The lab apps of `lab-apps.toml` in written order, then older kept
    /// destinations not listed there; includes those a set-up machine uses.
    #[must_use]
    pub fn lab_apps(&self) -> &[LabApp] {
        &self.lab_apps
    }

    /// Lab apps no set-up machine uses: only these get a delivery-only worker,
    /// because a machine's lab app already has a worker and an order poller.
    pub(crate) fn unused_lab_apps(&self) -> impl Iterator<Item = &LabApp> {
        self.lab_apps.iter().filter(|known| {
            !self
                .machines
                .iter()
                .any(|machine| machine.sends_to(known.settings.name.as_str()))
        })
    }
}

/// Whether `name` is a lab app some set-up machine sends to, or one listed in
/// `lab-apps.toml` or kept in the program file.
fn knows_destination(machines: &[Machine], lab_apps: &[LabApp], name: &str) -> bool {
    machines.iter().any(|machine| machine.sends_to(name))
        || lab_apps
            .iter()
            .any(|known| known.settings.name.as_str() == name)
}

impl Machine {
    /// Whether this machine's site sends results to the lab app called `name`.
    pub(crate) fn sends_to(&self, name: &str) -> bool {
        self.snapshot
            .effective()
            .site()
            .destination
            .as_ref()
            .is_some_and(|destination| destination.name.as_str() == name)
    }

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

fn default_startup_timeout() -> Duration {
    analyte_store::DEFAULT_STARTUP_TIMEOUT
}
fn startup_timeout<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Duration, D::Error> {
    let seconds = u64::deserialize(deserializer)?;
    if !(30..=3600).contains(&seconds) {
        return Err(serde::de::Error::custom(analyte_i18n::text(
            "setup-config-startup-timeout-range",
        )));
    }
    Ok(Duration::from_secs(seconds))
}

/// What every machine is loaded with.
struct Loading<'a> {
    base: &'a Path,
    record_bytes: usize,
    admin: bool,
    lab_apps: &'a analyte_config::LabApps,
}

/// Loads each listed machine; one without a definition waits for the setup page.
/// A lab app a machine writes itself (or in its overrides) is added to `places`,
/// so one name written differently anywhere is refused.
fn entries(
    listed: Vec<Entry>,
    loading: &Loading<'_>,
    places: &mut lab_apps::Places,
) -> Result<(Vec<Machine>, Vec<analyte_model::Identifier>), Error> {
    let mut machines: Vec<Machine> = Vec::new();
    let mut awaiting_setup = Vec::new();
    for entry in listed {
        if machines.iter().any(|m| m.id == entry.id) || awaiting_setup.contains(&entry.id) {
            return Err(Error::DuplicateMachineIdentity);
        }
        match entry.definition.clone() {
            Some(definition) => {
                let files = Files {
                    definition: loading.base.join(definition),
                    site: loading.base.join(&entry.site),
                    overrides: entry.overrides.as_ref().map(|p| loading.base.join(p)),
                };
                let machine = load_machine(entry, &files, loading)?;
                let effective = machine.snapshot.effective();
                let place = match effective.lab_app_source() {
                    analyte_config::LabAppSource::LabApps | analyte_config::LabAppSource::None => {
                        None
                    }
                    analyte_config::LabAppSource::Site => Some(files.site),
                    analyte_config::LabAppSource::Overrides => files.overrides,
                };
                if let (Some(place), Some(destination)) = (place, &effective.site().destination) {
                    places.add(destination, format!("{} [destination]", place.display()))?;
                }
                machines.push(machine);
            }
            // Only the setup page can give it a definition, so without the
            // page such an entry could never run: refuse it.
            None if !loading.admin => return Err(Error::SetupNeedsAdmin),
            None if entry.capture || entry.overrides.is_some() => {
                return Err(Error::AwaitingSetupWithSettings);
            }
            None => awaiting_setup.push(entry.id),
        }
    }
    Ok((machines, awaiting_setup))
}

/// The folder that holds the program file `program`; entry paths are relative to it.
pub(crate) fn folder(program: &Path) -> &Path {
    program.parent().unwrap_or_else(|| Path::new("."))
}

/// Whether `name` uses only ASCII letters, digits, `-` and `_`, so it can name
/// a file or folder and appear in a link without escaping.
pub(crate) fn plain_name(name: &str) -> bool {
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Reads the installation's store record limit from the program text being edited.
pub(crate) fn store_record_bytes(program: &str) -> Result<usize, Error> {
    #[derive(Deserialize)]
    struct Storage {
        store: Limits,
    }
    Ok(toml::from_str::<Storage>(program)?.store.record_bytes)
}

/// The files one machine entry names, resolved against the program folder.
struct Files {
    definition: PathBuf,
    site: PathBuf,
    overrides: Option<PathBuf>,
}

fn load_machine(entry: Entry, files: &Files, loading: &Loading<'_>) -> Result<Machine, Error> {
    let ValidatedMachine {
        snapshot,
        model,
        capture,
    } = validate_machine(
        (&entry.id, entry.capture),
        &files.definition,
        (&files.site, None),
        files.overrides.as_deref(),
        loading.record_bytes,
        loading.lab_apps,
    )?;
    let link = crate::machine::link::status::Link::of(snapshot.effective().stack());
    Ok(Machine {
        capture,

        hooks: MachineHooks::default(),
        destination_limits: crate::runtime::delivery::limits(
            snapshot.effective().site().destination.as_ref(),
        ),
        id: entry.id,
        status: if snapshot.effective().records_only() {
            crate::machine::link::status::Reporter::recording(link)
        } else {
            crate::machine::link::status::Reporter::new(link)
        },
        snapshot,
        listener: None,
        definition: files.definition.clone(),
        model,
    })
}

/// An effective machine snapshot checked against the program's requirements.
pub(crate) struct ValidatedMachine {
    /// The reproducible effective settings, bounded by the store record limit.
    pub(crate) snapshot: Validated,
    model: analyte_contract::document::ModelId,
    capture: bool,
}

/// Loads one machine and checks it against the runtime's constraints. Setup can
/// supply proposed site text in place of the file. This only reads settings.
pub(crate) fn validate_machine(
    (id, capture): (&analyte_model::Identifier, bool),
    definition: &Path,
    (site, proposed): (&Path, Option<&str>),
    overrides: Option<&Path>,
    record_bytes: usize,
    lab_apps: &analyte_config::LabApps,
) -> Result<ValidatedMachine, Error> {
    let replacement = overrides.map(read).transpose()?;
    let definition_text = read(definition)?;
    let saved;
    let site_text = match proposed {
        Some(text) => text,
        None => {
            saved = read(site)?;
            &saved
        }
    };
    let snapshot = analyte_config::load_machine(
        (&definition.display().to_string(), &definition_text),
        (&site.display().to_string(), site_text),
        replacement.as_deref().map(|text| ("overrides", text)),
        lab_apps,
    )?;
    // A record-only machine always captures: its recordings are its purpose (ADR 0034).
    let capture = capture || snapshot.effective().records_only();
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
    if capture && !plain_name(id.as_str()) {
        return Err(Error::InvalidCaptureMachineId);
    }
    Ok(ValidatedMachine {
        snapshot,
        model: model(definition)?,
        capture,
    })
}

/// The model of the definition file `definition`: the names of the two
/// folders it sits in (ADR 0033). Setup uses `<vendor>/<model>/machine.toml`
/// or that model's `record.toml`. If the folders cannot name a model, the
/// definition is refused.
fn model(definition: &Path) -> Result<analyte_contract::document::ModelId, Error> {
    // A name that is not UTF-8 reads as empty, which no model key accepts.
    fn name(path: Option<&Path>) -> &str {
        path.and_then(Path::file_name)
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("")
    }
    let folder = definition.parent();
    analyte_contract::document::ModelId::new(name(folder.and_then(Path::parent)), name(folder))
        .map_err(|source| Error::DefinitionOutsideModelFolder {
            definition: definition.to_owned(),
            source,
        })
}
