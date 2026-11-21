//! Runs `analyte doctor`: one read-only health check for IT, each line OK, WARN or FAIL with what to do, every check within its own time limit.
//!
//! It changes nothing. It asks the running engine what only the engine knows
//! (the Overview's verdict); it opens no serial port and never sends a byte
//! to a machine; a machine Analyte connects to gets one connection that is
//! closed at once, and only while the engine is stopped; a lab app gets one
//! GET with no keys or data. It prints no key, token or value, and reads the
//! keys only to say whether they are there. Opening an existing store may
//! upgrade an older layout, as every store command does (ADR 0025).
use super::chain;
use super::engine::{self, EngineError};
use crate::admin::Error;
use crate::admin::overview::answer::{Answer, Verdict};
use crate::admin::view::format;
use crate::installation::config::Program;
use crate::machine::link::status::Link;
use analyte_config::Endpoint;
use std::{future::Future, path::Path, process::ExitCode, time::Duration};

/// How long one check may take; a slower one fails with that reason.
const CHECK_TIME: Duration = Duration::from_secs(8);
/// How long a machine or lab app has to accept a connection.
const CONNECT_TIME: Duration = Duration::from_secs(3);
/// The name the Windows firewall rule for analyzers has when the installer
/// adds one; doctor only reads whether it exists.
#[cfg(windows)]
const FIREWALL_RULE: &str = "Analyte – analyzers on the lab network";

/// How a check came out; ordered from best to worst.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub(in crate::admin) enum Level {
    Ok,
    Warn,
    Fail,
}

impl Level {
    const fn word(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }

    /// The exit code: 0 all OK, 1 a warning, 2 a failure.
    pub(in crate::admin) const fn exit_code(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Warn => 1,
            Self::Fail => 2,
        }
    }
}

/// Whether the engine runs, as far as doctor can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Engine {
    /// It answered on its admin address.
    Running,
    /// Nothing answered on its admin address.
    Stopped,
    /// The admin page is off, so doctor cannot ask.
    Unknown,
}

/// One line of the check.
#[derive(Debug, serde::Serialize)]
pub(in crate::admin) struct Check {
    pub(in crate::admin) level: Level,
    /// What was checked, such as "Engine" or "Machine cobas-1".
    pub(in crate::admin) what: String,
    /// What was found.
    pub(in crate::admin) found: String,
    /// What to do, unless all is well.
    pub(in crate::admin) fix: Option<String>,
}

fn ok(what: impl Into<String>, found: impl Into<String>) -> Check {
    Check {
        level: Level::Ok,
        what: what.into(),
        found: found.into(),
        fix: None,
    }
}

fn warn(what: impl Into<String>, found: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        level: Level::Warn,
        what: what.into(),
        found: found.into(),
        fix: Some(fix.into()),
    }
}

fn fail(what: impl Into<String>, found: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        level: Level::Fail,
        what: what.into(),
        found: found.into(),
        fix: Some(fix.into()),
    }
}

/// Runs `check` within [`CHECK_TIME`]; past it, `what` fails with that reason.
async fn bounded(what: &str, check: impl Future<Output = Vec<Check>>) -> Vec<Check> {
    tokio::time::timeout(CHECK_TIME, check)
        .await
        .unwrap_or_else(|_| {
            vec![fail(
                what,
                format!(
                    "the check did not finish within {} seconds",
                    CHECK_TIME.as_secs()
                ),
                "run analyte doctor again; if it stays slow, run analyte logs",
            )]
        })
}

pub(super) async fn run(config: &Path, json: bool) -> Result<ExitCode, Error> {
    let checks = checks(config).await;
    let worst = worst(&checks);
    show(&checks, worst, json)?;
    Ok(ExitCode::from(worst.exit_code()))
}

/// The worst level among `checks`: what the exit code says.
pub(in crate::admin) fn worst(checks: &[Check]) -> Level {
    checks
        .iter()
        .map(|check| check.level)
        .max()
        .unwrap_or(Level::Ok)
}

/// Every check, in the order printed, for the program file at `config`.
pub(in crate::admin) async fn checks(config: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    match Program::load(config) {
        Ok(program) => {
            checks.push(ok(
                "Program file",
                format!("{} is valid", config.display()),
            ));
            checks.extend(installation(&program).await);
        }
        Err(error) => checks.push(fail(
            "Program file",
            format!("{} cannot be used: {}", config.display(), chain(&error)),
            "fix what it names (or choose the right file with --config), then run analyte doctor again; the other checks need a valid program file",
        )),
    }
    checks
}

/// Prints `checks` as text or JSON.
fn show(checks: &[Check], worst: Level, json: bool) -> Result<(), Error> {
    if json {
        #[derive(serde::Serialize)]
        struct Shown<'a> {
            result: Level,
            checks: &'a [Check],
        }
        super::json(&Shown {
            result: worst,
            checks,
        })
    } else {
        print(checks)
    }
}

fn print(checks: &[Check]) -> Result<(), Error> {
    for check in checks {
        super::say(&format!(
            "{:<5} {}: {}",
            check.level.word(),
            check.what,
            check.found
        ))?;
        if let Some(fix) = &check.fix {
            super::say(&format!("      What to do: {fix}"))?;
        }
    }
    let count = |level| checks.iter().filter(|check| check.level == level).count();
    let (failed, warned) = (count(Level::Fail), count(Level::Warn));
    super::say("")?;
    super::say(&if failed == 0 && warned == 0 {
        "Doctor: everything checked is OK.".to_owned()
    } else {
        format!(
            "Doctor: {} and {}.",
            format::amount(
                u64::try_from(failed).unwrap_or(u64::MAX),
                "failure",
                "failures"
            ),
            format::amount(
                u64::try_from(warned).unwrap_or(u64::MAX),
                "warning",
                "warnings"
            )
        )
    })
}

/// Every check that needs a valid program file.
async fn installation(program: &Program) -> Vec<Check> {
    let mut checks = Vec::new();
    let (engine_checks, running, answer) = engine(program).await;
    checks.extend(engine_checks);
    let (store_checks, lists) = tokio::time::timeout(CHECK_TIME, store(program))
        .await
        .unwrap_or_else(|_| {
            let late = format!(
                "the store did not open within {} seconds",
                CHECK_TIME.as_secs()
            );
            (
                vec![fail(
                    "Store",
                    late,
                    "run analyte doctor again; if it stays slow, run analyte logs",
                )],
                Vec::new(),
            )
        });
    checks.extend(store_checks);
    checks.push(disk(program));
    for machine in program.machines() {
        let what = format!("Machine {}", machine.id().as_str());
        let check = connection(&what, machine, running);
        checks.extend(bounded(&what, check).await);
    }
    for id in program.awaiting_setup() {
        checks.push(warn(
            format!("Machine {}", id.as_str()),
            "listed but not set up yet, so it does not run",
            "choose its model on the Setup page, save, then restart the engine",
        ));
    }
    checks.extend(bounded("Lab apps", lab_apps(program, answer.as_ref(), &lists)).await);
    #[cfg(windows)]
    {
        checks.push(users_group());
        checks.extend(bounded("Firewall", firewall(program)).await);
    }
    checks
}

/// Whether the engine answers, its version, and the Overview's verdict.
async fn engine(program: &Program) -> (Vec<Check>, Engine, Option<Answer>) {
    let version = match tokio::time::timeout(CHECK_TIME, engine::answering(program)).await {
        Ok(Ok(version)) => version,
        Ok(Err(EngineError::AdminOff)) => {
            return (
                vec![warn(
                    "Engine",
                    "the program file has no [admin] table, so doctor cannot ask the engine how it runs",
                    super::overview::advice(&EngineError::AdminOff),
                )],
                Engine::Unknown,
                None,
            );
        }
        Ok(Err(error)) => {
            return (
                vec![fail(
                    "Engine",
                    chain(&error),
                    super::overview::advice(&error),
                )],
                Engine::Stopped,
                None,
            );
        }
        Err(_) => {
            let error = EngineError::TimedOut;
            return (
                vec![fail(
                    "Engine",
                    chain(&error),
                    super::overview::advice(&error),
                )],
                Engine::Stopped,
                None,
            );
        }
    };
    let mut checks = vec![if version == env!("CARGO_PKG_VERSION") {
        ok("Engine", format!("running version {version} and answering"))
    } else {
        warn(
            "Engine",
            format!(
                "running version {version}, but this command is version {}",
                env!("CARGO_PKG_VERSION")
            ),
            "run the analyte.exe the service runs, or update both to the same version",
        )
    }];
    let answer = match tokio::time::timeout(CHECK_TIME, engine::verdict(program)).await {
        Ok(Ok(answer)) => answer,
        Ok(Err(error)) => {
            checks.push(warn(
                "Overview",
                chain(&error),
                super::overview::advice(&error),
            ));
            return (checks, Engine::Running, None);
        }
        Err(_) => {
            let error = EngineError::TimedOut;
            checks.push(warn(
                "Overview",
                chain(&error),
                super::overview::advice(&error),
            ));
            return (checks, Engine::Running, None);
        }
    };
    let things = answer
        .needs
        .iter()
        .map(|need| need.text.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    checks.push(match answer.verdict {
        Verdict::Good => ok("Overview", answer.title()),
        Verdict::Attention => warn(
            "Overview",
            format!("{}: {things}", answer.title()),
            "run analyte overview for where and how to fix each",
        ),
        Verdict::Problem => fail(
            "Overview",
            format!("{}: {things}", answer.title()),
            "run analyte overview for where and how to fix each",
        ),
        Verdict::Unknown => warn(
            "Overview",
            answer.sub(),
            "run analyte doctor again; if it stays unknown, run analyte logs",
        ),
    });
    (checks, Engine::Running, Some(answer))
}

/// Each lab app's order list health by name, as the store keeps it.
type OrderLists = Vec<(String, Result<analyte_store::Health, String>)>;

/// Whether the store opens, which also checks the data directory's access
/// list, and while it is open when each lab app's last complete order list
/// came. A store that does not exist yet is never created here.
async fn store(program: &Program) -> (Vec<Check>, OrderLists) {
    let file = program.data_directory().join("analyte.db");
    if !file.is_file() {
        return (
            vec![warn(
                "Store",
                format!("there is no store at {} yet", file.display()),
                "start the engine once (Start-Service Analyte); it creates the store",
            )],
            Vec::new(),
        );
    }
    match crate::open_system_store(program).await {
        Ok(mut store) => {
            let lists = order_lists(program, &store).await;
            let closed = match crate::deadline() {
                Ok(until) => store.shutdown(until).await.map_err(|error| chain(&error)),
                Err(error) => Err(chain(&error)),
            };
            let check = match closed {
                Ok(()) => ok("Store", format!("{} opens", file.display())),
                Err(error) => warn(
                    "Store",
                    format!("opened but did not close cleanly: {error}"),
                    "run analyte doctor again; if it repeats, run analyte logs",
                ),
            };
            (vec![check], lists)
        }
        Err(error) => (
            vec![fail(
                "Store",
                format!(
                    "{} cannot be opened: {}",
                    program.data_directory().display(),
                    chain(&error)
                ),
                "run this from an administrator PowerShell; if it names an access list, see docs/operations/install.md",
            )],
            Vec::new(),
        ),
    }
}

/// Free space against the space the program always keeps free.
fn disk(program: &Program) -> Check {
    let reserve = program.store_limits.reserve_bytes;
    match fs4::available_space(program.data_directory()) {
        Ok(free) if free > reserve => ok(
            "Disk",
            format!(
                "{} free; {} is always kept free",
                format::bytes(free),
                format::bytes(reserve)
            ),
        ),
        Ok(free) => fail(
            "Disk",
            format!(
                "{} free, at or below the {} kept free: Analyte refuses new data from machines",
                format::bytes(free),
                format::bytes(reserve)
            ),
            "free disk space on this drive; Analyte deletes nothing to make room",
        ),
        Err(error) => warn(
            "Disk",
            format!("the free space could not be read: {error}"),
            "check the data directory's drive in File Explorer",
        ),
    }
}

/// Whether `machine`'s port, address or folder can be used. While the engine
/// runs it holds them, so a held port is what is expected.
async fn connection(
    what: &str,
    machine: &crate::installation::config::Machine,
    running: Engine,
) -> Vec<Check> {
    let (link, endpoint) = crate::installation::config::connections::of(machine);
    vec![match (link, endpoint) {
        (Link::Serial, endpoint) => serial(what, endpoint).await,
        (Link::TcpServer, Endpoint::Fixed(address)) => listen(what, address, running).await,
        (Link::TcpClient, Endpoint::Fixed(address)) => connect(what, address, running).await,
        (Link::Folder, Endpoint::Fixed(folder)) => match std::fs::metadata(folder) {
            Ok(metadata) if metadata.is_dir() => ok(what, format!("folder {folder} is there")),
            Ok(_) => fail(
                what,
                format!("{folder} is not a folder"),
                "choose the machine's export folder on the Setup page",
            ),
            Err(error) => fail(
                what,
                format!("folder {folder} cannot be read: {error}"),
                "create the folder or share it again, or choose another on the Setup page",
            ),
        },
        // The program refuses a network or folder link given by USB serial number.
        (Link::TcpServer | Link::TcpClient | Link::Folder, Endpoint::UsbSerial(number)) => warn(
            what,
            format!(
                "a USB serial number ({}) names a network or folder link",
                number.as_str()
            ),
            "choose the machine's address or folder on the Setup page",
        ),
    }]
}

/// Whether the serial port is present; it is never opened, so nothing
/// reaches the machine and the engine keeps it.
async fn serial(what: &str, endpoint: &Endpoint) -> Check {
    let listed = match tokio::task::spawn_blocking(analyte_transport::serial::list).await {
        Ok(Ok(listed)) => listed,
        Ok(Err(error)) => {
            return warn(
                what,
                format!("the serial ports could not be listed: {error}"),
                "check the USB serial adapter's driver in Device Manager",
            );
        }
        Err(error) => {
            return warn(
                what,
                format!("the serial ports could not be listed: {error}"),
                "run analyte doctor again",
            );
        }
    };
    let plug = "plug in the USB serial adapter, or choose the right port on the Setup page";
    match endpoint {
        Endpoint::Fixed(name) => {
            if listed.iter().any(|port| {
                crate::installation::config::connections::same_port_name(&port.name, name)
            }) {
                ok(what, format!("serial port {name} is present"))
            } else {
                fail(what, format!("serial port {name} is not present"), plug)
            }
        }
        Endpoint::UsbSerial(number) => {
            let ports: Vec<_> = listed
                .iter()
                .filter(|port| {
                    port.usb_serial.as_deref().is_some_and(|reported| {
                        analyte_transport::serial::same_usb_serial(reported, number.as_str())
                    })
                })
                .map(|port| port.name.as_str())
                .collect();
            match ports.as_slice() {
                [port] => ok(
                    what,
                    format!(
                        "the adapter with serial number {} is at {port}",
                        number.as_str()
                    ),
                ),
                [] => fail(
                    what,
                    format!(
                        "no adapter with serial number {} is plugged in",
                        number.as_str()
                    ),
                    plug,
                ),
                several => fail(
                    what,
                    format!(
                        "{} adapters report serial number {}: {}",
                        several.len(),
                        number.as_str(),
                        several.join(", ")
                    ),
                    "unplug the adapter that does not belong to this machine",
                ),
            }
        }
    }
}

/// Whether Analyte can listen on `address`: free while the engine is
/// stopped, held while it runs. The test listener closes at once.
async fn listen(what: &str, address: &str, running: Engine) -> Check {
    match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => {
            drop(listener);
            match running {
                Engine::Running => warn(
                    what,
                    format!("nothing listens on {address} although the engine runs"),
                    "run analyte overview: the machine's service in Analyte may have stopped; restart the engine with Restart-Service Analyte",
                ),
                Engine::Stopped | Engine::Unknown => {
                    ok(what, format!("{address} is free for Analyte to listen on"))
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => match running {
            Engine::Running => ok(what, format!("{address} is held by the running engine")),
            Engine::Stopped => fail(
                what,
                format!("another program holds {address}"),
                "stop the other program, or choose another port on the Setup page and on the machine",
            ),
            Engine::Unknown => warn(
                what,
                format!("{address} is held, by the engine or by another program"),
                "turn the admin page on so doctor can tell, or look with Get-NetTCPConnection -LocalPort <port>",
            ),
        },
        Err(error) => fail(
            what,
            format!("{address} cannot be used on this computer: {error}"),
            "choose an address this computer has on the Setup page",
        ),
    }
}

/// Whether the machine Analyte connects to accepts a connection; only while
/// the engine is stopped, since a machine may take one connection at a time.
async fn connect(what: &str, address: &str, running: Engine) -> Check {
    match running {
        Engine::Running => {
            return ok(
                what,
                format!(
                    "the engine connects to {address}; analyte overview says if the link is off"
                ),
            );
        }
        Engine::Unknown => {
            return warn(
                what,
                format!(
                    "not tried: the engine may be connected to {address}, and a machine may take one connection at a time"
                ),
                "turn the admin page on so doctor can tell whether the engine runs",
            );
        }
        Engine::Stopped => {}
    }
    match tokio::time::timeout(CONNECT_TIME, tokio::net::TcpStream::connect(address)).await {
        // Closed at once: nothing is sent.
        Ok(Ok(stream)) => {
            drop(stream);
            ok(what, format!("{address} accepts a connection"))
        }
        Ok(Err(error)) => warn(
            what,
            format!("{address} does not accept a connection: {error}"),
            "check that the machine is on, its network settings and the cable",
        ),
        Err(_) => warn(
            what,
            format!(
                "{address} did not answer within {} seconds",
                CONNECT_TIME.as_secs()
            ),
            "check that the machine is on, its network settings and the cable",
        ),
    }
}

/// Each lab app: its name resolves, it answers over HTTPS with a valid
/// certificate, its keys are there, and when the last complete order list came.
async fn lab_apps(program: &Program, answer: Option<&Answer>, health: &OrderLists) -> Vec<Check> {
    let mut checks = Vec::new();
    // Read once for every lab app, and only while the engine cannot say.
    let keys = answer
        .is_none()
        .then(|| crate::installation::secrets::Environment::load(program));
    for settings in every_lab_app(program) {
        let name = settings.name.as_str();
        let what = format!("Lab app {name}");
        let url = settings.base_url.url();
        checks.push(reach(&what, url).await);
        checks.push(match (answer, &keys) {
            (Some(answer), _) => match answer.needs.iter().find(|need| {
                need.reason == "lab_app_blocked" && need.about.as_deref() == Some(name)
            }) {
                Some(need) => fail(
                    &what,
                    need.text.clone(),
                    "enter its keys on the Lab apps page, then restart the engine",
                ),
                None => ok(&what, "the running engine has its keys"),
            },
            (None, Some(Ok(keys))) => {
                if keys.has_keys(&settings.name) {
                    ok(&what, "its API key and signing key are there")
                } else {
                    fail(
                        &what,
                        "its API key or signing key is missing",
                        "enter its keys on the Lab apps page, then restart the engine",
                    )
                }
            }
            (None, Some(Err(error))) => warn(
                &what,
                format!(
                    "its keys could not be read from this account: {}",
                    chain(error)
                ),
                "start the engine and run analyte doctor again; the engine checks the keys itself",
            ),
            (None, None) => warn(
                &what,
                "its keys were not checked",
                "run analyte doctor again",
            ),
        });
        // Only a set-up machine's lab app is asked for orders; without an open
        // store the store check already says why nothing was read.
        let polled = program
            .machines()
            .iter()
            .any(|machine| machine.sends_to(name));
        if polled && !health.is_empty() {
            checks.push(match health.iter().find(|(id, _)| id == name) {
                Some((_, Ok(health))) => order_list(&what, health),
                Some((_, Err(error))) => warn(
                    &what,
                    format!("when its order list last came could not be read: {error}"),
                    "run analyte doctor again",
                ),
                None => warn(&what, "no order list is kept for it", "restart the engine"),
            });
        }
    }
    checks
}

/// Every lab app, once each by name: those on the Lab apps page, then any a
/// machine's own site file still names.
fn every_lab_app(program: &Program) -> Vec<&analyte_config::Destination> {
    let mut all: Vec<&analyte_config::Destination> = program
        .lab_apps()
        .iter()
        .map(crate::installation::config::LabApp::settings)
        .collect();
    for machine in program.machines() {
        let own = &machine.snapshot().effective().site().destination;
        if !all.iter().any(|known| known.name == own.name) {
            all.push(own);
        }
    }
    all
}

/// Whether the lab app's name resolves and it answers over HTTPS.
async fn reach(what: &str, url: &reqwest::Url) -> Check {
    let Some(host) = url.host_str() else {
        return fail(
            what,
            "its address has no host name",
            "fix its address on the Lab apps page",
        );
    };
    let port = url.port_or_known_default().unwrap_or(443);
    let resolved = tokio::time::timeout(CONNECT_TIME, tokio::net::lookup_host((host, port)))
        .await
        .map(|found| found.map(|mut addresses| addresses.next().is_some()));
    match resolved {
        Ok(Ok(true)) => {}
        Ok(Ok(false)) => {
            return fail(
                what,
                format!("{host} resolves to no address"),
                "check the lab app's address on the Lab apps page and this computer's DNS",
            );
        }
        Ok(Err(error)) => {
            return fail(
                what,
                format!("{host} does not resolve: {error}"),
                "check the lab app's address on the Lab apps page and this computer's DNS",
            );
        }
        Err(_) => {
            return fail(
                what,
                format!(
                    "{host} did not resolve within {} seconds",
                    CONNECT_TIME.as_secs()
                ),
                "check this computer's DNS and network",
            );
        }
    }
    match analyte_sink::answers(url, CONNECT_TIME).await {
        Ok(status) => ok(
            what,
            format!("{url} answers over HTTPS with a valid certificate (HTTP {status})"),
        ),
        Err(error) => fail(
            what,
            format!("{url} gives no HTTPS answer: {}", chain(&error)),
            "check the address, the lab app's server and its certificate; a certificate this computer does not trust must be fixed on the server",
        ),
    }
}

/// When each lab app's last complete order list came, read from the open store.
async fn order_lists(program: &Program, store: &analyte_store::Handle) -> OrderLists {
    let destinations = match crate::runtime::delivery::destinations(
        program,
        crate::runtime::hooks::ServiceMode::Production,
    ) {
        Ok(destinations) => destinations,
        Err(error) => {
            let why = chain(&error);
            return program
                .machines()
                .iter()
                .map(|machine| {
                    let name = &machine.snapshot().effective().site().destination.name;
                    (name.as_str().to_owned(), Err(why.clone()))
                })
                .collect();
        }
    };
    let now = jiff::Timestamp::now().as_second();
    let mut read = Vec::new();
    for destination in &destinations {
        let health = destination
            .health(store, now)
            .await
            .map_err(|error| chain(&error));
        read.push((destination.id().as_str().to_owned(), health));
    }
    read
}

/// The order list line, by the store's own trust rule (no threshold of its own).
fn order_list(what: &str, health: &analyte_store::Health) -> Check {
    let now = jiff::Timestamp::now().as_second();
    let when = health.last_successful_poll.map_or_else(
        || "never".to_owned(),
        |at| {
            format!(
                "{} ago",
                format::duration(u64::try_from(now.saturating_sub(at)).unwrap_or(0))
            )
        },
    );
    if health.cache_trusted {
        ok(what, format!("last complete order list {when}"))
    } else {
        warn(
            what,
            format!("its order list is not up to date (last complete list: {when})"),
            "check that the engine runs and the lab app answers; machines asking for orders get no answer meanwhile",
        )
    }
}

/// Whether the local "Analyte users" group exists, so members can sign in.
#[cfg(windows)]
fn users_group() -> Check {
    use crate::admin::server::windows::{Found, GROUP};
    let what = "Windows sign-in";
    let create = format!(
        "create it with New-LocalGroup -Name \"{GROUP}\", then add people with Add-LocalGroupMember -Group \"{GROUP}\" -Member <user>"
    );
    match crate::admin::server::windows::users_group() {
        Found::Group(_) => ok(what, format!("the {GROUP} group exists")),
        Found::Missing => warn(
            what,
            format!(
                "the {GROUP} group is missing, so only administrators can sign in with Windows"
            ),
            create,
        ),
        Found::NotAGroup => warn(
            what,
            format!("{GROUP} is an account but not a local group"),
            create,
        ),
        Found::NoComputerName => warn(
            what,
            "COMPUTERNAME is not set, so the group cannot be looked up",
            "run doctor from a normal PowerShell window",
        ),
        Found::Failed(error) => warn(
            what,
            format!(
                "the {GROUP} group could not be looked up: {}",
                chain(&error)
            ),
            "run analyte doctor again",
        ),
    }
}

/// Whether the installer's firewall rule lets analyzers connect, when a
/// machine connects to Analyte; read with `netsh`, which changes nothing.
#[cfg(windows)]
async fn firewall(program: &Program) -> Vec<Check> {
    let ports: Vec<String> = program
        .machines()
        .iter()
        .filter_map(
            |machine| match crate::installation::config::connections::of(machine) {
                // A loopback-only listener is never reached from the lab network.
                (Link::TcpServer, Endpoint::Fixed(address))
                    if !address
                        .parse::<std::net::SocketAddr>()
                        .is_ok_and(|parsed| parsed.ip().is_loopback()) =>
                {
                    Some(address.clone())
                }
                (Link::TcpServer | Link::TcpClient | Link::Serial | Link::Folder, _) => None,
            },
        )
        .collect();
    if ports.is_empty() {
        return Vec::new();
    }
    let what = "Firewall";
    let shown = tokio::process::Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule"])
        .arg(format!("name={FIREWALL_RULE}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status();
    vec![match tokio::time::timeout(CONNECT_TIME, shown).await {
        Ok(Ok(status)) if status.success() => {
            ok(what, format!("the rule \"{FIREWALL_RULE}\" is there"))
        }
        Ok(Ok(_)) => warn(
            what,
            format!(
                "no rule named \"{FIREWALL_RULE}\" lets analyzers connect to {}",
                ports.join(", ")
            ),
            "run the installer again, or allow inbound TCP to these ports from the lab network in Windows Defender Firewall",
        ),
        Ok(Err(error)) => warn(
            what,
            format!("the firewall rules could not be read: {error}"),
            "check the rule in Windows Defender Firewall",
        ),
        Err(_) => warn(
            what,
            "reading the firewall rules took too long",
            "check the rule in Windows Defender Firewall",
        ),
    }]
}
