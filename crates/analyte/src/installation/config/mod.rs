//! Loads bounded program TOML and machine snapshots without secret values.
#[cfg(test)]
use analyte_fixtures::ConfigEdit as _;
mod alerts;
pub(crate) mod connections;
mod hooks;
pub(crate) use crate::machine::hooks::MachineHooks;
pub(crate) use hooks::ProgramHooks;
mod error;
pub(crate) mod lab_apps;
mod webhooks;
pub(crate) use alerts::Condition;
pub use alerts::{Alert, AlertConfigError, AlertKind};
use analyte_config::{Draft, Validated};
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
        self.snapshot.effective().site().destination.name.as_str() == name
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

#[cfg(test)]
mod admin_tests {
    use super::*;
    use crate::Error;

    // Plan §11
    #[test]
    #[cfg(feature = "test-hooks")]
    fn loopback_poll_override_keeps_production_defaults() -> Result<(), Error> {
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
    // Plan §11
    #[test]
    fn admin_refuses_non_loopback_configuration() -> Result<(), Error> {
        for address in ["0.0.0.0", "192.0.2.1", "127.0.0.2"] {
            let admin: Admin = toml::from_str(&format!("port = 8080\naddress = '{address}'"))?;
            assert!(admin.address().is_err());
        }
        assert!(toml::from_str::<Admin>("port = 8080\naddress = '::'").is_err());
        Ok(())
    }

    // Plan section 11: the optional port selects only 127.0.0.1, never a wildcard bind.
    // Plan §11
    #[test]
    fn admin_port_defaults_to_ipv4_loopback() -> Result<(), Error> {
        let admin: Admin = toml::from_str("port = 8080")?;
        assert_eq!(admin.address()?.to_string(), "127.0.0.1:8080");
        assert!(toml::from_str::<Admin>("port = 65536").is_err());
        assert!(toml::from_str::<Admin>("port = 0")?.address().is_err());
        Ok(())
    }

    // Owner request 2026-11-18 (installers): a machine listed without a
    // definition waits for the setup page and does not run. It needs the admin
    // page, cannot carry capture or overrides, and keeps identities unique.
    #[test]
    fn machine_without_definition_waits_for_setup() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("program.toml");
        let program = |machines: &str| {
            analyte_fixtures::program_with_limits(
                machines,
                &analyte_store::Limits {
                    reserve_bytes: 1,
                    database_bytes: 100_000_000,
                    record_bytes: 2_097_152,
                    page_rows: 100,
                    busy_timeout: Duration::from_millis(100),
                },
            )
        };
        let waiting = "[[machines]]\nid = 'machine-1'\nsite = 'machine-1.site.toml'\n";
        std::fs::write(&path, program(&format!("[admin]\nport = 8080\n{waiting}")))?;
        let loaded = Program::load(&path)?;
        assert!(loaded.machines().is_empty());
        let ids: Vec<_> = loaded
            .awaiting_setup()
            .iter()
            .map(analyte_model::Identifier::as_str)
            .collect();
        assert_eq!(ids, ["machine-1"]);
        std::fs::write(&path, program(waiting))?;
        assert!(matches!(
            Program::load(&path),
            Err(super::Error::SetupNeedsAdmin)
        ));
        for extra in ["capture = true\n", "overrides = 'o.toml'\n"] {
            std::fs::write(
                &path,
                program(&format!("[admin]\nport = 8080\n{waiting}{extra}")),
            )?;
            assert!(matches!(
                Program::load(&path),
                Err(super::Error::AwaitingSetupWithSettings)
            ));
        }
        std::fs::write(
            &path,
            program(&format!("[admin]\nport = 8080\n{waiting}{waiting}")),
        )?;
        assert!(matches!(
            Program::load(&path),
            Err(super::Error::DuplicateMachineIdentity)
        ));
        Ok(())
    }

    // Owner request 2026-11-19 (results waiting after a removal; lab app set once,
    // item 3); safety rule 2: a kept destination is read as a lab app with the site
    // file's [destination] rules, once per name, may carry a signing rotation, and
    // gets a delivery-only worker only while no set-up machine uses its name.
    #[test]
    fn retired_destinations_load_for_delivery_only() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("program.toml");
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/config/examples");
        let machine = format!(
            "[[machines]]\nid = 'synthetic'\ndefinition = '{}'\nsite = '{}'\n",
            examples.join("hl7.toml").display(),
            examples.join("site.toml").display()
        );
        let program = |machines: &str, tail: &str| {
            let head = analyte_fixtures::program(&format!("[admin]\nport = 8080\n{machines}"));
            format!("{head}{tail}")
        };
        let kept = |name: &str, extra: &str| {
            format!(
                concat!(
                    "[[retired_destinations]]\nname = '{}'\nbase_url = 'https://old.example.invalid'\n",
                    "api_key_name = 'old-api'\nsigning_key_name = 'old-signing'\n{}"
                ),
                name, extra
            )
        };
        let rotation = "[signing_rotation]\nold = ['old-previous']\n";
        // The machine's "demo" kept with the very same settings is allowed.
        let same = concat!(
            "[[retired_destinations]]\nname = 'demo'\nbase_url = 'https://lab.example.invalid'\n",
            "api_key_name = 'demo-api'\nsigning_key_name = 'demo-signing'\n"
        );
        let tail = format!("{}{same}{rotation}", kept("old", ""));
        std::fs::write(&path, program(&machine, &tail))?;
        let loaded = Program::load(&path)?;
        let names = |kept: &mut dyn Iterator<Item = &LabApp>| -> Vec<String> {
            kept.map(|kept| kept.settings().name.as_str().to_owned())
                .collect()
        };
        let all = names(&mut loaded.lab_apps().iter());
        assert_eq!(all, ["old", "demo"]);
        // The running machine serves "demo"; only "old" is drained.
        assert_eq!(names(&mut loaded.unused_lab_apps()), ["old"]);
        let timeout = loaded.lab_apps()[0].limits.request_timeout;
        assert_eq!(timeout, Duration::from_secs(10));
        let twice = format!("{}{}", kept("old", ""), kept("old", ""));
        std::fs::write(&path, program("", &twice))?;
        assert!(matches!(
            Program::load(&path),
            Err(super::Error::DuplicateRetiredDestination(name)) if name == "old"
        ));
        let zero = kept("old", "request_timeout_seconds = 0\n");
        std::fs::write(&path, program("", &zero))?;
        let refused = Program::load(&path).err().ok_or("timeout accepted")?;
        let key = "retired_destinations.request_timeout_seconds";
        assert!(refused.to_string().contains(key), "{refused}");
        // A machine-only key, an unknown key and plain HTTP are refused.
        for invalid in [
            kept("old", "endpoint = '127.0.0.1:1'\n"),
            kept("old", "base_url2 = 'x'\n"),
            kept("old", "").replace("https://", "http://"),
        ] {
            std::fs::write(&path, program("", &invalid))?;
            assert!(Program::load(&path).is_err(), "{invalid}");
        }
        Ok(())
    }

    // Owner request 2026-11-19 (lab app set once), items 1 to 3 and 6: a site file
    // names a lab app from lab-apps.toml beside the program file and runs with the
    // same snapshot digest as the old embedded form; a missing file lists none; one
    // name written differently in two places (lab-apps.toml, a site file or a kept
    // destination) is refused, naming both places, and never guessed.
    #[test]
    fn lab_apps_file_is_read_and_conflicts_refused() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("program.toml");
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/config/examples");
        let site = include_str!("../../../../core/config/examples/site.toml");
        let lab_apps = include_str!("../../../../core/config/examples/lab-apps.toml");
        let at = site.find("[destination]").ok_or("example site")?;
        let named = format!("{}lab_app = \"demo\"\n", &site[..at]);
        let machine = format!(
            "[[machines]]\nid = 'synthetic'\ndefinition = '{}'\nsite = 'site.toml'\n",
            examples
                .join("hl7.toml")
                .display()
                .to_string()
                .replace('\\', "/"),
        );
        let program = analyte_fixtures::program(&format!("[admin]\nport = 8080\n{machine}"));
        std::fs::write(&path, &program)?;
        let file = temp.path().join(analyte_config::LAB_APPS_FILE);
        let digest = |path: &Path| -> Result<[u8; 32], Error> {
            Ok(Program::load(path)?.machines()[0].snapshot().digest())
        };

        // Old setup: no lab-apps.toml, the site file writes its own [destination].
        std::fs::write(temp.path().join("site.toml"), site)?;
        let old = digest(&path)?;
        assert!(Program::load(&path)?.lab_apps().is_empty());
        // New setup: the lab app is listed once and the site file names it.
        std::fs::write(&file, lab_apps)?;
        std::fs::write(temp.path().join("site.toml"), &named)?;
        assert_eq!(digest(&path)?, old, "the effective config is unchanged");
        let loaded = Program::load(&path)?;
        assert_eq!(loaded.lab_apps().len(), 1);
        assert_eq!(loaded.unused_lab_apps().count(), 0);

        // The same name written differently in lab-apps.toml and a site file.
        std::fs::write(
            &file,
            lab_apps.replace("lab.example.invalid", "other.example.invalid"),
        )?;
        std::fs::write(temp.path().join("site.toml"), site)?;
        let refused = Program::load(&path).err().ok_or("conflict accepted")?;
        let text = refused.to_string();
        assert!(
            matches!(
                &refused,
                super::Error::ConflictingLabApp { name, .. } if name == "demo"
            ),
            "{text}"
        );
        assert!(text.contains(&file.display().to_string()), "{text}");
        assert!(text.contains("site.toml [destination]"), "{text}");
        // ...and in lab-apps.toml and a kept destination in the program file.
        std::fs::write(&file, lab_apps)?;
        let kept = concat!(
            "[[retired_destinations]]\nname = 'demo'\nbase_url = 'https://moved.example.invalid'\n",
            "api_key_name = 'demo-api'\nsigning_key_name = 'demo-signing'\n"
        );
        std::fs::write(&path, format!("{program}{kept}"))?;
        let text = Program::load(&path)
            .err()
            .ok_or("conflict accepted")?
            .to_string();
        assert!(
            text.contains("program.toml [[retired_destinations]]"),
            "{text}"
        );
        assert!(text.contains(&file.display().to_string()), "{text}");
        // A kept destination identical to the listed one is the same lab app.
        std::fs::write(
            &path,
            format!(
                "{program}{}",
                kept.replace("moved.example.invalid", "lab.example.invalid")
            ),
        )?;
        assert_eq!(Program::load(&path)?.lab_apps().len(), 1);
        Ok(())
    }

    // Owner decision 2026-11-19 (key clash): two lab app names that differ only by
    // letter case or - versus _ are refused at startup wherever they are written
    // (lab-apps.toml, a site file's [destination], a kept destination), and so are
    // two different key names, rotation included, that share one variable. The
    // message names both, the variable and the fix.
    #[test]
    fn clashing_lab_app_and_key_names_are_refused_at_startup() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("program.toml");
        let file = temp.path().join(analyte_config::LAB_APPS_FILE);
        let app = |name: &str, api: &str, signing: &str| {
            format!(
                "name = '{name}'\nbase_url = 'https://lab.example.invalid'\napi_key_name = '{api}'\nsigning_key_name = '{signing}'\n"
            )
        };
        let listed = |body: &str| format!("schema_version = 1\n[[lab_apps]]\n{body}");
        let program = |tail: &str| {
            format!(
                "{}{tail}",
                analyte_fixtures::program("[admin]\nport = 8080\n")
            )
        };
        let refused = |what: Clashing, first: &str, second: &str| {
            let error = Program::load(&path).err().ok_or("clash accepted")?;
            let named = matches!(
                &error,
                super::Error::NameClash { what: found, first: one, second: two, .. }
                    if *found == what && one == first && two == second
            );
            if named {
                Ok(error.to_string())
            } else {
                Err(Error::from(format!("unexpected: {error}")))
            }
        };

        // lab-apps.toml and a kept destination in the program file.
        std::fs::write(&file, listed(&app("lab-1", "lab-1-api", "lab-1-signing")))?;
        let kept = format!(
            "[[retired_destinations]]\n{}",
            app("lab_1", "lab_1-api", "lab_1-signing")
        );
        std::fs::write(&path, program(&kept))?;
        assert_eq!(
            refused(Clashing::LabApps, "lab-1", "lab_1")?,
            concat!(
                "Lab apps \"lab-1\" and \"lab_1\" clash: both would use the key name ",
                "ANALYTE_SECRET_LAB_1_API. Rename one so the names differ by more than letter ",
                "case or - versus _, in the file that defines it (lab-apps.toml, a site file ",
                "or the program file)"
            )
        );

        // lab-apps.toml and a machine's own [destination] differing by case.
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/config/examples");
        let site = include_str!("../../../../core/config/examples/site.toml");
        std::fs::write(
            temp.path().join("site.toml"),
            site.replace("name = \"demo\"", "name = \"DEMO\"")
                .replace("demo-", "DEMO-"),
        )?;
        std::fs::write(&file, listed(&app("demo", "demo-api", "demo-signing")))?;
        let machine = format!(
            "[[machines]]\nid = 'synthetic'\ndefinition = '{}'\nsite = 'site.toml'\n",
            examples
                .join("hl7.toml")
                .display()
                .to_string()
                .replace('\\', "/"),
        );
        std::fs::write(
            &path,
            analyte_fixtures::program(&format!("[admin]\nport = 8080\n{machine}")),
        )?;
        refused(Clashing::LabApps, "demo", "DEMO")?;

        // Two lab apps whose key names share one variable.
        std::fs::write(&file, listed(&app("first", "shared-key", "first-signing")))?;
        let kept = format!(
            "[[retired_destinations]]\n{}",
            app("second", "shared_key", "second-signing")
        );
        std::fs::write(&path, program(&kept))?;
        let text = refused(Clashing::Keys, "shared-key", "shared_key")?;
        assert!(text.contains("ANALYTE_SECRET_SHARED_KEY"), "{text}");

        // A rotation key name against the lab app's own signing key name.
        std::fs::write(
            &path,
            program("[signing_rotation]\nfirst = ['first_signing']\n"),
        )?;
        refused(Clashing::Keys, "first-signing", "first_signing")?;
        // The same names written alike are no clash.
        std::fs::write(
            &path,
            program("[signing_rotation]\nfirst = ['first-previous']\n"),
        )?;
        assert_eq!(Program::load(&path)?.lab_apps().len(), 1);
        Ok(())
    }

    // Owner request 2026-11-18 (remove machines): removing the last machine leaves
    // a program with no machines; it still serves the setup page, so a machine can
    // be added again. Without the admin page it could do nothing, so it is refused.
    #[test]
    fn no_machines_is_allowed_only_with_the_setup_page() -> Result<(), Error> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("program.toml");
        let program = |machines: &str| {
            analyte_fixtures::program_with_limits(
                machines,
                &analyte_store::Limits {
                    reserve_bytes: 1,
                    database_bytes: 100_000_000,
                    record_bytes: 2_097_152,
                    page_rows: 100,
                    busy_timeout: Duration::from_millis(100),
                },
            )
        };
        for machines in [
            "machines = []\n[admin]\nport = 8080\n",
            "[admin]\nport = 8080\n",
        ] {
            std::fs::write(&path, program(machines))?;
            let loaded = Program::load(&path)?;
            assert!(loaded.machines().is_empty() && loaded.awaiting_setup().is_empty());
        }
        for machines in ["machines = []\n", ""] {
            std::fs::write(&path, program(machines))?;
            assert!(matches!(
                Program::load(&path),
                Err(super::Error::NoMachines)
            ));
        }
        Ok(())
    }

    // Plan section 11: existing installations have no admin listener by default.
    // Plan §11
    #[test]
    fn admin_is_absent_by_default() -> Result<(), Error> {
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
    // Plan §11
    #[test]
    fn alert_policies_name_installed_machines_uniquely() -> Result<(), Error> {
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
                    analyte_config::LabAppSource::LabApps => None,
                    analyte_config::LabAppSource::Site => Some(files.site),
                    analyte_config::LabAppSource::Overrides => files.overrides,
                };
                if let Some(place) = place {
                    places.add(
                        &effective.site().destination,
                        format!("{} [destination]", place.display()),
                    )?;
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

/// The files one machine entry names, resolved against the program folder.
struct Files {
    definition: PathBuf,
    site: PathBuf,
    overrides: Option<PathBuf>,
}

fn load_machine(entry: Entry, files: &Files, loading: &Loading<'_>) -> Result<Machine, Error> {
    let record_bytes = loading.record_bytes;
    let replacement = match &files.overrides {
        Some(p) => read(p)?,
        None => "schema_version = 1".into(),
    };
    let snapshot = Draft::load_with(
        (
            &files.definition.display().to_string(),
            &read(&files.definition)?,
        ),
        (&files.site.display().to_string(), &read(&files.site)?),
        ("overrides", &replacement),
        loading.lab_apps,
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
    if entry.capture && !plain_name(entry.id.as_str()) {
        return Err(Error::InvalidCaptureMachineId);
    }
    Ok(Machine {
        capture: entry.capture,

        hooks: MachineHooks::default(),
        destination_limits: crate::runtime::delivery::limits(
            &snapshot.effective().site().destination,
        ),
        id: entry.id,
        status: crate::machine::link::status::Reporter::new(
            crate::machine::link::status::Link::of(snapshot.effective().stack()),
        ),
        snapshot,
        listener: None,
        definition: files.definition.clone(),
    })
}
