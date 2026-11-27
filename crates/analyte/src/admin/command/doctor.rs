//! Runs the read-only `analyte doctor` health checks and prints OK, WARN or FAIL with what to do.
//!
//! It changes nothing. It asks the running engine what only the engine knows
//! (the Overview's verdict); it opens no serial port and never sends a byte
//! to a machine; a machine Analyte connects to gets one connection that is
//! closed at once, and only while the engine is stopped; a lab app gets one
//! GET with no keys or data. It prints no key, token or value, and reads the
//! keys only to say whether they are there. Opening an existing store may
//! upgrade an older layout, as every store command does (ADR 0027).
use super::chain;
use crate::admin::Error;
use crate::admin::engine::{self, EngineError};
use crate::admin::overview::answer::{Answer, Verdict};
use crate::admin::view::format;
use crate::installation::config::Program;
use crate::installation::credentials::Locked;
use crate::installation::secrets::{self, SecretError};
use crate::machine::link::status::Link;
use analyte_config::Endpoint;
use analyte_i18n::{count as tr_count, message as tr_args, text as tr};
use std::{future::Future, path::Path, process::ExitCode, time::Duration};

/// How long doctor waits on an asynchronous check before reporting a timeout.
const CHECK_TIME: Duration = Duration::from_secs(8);
/// How long reading the firewall rule may take: PowerShell starts slowly.
#[cfg(windows)]
const FIREWALL_TIME: Duration = Duration::from_secs(7);
/// How long a machine or lab app has to accept a connection.
const CONNECT_TIME: Duration = Duration::from_secs(3);
#[cfg(windows)]
use crate::admin::setup::{FIREWALL_RULE, FIREWALL_RULE_NAME};

#[cfg(test)]
mod tests {
    use super::{CHECK_TIME, Level, bounded, folder};
    use crate::Error;

    #[tokio::test(start_paused = true)]
    async fn doctor_reports_a_folder_probe_that_outlives_its_deadline() -> Result<(), Error> {
        let (started, running) = tokio::sync::oneshot::channel();
        let (release, hold) = tokio::sync::oneshot::channel::<()>();
        let executor = std::thread::current().id();
        let probe = move |_: &str| {
            // If the check runs inline, return immediately so the test fails
            // without locking up the same thread that drives the timer.
            if std::thread::current().id() != executor {
                let _ = started.send(());
                let _ = hold.blocking_recv();
            }
            Ok(true)
        };
        let what = "Machine synthetic";
        let mut check = std::pin::pin!(bounded(what, async {
            vec![folder(what, "synthetic", probe).await]
        }));
        tokio::select! {
            biased;
            checks = &mut check => return Err(format!("the check finished early: {checks:?}").into()),
            result = running => result?,
        }
        let guard = tokio::time::sleep(CHECK_TIME);
        tokio::time::advance(CHECK_TIME).await;
        let checks = tokio::select! {
            biased;
            checks = &mut check => checks,
            () = guard => return Err("the folder probe exceeded the check's deadline".into()),
        };
        drop(release);

        let [check] = checks.as_slice() else {
            return Err("expected one folder check".into());
        };
        assert_eq!(check.what, what);
        assert_eq!(check.level, Level::Fail);
        assert_eq!(check.found, "the check did not finish within 8 seconds");
        Ok(())
    }
}

/// How a check came out; ordered from best to worst.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub(in crate::admin) enum Level {
    Ok,
    Warn,
    Fail,
}

impl Level {
    fn word(self) -> &'static str {
        match self {
            Self::Ok => tr("doctor-overview-ok"),
            Self::Warn => tr("doctor-overview-warn"),
            Self::Fail => tr("doctor-overview-fail"),
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

/// Reports a timeout for `what` if `check` is still pending at [`CHECK_TIME`].
/// The check must yield while waiting for I/O so the timer can run.
async fn bounded(what: &str, check: impl Future<Output = Vec<Check>>) -> Vec<Check> {
    tokio::time::timeout(CHECK_TIME, check)
        .await
        .unwrap_or_else(|_| {
            vec![fail(
                what,
                tr_args(
                    "doctor-overview-the-check-did-not-finish-within-seconds",
                    [("as-secs", (CHECK_TIME.as_secs()).to_string().into())],
                ),
                tr("doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run"),
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
                tr("admin-program-file"),
                tr_args(
                    "doctor-overview-is-valid",
                    [("config", (config.display()).to_string().into())],
                ),
            ));
            checks.extend(installation(&program).await);
        }
        Err(error) => checks.push(fail(
            tr("admin-program-file"),
            tr_args(
                "doctor-overview-cannot-be-used",
                [
                    ("config", (config.display()).to_string().into()),
                    ("error", (chain(&error)).to_string().into()),
                ],
            ),
            tr("doctor-overview-fix-what-it-names-or-choose-the-right-file"),
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
            super::say(&tr_args(
                "doctor-overview-what-to-do-2",
                [("fix", (fix).to_string().into())],
            ))?;
        }
    }
    let count = |level| checks.iter().filter(|check| check.level == level).count();
    let (failed, warned) = (count(Level::Fail), count(Level::Warn));
    super::say("")?;
    super::say(&if failed == 0 && warned == 0 {
        tr("doctor-overview-doctor-everything-checked-is-ok").to_owned()
    } else {
        tr_args(
            "doctor-overview-doctor-and",
            [
                (
                    "failures",
                    (tr_count(
                        "admin-count-failures",
                        u64::try_from(failed).unwrap_or(u64::MAX),
                    ))
                    .to_string()
                    .into(),
                ),
                (
                    "warnings",
                    (tr_count(
                        "admin-count-warnings",
                        u64::try_from(warned).unwrap_or(u64::MAX),
                    ))
                    .to_string()
                    .into(),
                ),
            ],
        )
    })
}

/// Every check that needs a valid program file.
async fn installation(program: &Program) -> Vec<Check> {
    let mut checks = Vec::new();
    let (engine_checks, running, answer) = engine(program).await;
    checks.extend(engine_checks);
    let (store_checks, lists) = tokio::time::timeout(CHECK_TIME, store(program, answer.is_none()))
        .await
        .unwrap_or_else(|_| {
            let late = tr_args(
                "doctor-overview-the-store-did-not-open-within-seconds",
                [("as-secs", (CHECK_TIME.as_secs()).to_string().into())],
            );
            (
                vec![fail(
                    tr("doctor-overview-store"),
                    late,
                    tr("doctor-overview-run-analyte-doctor-again-if-it-stays-slow-run"),
                )],
                Vec::new(),
            )
        });
    checks.extend(store_checks);
    checks.push(disk(program));
    for machine in program.machines() {
        let what = tr_args(
            "doctor-overview-machine",
            [("id", machine.id().as_str().into())],
        );
        let check = connection(&what, machine, running);
        checks.extend(bounded(&what, check).await);
    }
    for id in program.awaiting_setup() {
        checks.push(warn(
            tr_args("doctor-overview-machine", [("id", id.as_str().into())]),
            tr("doctor-overview-listed-but-not-set-up-yet-so-it-does"),
            tr("doctor-overview-choose-its-model-on-the-setup-page-save-then"),
        ));
    }
    checks.extend(
        bounded(
            tr("admin-lab-apps-2"),
            lab_apps(program, answer.as_ref(), &lists),
        )
        .await,
    );
    #[cfg(windows)]
    {
        checks.push(users_group());
        checks.extend(bounded("Firewall", firewall(program)).await);
    }
    checks
}

/// Whether the engine answers, its version, and the Overview's verdict.
async fn engine(program: &Program) -> (Vec<Check>, Engine, Option<Answer>) {
    let stop = |level: Level, state: Engine, error: &EngineError| {
        let check = Check {
            level,
            what: tr("admin-engine").into(),
            found: chain(error),
            fix: Some(super::overview::advice(error).into()),
        };
        (vec![check], state, None)
    };
    // The key goes only to a listener that proves it holds it (ADR 0025).
    let proven = match tokio::time::timeout(CHECK_TIME, engine::Engine::proven(program)).await {
        Ok(Ok(proven)) => proven,
        Ok(Err(error @ EngineError::AdminOff)) => {
            return stop(Level::Warn, Engine::Unknown, &error);
        }
        // Something holds the admin address but is not this engine, or the
        // key cannot be read to check: whether the engine runs is unknown.
        Ok(Err(
            error @ (EngineError::Unproven | EngineError::Key(_) | EngineError::KeyThread(_)),
        )) => {
            return stop(Level::Fail, Engine::Unknown, &error);
        }
        Ok(Err(error)) => return stop(Level::Fail, Engine::Stopped, &error),
        Err(_) => return stop(Level::Fail, Engine::Stopped, &EngineError::TimedOut),
    };
    let version = match tokio::time::timeout(CHECK_TIME, proven.version()).await {
        Ok(Ok(version)) => version,
        Ok(Err(error)) => return stop(Level::Fail, Engine::Running, &error),
        Err(_) => return stop(Level::Fail, Engine::Running, &EngineError::TimedOut),
    };
    let mut checks = vec![if version == env!("CARGO_PKG_VERSION") {
        ok(
            tr("admin-engine"),
            tr_args(
                "doctor-overview-running-version-and-answering",
                [("version", (version).to_string().into())],
            ),
        )
    } else {
        warn(
            tr("admin-engine"),
            tr_args(
                "doctor-overview-running-version-but-this-command-is-version",
                [
                    ("version", (version).to_string().into()),
                    (
                        "CARGO-PKG-VERSION",
                        (env!("CARGO_PKG_VERSION")).to_string().into(),
                    ),
                ],
            ),
            tr("doctor-overview-run-the-analyte-exe-the-service-runs-or-update"),
        )
    }];
    let answer = match tokio::time::timeout(CHECK_TIME, proven.verdict()).await {
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
            tr("doctor-overview-run-analyte-overview-for-where-and-how-to-fix"),
        ),
        Verdict::Problem => fail(
            "Overview",
            format!("{}: {things}", answer.title()),
            tr("doctor-overview-run-analyte-overview-for-where-and-how-to-fix"),
        ),
        Verdict::Unknown => warn(
            "Overview",
            answer.sub(),
            tr("doctor-overview-run-analyte-doctor-again-if-it-stays-unknown-run"),
        ),
    });
    (checks, Engine::Running, Some(answer))
}

/// Each lab app's order list health by name, as the store keeps it.
type OrderLists = Vec<(String, Result<analyte_store::Health, String>)>;

/// Whether the store opens, which also checks the data directory's access
/// list, and while it is open when each lab app's last complete order list
/// came. A store that does not exist yet is never created here.
async fn store(program: &Program, check_software: bool) -> (Vec<Check>, OrderLists) {
    let file = program.data_directory().join("analyte.db");
    if !file.is_file() {
        return (
            vec![warn(
                tr("doctor-overview-store"),
                tr_args(
                    "doctor-overview-there-is-no-store-at-yet",
                    [("file", (file.display()).to_string().into())],
                ),
                tr("doctor-overview-start-the-engine-once-start-service-analyte-it-creates"),
            )],
            Vec::new(),
        );
    }
    match crate::open_system_store(program).await {
        Ok(mut store) => {
            let lists = order_lists(program, &store).await;
            let mut software = if check_software {
                software_checks(program, &store).await
            } else {
                Vec::new()
            };
            let closed = match crate::deadline() {
                Ok(until) => store.shutdown(until).await.map_err(|error| chain(&error)),
                Err(error) => Err(chain(&error)),
            };
            let check = match closed {
                Ok(()) => ok(
                    tr("doctor-overview-store"),
                    format!("{} opens", file.display()),
                ),
                Err(error) => warn(
                    tr("doctor-overview-store"),
                    tr_args(
                        "doctor-overview-opened-but-did-not-close-cleanly",
                        [("error", (error).to_string().into())],
                    ),
                    tr("doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte"),
                ),
            };
            software.insert(0, check);
            (software, lists)
        }
        Err(error) => (
            vec![fail(
                tr("doctor-overview-store"),
                tr_args(
                    "doctor-overview-cannot-be-opened",
                    [
                        (
                            "data-directory",
                            (program.data_directory().display()).to_string().into(),
                        ),
                        ("error", (chain(&error)).to_string().into()),
                    ],
                ),
                tr("doctor-overview-run-this-from-an-administrator-powershell-if-it-names"),
            )],
            Vec::new(),
        ),
    }
}

/// Reads stored software warnings when no live verdict is available.
async fn software_checks(program: &Program, store: &analyte_store::Handle) -> Vec<Check> {
    let selected = program
        .machines()
        .iter()
        .filter_map(|machine| {
            let check = machine
                .snapshot()
                .effective()
                .machine()
                .software_version()?
                .expected()?
                .clone();
            Some((machine.id().clone(), check))
        })
        .collect();
    let result = async {
        crate::admin::machines::software_version::warnings(store, crate::deadline()?, selected)
            .await
    }
    .await;
    match result {
        Ok(warnings) => warnings
            .into_iter()
            .map(|(machine, text)| {
                warn(
                    tr_args(
                        "doctor-overview-machine-named-software",
                        [("id", machine.as_str().into())],
                    ),
                    text,
                    tr("doctor-overview-check-the-machine-s-settings"),
                )
            })
            .collect(),
        Err(error) => vec![warn(
            tr("doctor-overview-machine-software"),
            chain(&error),
            tr("doctor-overview-check-the-store-and-run-analyte-doctor-again"),
        )],
    }
}

/// Free space against the space the program always keeps free.
fn disk(program: &Program) -> Check {
    let reserve = program.store_limits.reserve_bytes;
    match fs4::available_space(program.data_directory()) {
        Ok(free) if free > reserve => ok(
            tr("doctor-overview-disk"),
            tr_args(
                "doctor-overview-free-is-always-kept-free",
                [
                    ("free", (format::bytes(free)).to_string().into()),
                    ("reserve", (format::bytes(reserve)).to_string().into()),
                ],
            ),
        ),
        Ok(free) => fail(
            tr("doctor-overview-disk"),
            tr_args(
                "doctor-overview-free-at-or-below-the-kept-free-analyte-refuses",
                [
                    ("free", (format::bytes(free)).to_string().into()),
                    ("reserve", (format::bytes(reserve)).to_string().into()),
                ],
            ),
            tr("doctor-overview-free-disk-space-on-this-drive-analyte-deletes-nothing"),
        ),
        Err(error) => warn(
            tr("doctor-overview-disk"),
            tr_args(
                "doctor-overview-the-free-space-could-not-be-read",
                [("error", (error).to_string().into())],
            ),
            tr("doctor-overview-check-the-data-directory-s-drive-in-file-explorer"),
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
    let reached = match (link, endpoint) {
        (Link::Serial, endpoint) => serial(what, endpoint).await,
        (Link::TcpServer, Endpoint::Fixed(address)) => listen(what, address, running).await,
        (Link::TcpClient, Endpoint::Fixed(address)) => connect(what, address, running).await,
        (Link::Folder, Endpoint::Fixed(path)) => {
            folder(what, path, |path| {
                std::fs::metadata(path).map(|metadata| metadata.is_dir())
            })
            .await
        }
        // The program refuses a network or folder link given by USB serial number.
        (Link::TcpServer | Link::TcpClient | Link::Folder, Endpoint::UsbSerial(number)) => warn(
            what,
            tr_args(
                "doctor-overview-a-usb-serial-number-names-a-network-or-folder",
                [("number", (number.as_str()).to_string().into())],
            ),
            tr("doctor-overview-choose-the-machine-s-address-or-folder-on-the"),
        ),
    };
    let mut checks = vec![reached];
    if link == Link::TcpServer {
        checks.push(devices(what, machine));
    }
    let effective = machine.snapshot().effective();
    if let Some(serial) = crate::admin::setup::cable::of(effective.stack()) {
        // The lab chose these to match the analyzer's screen; a difference
        // from the manual is shown, not judged.
        checks.push(ok(
            tr_args(
                "doctor-overview-cable",
                [("what", (what).to_string().into())],
            ),
            crate::admin::setup::cable::summary(serial, effective.serial_suggestion()),
        ));
    }
    if let Some(choices) = crate::admin::setup::choices::summary(effective.choices()) {
        // The lab chose these to match the analyzer (ADR 0032); like the
        // cable, a difference from the manual is shown, not judged.
        checks.push(ok(
            tr_args(
                "doctor-overview-analyzer-settings",
                [("what", (what).to_string().into())],
            ),
            choices,
        ));
    }
    if let analyte_config::Stack::Autolyser(stack) = effective.stack() {
        checks.push(ok(
            tr_args(
                "doctor-overview-report-poll-interval",
                [("what", (what).to_string().into())],
            ),
            format!("{} ms", stack.poll_ms()),
        ));
    }
    if effective.records_only() {
        // Record only is the lab's choice for a first visit (ADR 0034); it
        // is reported, not judged: nothing goes to a lab app by design.
        checks.push(ok(
            tr_args(
                "doctor-overview-mode",
                [("what", (what).to_string().into())],
            ),
            tr_args(
                "doctor-overview-every-byte-is-kept-and-recorded-under-captures",
                [
                    (
                        "recording-only",
                        (format::recording_only()).to_string().into(),
                    ),
                    ("id", (machine.id().as_str()).to_string().into()),
                ],
            ),
        ));
    }
    checks
}

/// Which devices a machine that connects to Analyte may connect from (ADR
/// 0026): a warning when any device on the network is accepted.
fn devices(what: &str, machine: &crate::installation::config::Machine) -> Check {
    let what = tr_args(
        "doctor-overview-devices",
        [("what", (what).to_string().into())],
    );
    match &machine.snapshot().effective().site().allowed_peers {
        Some(allowed) => ok(
            what,
            tr_args(
                "doctor-overview-accepts-connections-only-from",
                [("allowed", (allowed).to_string().into())],
            ),
        ),
        None => warn(
            what,
            tr("doctor-overview-accepts-connections-from-any-device-on-the-network-which"),
            tr_args(
                "doctor-overview-run-analyte-machines-set-allowed-peer-the-analyzer-s",
                [("id", (machine.id().as_str()).to_string().into())],
            ),
        ),
    }
}

/// Checks that the configured path can be read and is a directory.
/// On timeout the caller stops waiting. An in-flight read-only metadata call
/// can keep its blocking thread until the OS returns or the process exits.
async fn folder(
    what: &str,
    path: &str,
    is_dir: impl FnOnce(&str) -> std::io::Result<bool> + Send + 'static,
) -> Check {
    let probe_path = path.to_owned();
    let found = match tokio::task::spawn_blocking(move || is_dir(&probe_path)).await {
        Ok(found) => found,
        Err(error) => {
            return fail(
                what,
                tr_args(
                    "doctor-overview-the-check-for-folder-failed",
                    [
                        ("path", (path).to_string().into()),
                        ("error", (error).to_string().into()),
                    ],
                ),
                tr("doctor-overview-run-analyte-doctor-again-if-it-repeats-run-analyte"),
            );
        }
    };
    match found {
        Ok(true) => ok(
            what,
            tr_args(
                "doctor-overview-folder-is-there",
                [("path", (path).to_string().into())],
            ),
        ),
        Ok(false) => fail(
            what,
            tr_args(
                "doctor-overview-is-not-a-folder",
                [("path", (path).to_string().into())],
            ),
            tr("doctor-overview-choose-the-machine-s-export-folder-on-the-setup"),
        ),
        Err(error) => fail(
            what,
            tr_args(
                "doctor-overview-folder-cannot-be-read",
                [
                    ("path", (path).to_string().into()),
                    ("error", (error).to_string().into()),
                ],
            ),
            tr("doctor-overview-create-the-folder-or-share-it-again-or-choose"),
        ),
    }
}

/// Whether the serial port is present; it is never opened, so nothing
/// reaches the machine and the engine keeps it.
async fn serial(what: &str, endpoint: &Endpoint) -> Check {
    let listed = match tokio::task::spawn_blocking(analyte_transport::serial::list).await {
        Ok(Ok(listed)) => listed,
        Ok(Err(error)) => {
            return warn(
                what,
                tr_args(
                    "doctor-overview-the-serial-ports-could-not-be-listed",
                    [("error", (error).to_string().into())],
                ),
                tr("doctor-overview-check-the-usb-serial-adapter-s-driver-in-device"),
            );
        }
        Err(error) => {
            return warn(
                what,
                tr_args(
                    "doctor-overview-the-serial-ports-could-not-be-listed",
                    [("error", (error).to_string().into())],
                ),
                tr("doctor-overview-run-analyte-doctor-again"),
            );
        }
    };
    let plug = tr("doctor-overview-plug-in-the-usb-serial-adapter-or-choose-the");
    match endpoint {
        Endpoint::Fixed(name) => {
            if listed.iter().any(|port| {
                crate::installation::config::connections::same_port_name(&port.name, name)
            }) {
                ok(
                    what,
                    tr_args(
                        "doctor-overview-serial-port-is-present",
                        [("name", (name).to_string().into())],
                    ),
                )
            } else {
                fail(
                    what,
                    tr_args(
                        "doctor-overview-serial-port-is-not-present",
                        [("name", (name).to_string().into())],
                    ),
                    plug,
                )
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
                    tr_args(
                        "doctor-overview-the-adapter-with-serial-number-is-at",
                        [
                            ("number", (number.as_str()).to_string().into()),
                            ("port", (port).to_string().into()),
                        ],
                    ),
                ),
                [] => fail(
                    what,
                    tr_args(
                        "doctor-overview-no-adapter-with-serial-number-is-plugged-in",
                        [("number", (number.as_str()).to_string().into())],
                    ),
                    plug,
                ),
                several => fail(
                    what,
                    tr_args(
                        "doctor-overview-adapters-report-serial-number",
                        [
                            ("several", (several.len()).to_string().into()),
                            ("number", (number.as_str()).to_string().into()),
                            ("join", (several.join(", ")).to_string().into()),
                        ],
                    ),
                    tr("doctor-overview-unplug-the-adapter-that-does-not-belong-to-this"),
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
                    tr_args(
                        "doctor-overview-nothing-listens-on-although-the-engine-runs",
                        [("address", (address).to_string().into())],
                    ),
                    tr("doctor-overview-run-analyte-overview-the-machine-s-service-in-analyte"),
                ),
                Engine::Stopped | Engine::Unknown => ok(
                    what,
                    tr_args(
                        "doctor-overview-is-free-for-analyte-to-listen-on",
                        [("address", (address).to_string().into())],
                    ),
                ),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => match running {
            Engine::Running => ok(
                what,
                tr_args(
                    "doctor-overview-is-held-by-the-running-engine",
                    [("address", (address).to_string().into())],
                ),
            ),
            Engine::Stopped => fail(
                what,
                tr_args(
                    "doctor-overview-another-program-holds",
                    [("address", (address).to_string().into())],
                ),
                tr("doctor-overview-stop-the-other-program-or-choose-another-port-on"),
            ),
            Engine::Unknown => warn(
                what,
                tr_args(
                    "doctor-overview-is-held-by-the-engine-or-by-another-program",
                    [("address", (address).to_string().into())],
                ),
                tr("doctor-overview-turn-the-admin-page-on-so-doctor-can-tell"),
            ),
        },
        Err(error) => fail(
            what,
            tr_args(
                "doctor-overview-cannot-be-used-on-this-computer",
                [
                    ("address", (address).to_string().into()),
                    ("error", (error).to_string().into()),
                ],
            ),
            tr("doctor-overview-choose-an-address-this-computer-has-on-the-setup"),
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
                tr_args(
                    "doctor-overview-the-engine-connects-to-analyte-overview-says-if-the",
                    [("address", (address).to_string().into())],
                ),
            );
        }
        Engine::Unknown => {
            return warn(
                what,
                tr_args(
                    "doctor-overview-not-tried-the-engine-may-be-connected-to-and",
                    [("address", (address).to_string().into())],
                ),
                tr("doctor-overview-turn-the-admin-page-on-so-doctor-can-tell-2"),
            );
        }
        Engine::Stopped => {}
    }
    match tokio::time::timeout(CONNECT_TIME, tokio::net::TcpStream::connect(address)).await {
        // Closed at once: nothing is sent.
        Ok(Ok(stream)) => {
            drop(stream);
            ok(
                what,
                tr_args(
                    "doctor-overview-accepts-a-connection",
                    [("address", (address).to_string().into())],
                ),
            )
        }
        Ok(Err(error)) => warn(
            what,
            tr_args(
                "doctor-overview-does-not-accept-a-connection",
                [
                    ("address", (address).to_string().into()),
                    ("error", (error).to_string().into()),
                ],
            ),
            tr("doctor-overview-check-that-the-machine-is-on-its-network-settings"),
        ),
        Err(_) => warn(
            what,
            tr_args(
                "doctor-overview-did-not-answer-within-seconds",
                [
                    ("address", (address).to_string().into()),
                    ("as-secs", (CONNECT_TIME.as_secs()).to_string().into()),
                ],
            ),
            tr("doctor-overview-check-that-the-machine-is-on-its-network-settings"),
        ),
    }
}

fn lab_app_key_advice() -> &'static str {
    tr("doctor-overview-lab-app-key-advice")
}

/// Each lab app: its name resolves, it answers over HTTPS with a valid
/// certificate, its keys are there, and when the last complete order list came.
async fn lab_apps(program: &Program, answer: Option<&Answer>, health: &OrderLists) -> Vec<Check> {
    let mut checks = Vec::new();
    // Read once for every lab app, and only while the engine cannot say.
    let stored = answer.is_none().then(|| {
        Locked::new(program.data_directory())
            .read()
            .map_err(SecretError::from)
    });
    for settings in every_lab_app(program) {
        let name = settings.name.as_str();
        let what = tr_args(
            "doctor-overview-lab-app",
            [("name", (name).to_string().into())],
        );
        let url = settings.base_url.url();
        checks.push(reach(&what, url).await);
        checks.push(match (answer, &stored) {
            (Some(answer), _) => match answer.needs.iter().find(|need| {
                need.reason == "lab_app_blocked" && need.about.as_deref() == Some(name)
            }) {
                Some(need) => fail(&what, need.text.clone(), lab_app_key_advice()),
                None => ok(&what, tr("doctor-overview-the-running-engine-has-its-keys")),
            },
            (None, Some(Ok(stored))) => {
                match secrets::read_destination(
                    program,
                    settings,
                    |key| std::env::var(key).ok(),
                    stored,
                ) {
                    Ok(_) => ok(
                        &what,
                        tr("doctor-overview-its-api-key-and-signing-key-are-there"),
                    ),
                    Err(error) => lab_app_key_failure(&what, &error),
                }
            }
            (None, Some(Err(error))) => lab_app_key_failure(&what, error),
            (None, None) => warn(
                &what,
                tr("doctor-overview-its-keys-were-not-checked"),
                tr("doctor-overview-run-analyte-doctor-again"),
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
                    tr_args(
                        "doctor-overview-when-its-order-list-last-came-could-not-be",
                        [("error", (error).to_string().into())],
                    ),
                    tr("doctor-overview-run-analyte-doctor-again"),
                ),
                None => warn(
                    &what,
                    tr("doctor-overview-no-order-list-is-kept-for-it"),
                    tr("doctor-overview-restart-the-engine"),
                ),
            });
        }
    }
    checks
}

/// Gives different remedies for a bad key and a key store this account cannot check.
fn lab_app_key_failure(what: &str, error: &SecretError) -> Check {
    match error {
        SecretError::Missing(name) => fail(
            what,
            tr_args(
                "doctor-overview-credential-is-missing",
                [("name", (name).to_string().into())],
            ),
            lab_app_key_advice(),
        ),
        SecretError::Invalid(_) => fail(what, error.to_string(), lab_app_key_advice()),
        SecretError::File(error) => warn(
            what,
            tr_args(
                "doctor-overview-its-keys-could-not-be-read-from-this-account",
                [("error", (chain(error)).to_string().into())],
            ),
            tr("doctor-overview-start-the-engine-and-run-analyte-doctor-again-the"),
        ),
    }
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
        // A record-only machine names no lab app (ADR 0034).
        if let Some(own) = &machine.snapshot().effective().site().destination
            && !all.iter().any(|known| known.name == own.name)
        {
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
            tr("doctor-overview-its-address-has-no-host-name"),
            tr("doctor-overview-fix-its-address-on-the-lab-apps-page"),
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
                tr_args(
                    "doctor-overview-resolves-to-no-address",
                    [("host", (host).to_string().into())],
                ),
                tr("doctor-overview-check-the-lab-app-s-address-on-the-lab"),
            );
        }
        Ok(Err(error)) => {
            return fail(
                what,
                tr_args(
                    "doctor-overview-does-not-resolve",
                    [
                        ("host", (host).to_string().into()),
                        ("error", (error).to_string().into()),
                    ],
                ),
                tr("doctor-overview-check-the-lab-app-s-address-on-the-lab"),
            );
        }
        Err(_) => {
            return fail(
                what,
                tr_args(
                    "doctor-overview-did-not-resolve-within-seconds",
                    [
                        ("host", (host).to_string().into()),
                        ("as-secs", (CONNECT_TIME.as_secs()).to_string().into()),
                    ],
                ),
                tr("doctor-overview-check-this-computer-s-dns-and-network"),
            );
        }
    }
    match analyte_sink::answers(url, CONNECT_TIME).await {
        Ok(status) => ok(
            what,
            tr_args(
                "doctor-overview-answers-over-https-with-a-valid-certificate-http",
                [
                    ("url", (url).to_string().into()),
                    ("status", (status).to_string().into()),
                ],
            ),
        ),
        Err(error) => fail(
            what,
            tr_args(
                "doctor-overview-gives-no-https-answer",
                [
                    ("url", (url).to_string().into()),
                    ("error", (chain(&error)).to_string().into()),
                ],
            ),
            tr("doctor-overview-check-the-address-the-lab-app-s-server-and"),
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
                .filter_map(|machine| machine.snapshot().effective().site().destination.as_ref())
                .map(|destination| (destination.name.as_str().to_owned(), Err(why.clone())))
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
        ok(
            what,
            tr_args(
                "doctor-overview-last-complete-order-list",
                [("when", (when).to_string().into())],
            ),
        )
    } else {
        warn(
            what,
            tr_args(
                "doctor-overview-its-order-list-is-not-up-to-date-last",
                [("when", (when).to_string().into())],
            ),
            tr("doctor-overview-check-that-the-engine-runs-and-the-lab-app"),
        )
    }
}

/// Whether the local "Analyte users" group exists, so members can sign in.
#[cfg(windows)]
fn users_group() -> Check {
    use crate::admin::server::windows::{Found, GROUP};
    let what = tr("admin-windows-sign-in");
    let create = tr_args(
        "doctor-overview-create-it-with-new-localgroup-name-then-add-people",
        [("GROUP", (GROUP).to_string().into())],
    );
    match crate::admin::server::windows::users_group() {
        Found::Group(_) => ok(
            what,
            tr_args(
                "doctor-overview-the-group-exists",
                [("GROUP", (GROUP).to_string().into())],
            ),
        ),
        Found::Missing => warn(
            what,
            tr_args(
                "doctor-overview-the-group-is-missing-so-only-administrators-can-sign",
                [("GROUP", (GROUP).to_string().into())],
            ),
            create,
        ),
        Found::NotAGroup => warn(
            what,
            tr_args(
                "doctor-overview-is-an-account-but-not-a-local-group",
                [("GROUP", (GROUP).to_string().into())],
            ),
            create,
        ),
        Found::NoComputerName => warn(
            what,
            tr("doctor-overview-computername-is-not-set-so-the-group-cannot-be"),
            tr("doctor-overview-run-doctor-from-a-normal-powershell-window"),
        ),
        Found::Failed(error) => warn(
            what,
            tr_args(
                "doctor-overview-the-group-could-not-be-looked-up",
                [
                    ("GROUP", (GROUP).to_string().into()),
                    ("error", (chain(&error)).to_string().into()),
                ],
            ),
            tr("doctor-overview-run-analyte-doctor-again"),
        ),
    }
}

/// Whether the installer's firewall rule lets analyzers connect, when a
/// machine connects to Analyte; read with `Get-NetFirewallRule`, which changes nothing.
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
    // Read-only: finds the installer's rule by its fixed Name and changes nothing.
    let script = format!(
        "if (Get-NetFirewallRule -Name '{FIREWALL_RULE_NAME}' -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
    );
    let shown = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status();
    vec![match tokio::time::timeout(FIREWALL_TIME, shown).await {
        Ok(Ok(status)) if status.success() => ok(
            what,
            tr_args(
                "doctor-overview-the-rule-is-there",
                [
                    ("FIREWALL-RULE", (FIREWALL_RULE).to_string().into()),
                    (
                        "FIREWALL-RULE-NAME",
                        (FIREWALL_RULE_NAME).to_string().into(),
                    ),
                ],
            ),
        ),
        Ok(Ok(status)) if status.code() == Some(1) => warn(
            what,
            tr_args(
                "doctor-overview-no-rule-lets-analyzers-connect-to",
                [
                    ("FIREWALL-RULE", (FIREWALL_RULE).to_string().into()),
                    (
                        "FIREWALL-RULE-NAME",
                        (FIREWALL_RULE_NAME).to_string().into(),
                    ),
                    ("join", (ports.join(", ")).to_string().into()),
                ],
            ),
            tr("doctor-overview-run-the-installer-again-or-allow-inbound-tcp-to"),
        ),
        Ok(Ok(status)) => warn(
            what,
            tr_args(
                "doctor-overview-the-firewall-rules-could-not-be-read-powershell-ended",
                [("status", (status).to_string().into())],
            ),
            tr("doctor-overview-check-the-rule-in-windows-defender-firewall"),
        ),
        Ok(Err(error)) => warn(
            what,
            tr_args(
                "doctor-overview-the-firewall-rules-could-not-be-read",
                [("error", (error).to_string().into())],
            ),
            tr("doctor-overview-check-the-rule-in-windows-defender-firewall"),
        ),
        Err(_) => warn(
            what,
            tr("doctor-overview-reading-the-firewall-rules-took-too-long"),
            tr("doctor-overview-check-the-rule-in-windows-defender-firewall"),
        ),
    }]
}
