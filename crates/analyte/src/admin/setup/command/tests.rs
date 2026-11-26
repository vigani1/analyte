//! Tests the `machines` and `lab-apps` commands on temporary installations: the pages' checks and refusals, dry runs, backups, roll back, audit rows, keys and the listen-only test.
use super::{Failure, Outcome, ask::Ask, lab_apps, machines};
use crate::admin::setup::{Limits, Seams, SetupError};
use crate::installation::{config::Program, credentials::Locked};
use analyte_fixtures::ConfigEdit as _;
use analyte_model::Identifier;
use clap::Parser as _;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

type Error = Box<dyn std::error::Error + Send + Sync>;

/// Synthetic keys; neither may appear in a file but the locked one, or in an audit row.
const API_KEY: &str = "synthetic-command-api-key-7QZ";
const SIGNING_KEY: &str = analyte_fixtures::SECRET;
/// What the fake serial port sends: an ASTM request to send, then a header frame.
const ASTM: &[u8] = b"\x05\x021H|\\^&|||SYN\r\x0300\r\n";

/// A temporary installation: shipped example models, a program file, and a
/// fake serial port that records anything written to it.
struct Install {
    _directory: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    written: Arc<Mutex<Vec<u8>>>,
    /// The line settings each open of the fake serial port asked for.
    opened: Arc<Mutex<Vec<String>>>,
}

/// A fake serial port: sends [`ASTM`] once, then only times out; records writes.
struct FakePort {
    input: Vec<u8>,
    written: Arc<Mutex<Vec<u8>>>,
}

impl std::io::Read for FakePort {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.input.is_empty() {
            std::thread::sleep(Duration::from_millis(10));
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let count = buffer.len().min(self.input.len());
        buffer
            .get_mut(..count)
            .ok_or_else(|| std::io::Error::other("buffer"))?
            .copy_from_slice(self.input.drain(..count).as_slice());
        Ok(count)
    }
}

impl std::io::Write for FakePort {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.written
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A loopback port nothing listens on, so "the engine is not running".
fn free_port() -> Result<u16, Error> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

fn install() -> Result<Install, Error> {
    install_on(free_port()?)
}

/// An installation whose admin pages are on loopback port `admin`.
fn install_on(admin: u16) -> Result<Install, Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path().to_owned();
    for (folder, text) in [
        (
            "machines/example/serial-astm",
            include_str!("../../../../../core/config/examples/astm.toml").to_owned(),
        ),
        (
            "machines/example/network-hl7",
            include_str!("../../../../../core/config/examples/hl7.toml").to_owned(),
        ),
        (
            // Validates on its own, but its record bound exceeds the store's.
            "machines/example/big-hl7",
            include_str!("../../../../../core/config/examples/hl7.toml")
                .rewrite("bytes = 1048576", "bytes = 4194304"),
        ),
    ] {
        std::fs::create_dir_all(root.join(folder))?;
        std::fs::write(root.join(folder).join("machine.toml"), text)?;
    }
    let blocked = root.join("machines/roche/cobas-e411");
    std::fs::create_dir_all(&blocked)?;
    std::fs::write(
        blocked.join("README.md"),
        "# Roche cobas e 411\n\nBlocked.\n",
    )?;
    let program = analyte_fixtures::program_with_limits(
        &format!("[admin]\nport = {admin}\n"),
        &analyte_store::Limits {
            reserve_bytes: 1,
            database_bytes: 100_000_000,
            record_bytes: 2_097_152,
            page_rows: 100,
            busy_timeout: Duration::from_millis(100),
        },
    );
    let config = root.join("analyte.toml");
    std::fs::write(&config, program)?;
    Ok(Install {
        _directory: directory,
        root,
        config,
        written: Arc::new(Mutex::new(Vec::new())),
        opened: Arc::new(Mutex::new(Vec::new())),
    })
}

impl Install {
    fn seams(&self) -> Seams {
        let (written, opened) = (self.written.clone(), self.opened.clone());
        Seams {
            definitions: Some(self.root.join("machines")),
            ports: Arc::new(|| {
                Ok(vec![analyte_transport::serial::Listed {
                    name: "COM12".into(),
                    usb_serial: Some("A10K3XYZ".into()),
                    description: Some("FT232R USB UART".into()),
                }])
            }),
            open: Arc::new(move |_, settings| {
                opened
                    .lock()
                    .map_err(|_| std::io::Error::other("poisoned"))?
                    .push(format!("{settings:?}"));
                Ok(Box::new(FakePort {
                    input: ASTM.to_vec(),
                    written: written.clone(),
                })
                    as Box<dyn analyte_transport::serial::Port>)
            }),
            limits: Limits {
                duration: Duration::from_secs(2),
                bytes: 1024,
            },
            addresses: Arc::new(|| Ok(Vec::new())),
        }
    }

    /// Runs `machines <args>` as `it-test`, never asking.
    async fn machines(&self, args: &[&str]) -> Result<Outcome, Failure> {
        let command =
            MachinesCli::try_parse_from(std::iter::once("machines").chain(args.iter().copied()))
                .map_err(|error| Failure::Console(std::io::Error::other(error.to_string())))?
                .command;
        machines::run(&self.config, actor(), command, Ask::Never, self.seams()).await
    }

    /// Runs `lab-apps <args>` as `it-test` with `keys` as the keys entered.
    async fn lab_apps(&self, args: &[&str], keys: [&str; 2]) -> Result<Outcome, Failure> {
        let command =
            LabAppsCli::try_parse_from(std::iter::once("lab-apps").chain(args.iter().copied()))
                .map_err(|error| Failure::Console(std::io::Error::other(error.to_string())))?
                .command;
        let keys = keys.map(str::to_owned).to_vec();
        let keys: lab_apps::Keys = Box::new(move |_| Ok(keys));
        lab_apps::run(
            &self.config,
            actor(),
            command,
            Ask::Never,
            self.seams(),
            keys,
        )
        .await
    }

    /// Every settings file and its text; dated backups left out.
    fn settings(&self) -> Result<BTreeMap<String, String>, Error> {
        let mut files = BTreeMap::new();
        for entry in std::fs::read_dir(&self.root)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            if path.is_file() && !name.ends_with(".bak") {
                files.insert(name, std::fs::read_to_string(&path)?);
            }
        }
        Ok(files)
    }

    /// The dated backups beside the program file.
    fn backups(&self) -> Result<Vec<String>, Error> {
        Ok(std::fs::read_dir(&self.root)?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.ends_with(".bak"))
            .collect())
    }

    /// Every audit row: its actor, action, first target and reason.
    async fn audits(&self) -> Result<Vec<(String, String, String, String)>, Error> {
        let program = Program::load(&self.config)?;
        let mut store = crate::open_system_store(&program).await?;
        let rows = store
            .request::<_, analyte_store::OperatorError>(crate::deadline()?, |store| store.audits(0))
            .await;
        store.shutdown(crate::deadline()?).await?;
        Ok(rows?
            .into_iter()
            .map(|(_, audit)| {
                (
                    audit.actor.as_str().to_owned(),
                    audit.action.as_str().to_owned(),
                    audit
                        .targets
                        .first()
                        .map(|t| t.as_str().to_owned())
                        .unwrap_or_default(),
                    audit.reason.as_str().to_owned(),
                )
            })
            .collect())
    }

    /// Lab app `demo` with both keys, and machine `cobas-1` set up on the
    /// fake USB serial adapter and sending to it.
    async fn set_up(&self) -> Result<(), Error> {
        let demo = ["add", "demo", "--address", "https://lab.example.invalid"];
        assert_eq!(
            self.lab_apps(&demo, [API_KEY, SIGNING_KEY]).await?,
            Outcome::Done
        );
        assert_eq!(self.machines(&["add", "cobas-1"]).await?, Outcome::Done);
        let set = [
            "set",
            "cobas-1",
            "--model",
            "example/serial-astm",
            "--usb-serial",
            "A10K3XYZ",
            "--lab-app",
            "demo",
        ];
        assert_eq!(self.machines(&set).await?, Outcome::Done);
        Ok(())
    }
}

#[derive(clap::Parser)]
struct MachinesCli {
    #[command(subcommand)]
    command: machines::Command,
}

#[derive(clap::Parser)]
struct LabAppsCli {
    #[command(subcommand)]
    command: lab_apps::Command,
}

fn actor() -> Option<Identifier> {
    Identifier::new("it-test").ok()
}

/// The setup refusal inside `result`, or a description of what came instead.
fn refusal(result: Result<Outcome, Failure>) -> Result<SetupError, String> {
    match result {
        Err(Failure::Setup(error)) => Ok(error),
        other => Err(format!("expected a setup refusal, got {other:?}")),
    }
}

// Owner request 2026-11-21 (command line), plan §11, ADR 0003 rule 2: add, set
// and the lab app with its keys go through the pages' own writes: the program
// entry, the starter and saved site files, lab-apps.toml, dated backups, a
// program that still loads, keys only in the locked file, and one audit row
// each naming the actor and the command line.
#[tokio::test(flavor = "multi_thread")]
async fn add_and_set_write_what_the_pages_write_with_backups_and_audit() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let program = std::fs::read_to_string(&install.config)?;
    assert!(program.contains("id = \"cobas-1\""), "{program}");
    assert!(
        program.contains("definition = \"machines/example/serial-astm/machine.toml\""),
        "{program}"
    );
    let site = std::fs::read_to_string(install.root.join("cobas-1.site.toml"))?;
    assert!(site.contains("usb_serial = \"A10K3XYZ\""), "{site}");
    assert!(site.contains("lab_app = \"demo\""), "{site}");
    let lab_apps = std::fs::read_to_string(install.root.join("lab-apps.toml"))?;
    assert!(
        lab_apps.contains("https://lab.example.invalid"),
        "{lab_apps}"
    );
    let backups = install.backups()?;
    assert!(
        backups.iter().any(|name| name.starts_with("analyte.toml.")),
        "{backups:?}"
    );
    assert!(
        backups
            .iter()
            .any(|name| name.starts_with("cobas-1.site.toml.")),
        "{backups:?}"
    );
    let loaded = Program::load(&install.config)?;
    assert_eq!(loaded.machines().len(), 1);
    let stored = Locked::new(loaded.data_directory()).read()?;
    assert_eq!(stored.value("demo-api"), Some(API_KEY));
    assert_eq!(stored.value("demo-signing"), Some(SIGNING_KEY));
    for (name, text) in install.settings()? {
        assert!(
            !text.contains(API_KEY) && !text.contains(SIGNING_KEY),
            "{name}"
        );
    }
    let audits = install.audits().await?;
    let actions: Vec<(&str, &str, &str)> = audits
        .iter()
        .map(|(actor, action, target, _)| (actor.as_str(), action.as_str(), target.as_str()))
        .collect();
    assert_eq!(
        actions,
        [
            ("it-test", "lab-app-add", "demo"),
            ("it-test", "setup-add", "cobas-1"),
            ("it-test", "setup-save", "cobas-1"),
        ]
    );
    for (_, _, _, reason) in &audits {
        assert!(reason.starts_with("command line "), "{reason}");
        assert!(!reason.contains(API_KEY) && !reason.contains(SIGNING_KEY));
    }
    Ok(())
}

// Owner request 2026-11-21 (command line): every refusal of the Save step
// refuses here too, in the page's words, and changes no file: an unknown lab
// app, a model that is not ready, an unknown model, a connection another
// machine already uses, and a model whose data is too large for the store.
#[tokio::test(flavor = "multi_thread")]
async fn set_refuses_as_the_save_step_does_and_changes_nothing() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    assert_eq!(install.machines(&["add", "hl7-2"]).await?, Outcome::Done);
    let network = [
        "set",
        "hl7-2",
        "--model",
        "example/network-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        "5600",
        "--lab-app",
        "demo",
    ];
    assert_eq!(install.machines(&network).await?, Outcome::Done);
    assert_eq!(install.machines(&["add", "hl7-3"]).await?, Outcome::Done);
    let before = install.settings()?;
    let refused = |args: &'static [&'static str]| install.machines(args);
    let error = refusal(refused(&["set", "cobas-1", "--lab-app", "nope"]).await)?;
    assert!(
        matches!(error, SetupError::UnknownLabApp(ref name) if name == "nope"),
        "{error}"
    );
    let error = refusal(refused(&["set", "cobas-1", "--model", "roche/cobas-e411"]).await)?;
    assert!(matches!(error, SetupError::BlockedDefinition(_)), "{error}");
    assert!(error.to_string().contains("is not ready yet"), "{error}");
    let error = refusal(refused(&["set", "cobas-1", "--model", "nobody/none"]).await)?;
    assert!(matches!(error, SetupError::UnknownDefinition), "{error}");
    let taken = &[
        "set",
        "hl7-3",
        "--model",
        "example/network-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        "5600",
        "--lab-app",
        "demo",
    ];
    let error = refusal(refused(taken).await)?;
    assert!(
        matches!(
            error,
            SetupError::Installation(crate::installation::config::Error::SharedConnection { .. })
        ),
        "{error}"
    );
    assert_eq!(install.settings()?, before);
    // AGENTS.md "One way per thing": save and dry run refuse the same store limit.
    let too_big = [
        "set",
        "hl7-2",
        "--model",
        "example/big-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        "5601",
    ];
    let backups = install.backups()?;
    let mut dry_run = too_big.to_vec();
    dry_run.push("--dry-run");
    for args in [too_big.as_slice(), dry_run.as_slice()] {
        let error = refusal(install.machines(args).await)?;
        assert!(
            matches!(
                error,
                SetupError::Installation(
                    crate::installation::config::Error::StoreRecordLimitTooSmall
                )
            ),
            "{error}"
        );
    }
    assert_eq!(install.settings()?, before);
    assert_eq!(install.backups()?, backups);
    Ok(())
}

// Owner request 2026-11-21 (command line): --dry-run makes every check of the
// save and writes nothing, not even a backup or an audit row.
#[tokio::test(flavor = "multi_thread")]
async fn dry_run_checks_and_writes_nothing() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let (before, backups) = (install.settings()?, install.backups()?);
    let audits = install.audits().await?.len();
    let dry = [
        "set",
        "cobas-1",
        "--model",
        "example/network-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        "5700",
        "--dry-run",
    ];
    assert_eq!(install.machines(&dry).await?, Outcome::Done);
    assert_eq!(install.settings()?, before);
    assert_eq!(install.backups()?, backups);
    assert_eq!(install.audits().await?.len(), audits);
    let refused = ["set", "cobas-1", "--lab-app", "nope", "--dry-run"];
    assert!(refusal(install.machines(&refused).await).is_ok());
    assert_eq!(install.settings()?, before);
    Ok(())
}

// ADR 0003 rule 2 and the Remove dialog: removing refuses while work is still
// unfinished, unless --anyway; then the entry goes, the site file is kept as a
// dated copy, the lab app stays, and the store keeps everything.
#[tokio::test(flavor = "multi_thread")]
async fn remove_refuses_unfinished_work_unless_anyway() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let program = Program::load(&install.config)?;
    let mut store = crate::open_system_store(&program).await?;
    let raw = b"synthetic-unread-message";
    let id = Identifier::new;
    let metadata = analyte_model::ReceiptMetadata {
        id: analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())?,
        raw_digest: analyte_model::Digest(analyte_contract::content_digest(raw)),
        connection: id("cobas-1")?,
        machine: id("cobas-1")?,
        driver: analyte_model::NamespacedId {
            namespace: id("synthetic")?,
            value: id("1")?,
        },
        config_version: id("1")?,
        config_digest: analyte_model::Digest(analyte_contract::content_digest(b"c")),
        received_at: jiff::Timestamp::from_second(100)?,
    };
    let stored = store
        .request::<_, analyte_store::IntakeError>(crate::deadline()?, move |store| {
            store.receive(&metadata, raw, b"c")
        })
        .await;
    store.shutdown(crate::deadline()?).await?;
    stored?;
    let before = install.settings()?;
    let error = refusal(install.machines(&["remove", "cobas-1"]).await)?;
    assert!(matches!(error, SetupError::UnfinishedWork(..)), "{error}");
    assert!(
        error
            .to_string()
            .contains("1 received message not read yet"),
        "{error}"
    );
    assert_eq!(install.settings()?, before);
    assert_eq!(
        install.machines(&["remove", "cobas-1", "--anyway"]).await?,
        Outcome::Done
    );
    let program = std::fs::read_to_string(&install.config)?;
    assert!(!program.contains("cobas-1"), "{program}");
    assert!(!install.root.join("cobas-1.site.toml").exists());
    assert!(
        install
            .backups()?
            .iter()
            .any(|name| name.starts_with("cobas-1.site.toml."))
    );
    assert!(std::fs::read_to_string(install.root.join("lab-apps.toml"))?.contains("demo"));
    let audits = install.audits().await?;
    let (_, _, _, reason) = audits.last().ok_or("no audit")?;
    assert!(
        reason.starts_with("command line removed machine cobas-1"),
        "{reason}"
    );
    assert!(reason.contains("removed anyway: yes"), "{reason}");
    Ok(())
}

// Owner request 2026-11-21 (command line), AGENTS.md rule 7: the test is step
// 3's listen-only test: it opens the saved port, reads within its bounds,
// says the page's verdict and never writes a byte to the machine. With the
// engine not running, no port counts as taken.
#[tokio::test(flavor = "multi_thread")]
async fn test_listens_on_the_saved_port_and_sends_nothing() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let outcome = install
        .machines(&["test", "cobas-1", "--seconds", "1"])
        .await?;
    assert_eq!(outcome, Outcome::Done, "the fake port sends ASTM");
    assert!(install.written.lock().map_err(|_| "poisoned")?.is_empty());
    let error = refusal(install.machines(&["test", "nobody"]).await)?;
    assert!(matches!(error, SetupError::UnknownMachine(_)), "{error}");
    assert!(
        MachinesCli::try_parse_from(["machines", "test", "cobas-1", "--seconds", "61"]).is_err()
    );
    Ok(())
}

// Owner request 2026-11-21 (command line): the Lab apps page's refusals hold
// here too and change nothing: a new lab app needs both keys and a plain name,
// keys for an unknown lab app are refused, and one a machine uses stays.
// A change keeps the values not given; new keys replace only those entered.
#[tokio::test(flavor = "multi_thread")]
async fn lab_apps_refuse_as_the_page_does_and_keep_what_is_not_given() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let before = install.settings()?;
    let address = "https://other.example.invalid";
    let error = refusal(
        install
            .lab_apps(&["add", "other", "--address", address], ["", ""])
            .await,
    )?;
    assert!(matches!(error, SetupError::KeysRequired), "{error}");
    let error = refusal(
        install
            .lab_apps(
                &["add", "two words", "--address", address],
                [API_KEY, SIGNING_KEY],
            )
            .await,
    )?;
    assert!(matches!(error, SetupError::LabAppName), "{error}");
    let error = refusal(install.lab_apps(&["keys", "nope"], [API_KEY, ""]).await)?;
    assert!(matches!(error, SetupError::UnknownLabApp(_)), "{error}");
    let error = refusal(install.lab_apps(&["remove", "demo"], ["", ""]).await)?;
    assert!(matches!(error, SetupError::LabAppInUse(..)), "{error}");
    assert_eq!(install.settings()?, before);
    let set = ["set", "demo", "--request-timeout-seconds", "20"];
    assert_eq!(install.lab_apps(&set, ["", ""]).await?, Outcome::Done);
    let lab_apps = std::fs::read_to_string(install.root.join("lab-apps.toml"))?;
    assert!(
        lab_apps.contains("request_timeout_seconds = 20"),
        "{lab_apps}"
    );
    assert!(
        lab_apps.contains("https://lab.example.invalid"),
        "{lab_apps}"
    );
    let replaced = "synthetic-command-api-key-NEW";
    assert_eq!(
        install.lab_apps(&["keys", "demo"], [replaced, ""]).await?,
        Outcome::Done
    );
    let stored = Locked::new(Program::load(&install.config)?.data_directory()).read()?;
    assert_eq!(stored.value("demo-api"), Some(replaced));
    assert_eq!(stored.value("demo-signing"), Some(SIGNING_KEY));
    let audits = install.audits().await?;
    let (_, action, _, reason) = audits.last().ok_or("no audit")?;
    assert_eq!(action, "lab-app-save");
    assert!(reason.contains("keys replaced: demo-api"), "{reason}");
    assert!(!reason.contains(replaced), "{reason}");
    Ok(())
}

/// A listener on the admin port that is not the engine: it records every
/// request and answers each with `answer` (raw HTTP), or closes at once.
async fn impostor(
    answer: Option<String>,
) -> Result<(u16, Arc<Mutex<Vec<u8>>>, tokio::task::JoinHandle<()>), Error> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let kept = heard.clone();
    let owner = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = vec![0_u8; 8192];
            let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer)).await;
            if let Ok(Ok(count)) = read
                && let (Ok(mut heard), Some(bytes)) = (kept.lock(), buffer.get(..count))
            {
                heard.extend_from_slice(bytes);
            }
            if let Some(answer) = &answer {
                let _ = stream.write_all(answer.as_bytes()).await;
            }
        }
    });
    Ok((port, heard, owner))
}

// ADR 0025 (threat: a local listener on the admin port while the service is
// stopped): `machines list` sends the admin key only to a listener that
// proves it holds it. A listener that answers a wrong proof, an error or
// nothing never receives the key or an Authorization header, and the live
// state is "unknown", never "not running".
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_key_goes_only_to_a_listener_that_proves_it_holds_it() -> Result<(), Error> {
    let wrong = format!("{{\"proof\":\"{}\"}}", "00".repeat(32));
    let answers = [
        Some(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{wrong}",
            wrong.len()
        )),
        Some("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()),
        None,
    ];
    for answer in answers {
        let (port, heard, owner) = impostor(answer).await?;
        let install = install_on(port)?;
        let data = install.root.join("data");
        std::fs::create_dir_all(&data)?;
        // The engine refuses a data directory others can read (Linux 0700).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
        }
        let key = crate::admin::rotate_token(&data)?;
        let program = Program::load(&install.config)?;
        match super::live::read(&program).await {
            super::live::Live::Unknown(why) => {
                assert!(
                    why.to_string()
                        .contains("could not prove it is this Analyte engine"),
                    "{why}"
                );
            }
            super::live::Live::Running { .. } | super::live::Live::NotRunning => {
                return Err("an unproven listener was trusted or called not running".into());
            }
        }
        owner.abort();
        let heard = String::from_utf8_lossy(&heard.lock().map_err(|_| "poisoned")?).into_owned();
        assert!(heard.contains("GET /proof?nonce="), "{heard}");
        assert!(
            !heard.to_ascii_lowercase().contains("authorization"),
            "{heard}"
        );
        assert!(!heard.contains(&key), "the admin key was sent");
        assert!(!heard.contains("/machines.json"), "{heard}");
    }
    Ok(())
}

/// The `allowed_peers` of machine `id`'s site file, as written.
fn site_peers(install: &Install, id: &str) -> Result<Option<Vec<String>>, Error> {
    let site: toml::Table =
        std::fs::read_to_string(install.root.join(format!("{id}.site.toml")))?.parse()?;
    Ok(site
        .get("allowed_peers")
        .and_then(toml::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect()
        }))
}

/// Adds machine `id` listening on 127.0.0.1:`port` with the network model, plus `extra` options.
async fn set_listening(
    install: &Install,
    id: &str,
    port: &str,
    extra: &[&str],
) -> Result<Outcome, Failure> {
    let mut args = vec![
        "set",
        id,
        "--model",
        "example/network-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        port,
        "--lab-app",
        "demo",
    ];
    args.extend_from_slice(extra);
    install.machines(&args).await
}

// ADR 0026 decision 2: `machines set` names the devices a listening machine
// accepts with a repeatable --allowed-peer, keeps them when other choices
// change (a new listening address included), clears them with --any-device,
// and refuses what the site file refuses, changing nothing.
#[tokio::test(flavor = "multi_thread")]
async fn set_names_keeps_and_clears_the_allowed_devices() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    assert_eq!(install.machines(&["add", "hl7-2"]).await?, Outcome::Done);
    let peers = [
        "--allowed-peer",
        "192.168.1.50",
        "--allowed-peer",
        "fd00::7",
    ];
    assert_eq!(
        set_listening(&install, "hl7-2", "5600", &peers).await?,
        Outcome::Done
    );
    let both = Some(vec!["192.168.1.50".to_owned(), "fd00::7".to_owned()]);
    assert_eq!(site_peers(&install, "hl7-2")?, both);
    assert_eq!(
        set_listening(&install, "hl7-2", "5601", &[]).await?,
        Outcome::Done
    );
    assert_eq!(site_peers(&install, "hl7-2")?, both, "kept on a new port");
    let program = Program::load(&install.config)?;
    let loaded = program
        .machines()
        .iter()
        .find(|machine| machine.id().as_str() == "hl7-2")
        .ok_or("hl7-2 not loaded")?;
    let allowed = loaded.snapshot().effective().site().allowed_peers.as_ref();
    let analyzer = std::net::IpAddr::from([192, 168, 1, 50]);
    assert!(
        allowed.is_some_and(|peers| peers.allows(analyzer)),
        "{allowed:?}"
    );
    let before = install.settings()?;
    for bad in [
        &["set", "hl7-2", "--allowed-peer", "192.168.1.0/24"][..],
        &["set", "hl7-2", "--allowed-peer", "analyzer.lab"][..],
        &["set", "cobas-1", "--allowed-peer", "192.168.1.50"][..],
    ] {
        let error = refusal(install.machines(bad).await)?;
        assert!(
            matches!(error, SetupError::Connection(_)),
            "{bad:?}: {error}"
        );
    }
    assert_eq!(install.settings()?, before, "a refusal changes nothing");
    assert!(
        MachinesCli::try_parse_from([
            "machines",
            "set",
            "hl7-2",
            "--allowed-peer",
            "192.168.1.50",
            "--any-device"
        ])
        .is_err(),
        "a list and any device cannot both be chosen"
    );
    assert_eq!(
        install.machines(&["set", "hl7-2", "--any-device"]).await?,
        Outcome::Done
    );
    assert_eq!(site_peers(&install, "hl7-2")?, None);
    Ok(())
}

// ADR 0026 decision 2: allowed devices apply only to an analyzer dialing in.
#[tokio::test(flavor = "multi_thread")]
async fn a_serial_model_does_not_inherit_a_network_allowlist() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    assert_eq!(install.machines(&["add", "other-2"]).await?, Outcome::Done);
    assert_eq!(
        set_listening(
            &install,
            "other-2",
            "5601",
            &["--allowed-peer", "192.168.1.50"]
        )
        .await?,
        Outcome::Done
    );
    assert_eq!(
        install
            .machines(&[
                "set",
                "other-2",
                "--model",
                "example/serial-astm",
                "--serial-port",
                "COM4",
                "--lab-app",
                "demo"
            ])
            .await?,
        Outcome::Done
    );
    assert_eq!(site_peers(&install, "other-2")?, None);
    Ok(())
}

// ADR 0026 decision 2: `machines list` says which devices a listening
// machine accepts, or warns that it accepts any device with the command
// that limits it; machines on other links say nothing about devices.
#[tokio::test(flavor = "multi_thread")]
async fn list_says_which_devices_each_machine_accepts() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    for (id, port, extra) in [
        ("guarded-2", "5600", &["--allowed-peer", "192.168.1.50"][..]),
        ("open-3", "5601", &[][..]),
    ] {
        assert_eq!(install.machines(&["add", id]).await?, Outcome::Done);
        assert_eq!(
            set_listening(&install, id, port, extra).await?,
            Outcome::Done
        );
    }
    let installation = super::Installation::load(&install.config, install.seams())?;
    let rows = machines::settings_rows(&installation)?;
    let line = |id: &str| {
        rows.iter()
            .find(|row| row.id == id)
            .and_then(machines::devices_line)
    };
    assert_eq!(
        line("guarded-2").as_deref(),
        Some("  Devices: only 192.168.1.50")
    );
    assert_eq!(
        line("open-3").as_deref(),
        Some(
            "  Devices: warning - accepts connections from any device. Accept only the analyzer with: analyte machines set open-3 --allowed-peer <IP>"
        )
    );
    assert_eq!(line("cobas-1"), None);
    Ok(())
}

// ADR 0002: the effective settings include explicit local overrides.
#[tokio::test(flavor = "multi_thread")]
async fn list_describes_the_connection_loaded_with_overrides() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    std::fs::write(
        install.root.join("serial.override.toml"),
        "schema_version = 1\nendpoint = 'COM99'\n",
    )?;
    let program = std::fs::read_to_string(&install.config)?;
    let definition_line = "definition = \"machines/example/serial-astm/machine.toml\"";
    assert!(program.contains(definition_line), "{program}");
    std::fs::write(
        &install.config,
        program.rewrite(
            definition_line,
            &format!("{definition_line}\noverrides = 'serial.override.toml'"),
        ),
    )?;
    let installation = super::Installation::load(&install.config, install.seams())?;
    let machines = machines::settings_rows(&installation)?;
    let machine = machines
        .iter()
        .find(|row| row.id == "cobas-1")
        .ok_or("missing machine")?;
    assert_eq!(
        serde_json::to_value(machine)?["connection"],
        "endpoint = COM99"
    );
    Ok(())
}

// ADR 0026 decision 2: `machines test` of a listening machine names the
// device that connected; in a script it only says the command that accepts
// only that device, and writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn test_names_the_device_and_a_script_changes_nothing() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    assert_eq!(install.machines(&["add", "hl7-2"]).await?, Outcome::Done);
    let port = free_port()?.to_string();
    assert_eq!(
        set_listening(&install, "hl7-2", &port, &[]).await?,
        Outcome::Done
    );
    let before = install.settings()?;
    let address = format!("127.0.0.1:{port}");
    let device = async {
        for _ in 0..100 {
            if let Ok(mut socket) = tokio::net::TcpStream::connect(&address).await {
                tokio::io::AsyncWriteExt::write_all(&mut socket, b"\x0bMSH|").await?;
                tokio::time::sleep(Duration::from_millis(1500)).await;
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err("the test never listened".into())
    };
    let (outcome, device) = tokio::join!(
        install.machines(&["test", "hl7-2", "--seconds", "2"]),
        device
    );
    device?;
    assert!(
        matches!(outcome, Ok(Outcome::Done | Outcome::NotSeen)),
        "{outcome:?}"
    );
    assert_eq!(
        install.settings()?,
        before,
        "a script never saves the device"
    );
    Ok(())
}

/// The data folder of `install` with a new admin key, as the engine needs it.
fn admin_key(install: &Install) -> Result<(PathBuf, String), Error> {
    let data = install.root.join("data");
    std::fs::create_dir_all(&data)?;
    // The engine refuses a data directory others can read (Linux 0700).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
    }
    let key = crate::admin::rotate_token(&data)?;
    Ok((data, key))
}

// ADR 0028: while the engine runs, `lab-apps add` and `lab-apps keys` hand the
// lab app and its keys to it after it proved itself; the engine stores them in
// the locked file through the Lab apps page's own write and records the audit
// row under the command's actor. The keys are in no other file and no row.
#[tokio::test(flavor = "multi_thread")]
async fn keys_reach_the_locked_file_through_the_running_engine() -> Result<(), Error> {
    let install = install_on(free_port()?)?;
    let (data, _) = admin_key(&install)?;
    let program = Program::load(&install.config)?;
    let store = Arc::new(crate::open_system_store(&program).await?);
    let server = crate::admin::prepare(
        &program,
        store.clone(),
        crate::runtime::hooks::ServiceMode::Production,
        &crate::admin::Draining::new(),
        Duration::from_secs(30),
    )
    .await?
    .ok_or("no admin address")?;
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let owner = tokio::spawn(server.run(stopped));
    let demo = ["add", "demo", "--address", "https://lab.example.invalid"];
    let added = install.lab_apps(&demo, [API_KEY, SIGNING_KEY]).await;
    let replaced = "synthetic-replaced-api-key-4QW";
    let keys = install.lab_apps(&["keys", "demo"], [replaced, ""]).await;
    stop.send_replace(true);
    owner.await??;
    let mut store = Arc::into_inner(store).ok_or("the store is still shared")?;
    store.shutdown(crate::deadline()?).await?;
    assert_eq!(added?, Outcome::Done);
    assert_eq!(keys?, Outcome::Done);
    let stored = Locked::new(&data).read()?;
    assert_eq!(stored.value("demo-api"), Some(replaced));
    assert_eq!(stored.value("demo-signing"), Some(SIGNING_KEY));
    for (name, text) in install.settings()? {
        for key in [API_KEY, SIGNING_KEY, replaced] {
            assert!(!text.contains(key), "{name}");
        }
    }
    let audits = install.audits().await?;
    let made: Vec<(&str, &str, &str)> = audits
        .iter()
        .map(|(actor, action, target, _)| (actor.as_str(), action.as_str(), target.as_str()))
        .collect();
    assert_eq!(
        made,
        [
            ("it-test", "lab-app-add", "demo"),
            ("it-test", "lab-app-save", "demo")
        ]
    );
    let reasons: Vec<&str> = audits.iter().map(|(.., reason)| reason.as_str()).collect();
    assert!(
        reasons.iter().all(|r| r.starts_with("command line ")),
        "{reasons:?}"
    );
    assert!(
        reasons
            .last()
            .is_some_and(|r| r.contains("keys replaced: demo-api")),
        "{reasons:?}"
    );
    for key in [API_KEY, SIGNING_KEY, replaced] {
        assert!(reasons.iter().all(|r| !r.contains(key)));
    }
    Ok(())
}

// ADR 0028 §2 (threat: a local listener on the admin port while the service
// is stopped): `lab-apps keys` and `lab-apps add` send the keys only to a
// listener that proves it holds the admin key. One that does not hears the
// challenge and nothing else: no admin key, no Authorization header, no lab app
// key, no write. The command does not fall back to writing the file itself.
#[tokio::test(flavor = "multi_thread")]
async fn an_unproven_listener_never_receives_the_keys() -> Result<(), Error> {
    let wrong = format!("{{\"proof\":\"{}\"}}", "00".repeat(32));
    let answer = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{wrong}",
        wrong.len()
    );
    let (port, heard, owner) = impostor(Some(answer)).await?;
    let install = install_on(port)?;
    let (data, key) = admin_key(&install)?;
    std::fs::write(
        install.root.join("lab-apps.toml"),
        "schema_version = 1\n[[lab_apps]]\nname = 'demo'\nbase_url = 'https://lab.example.invalid'\napi_key_name = 'demo-api'\nsigning_key_name = 'demo-signing'\n",
    )?;
    let before = install.settings()?;
    let keys = install
        .lab_apps(&["keys", "demo"], [API_KEY, SIGNING_KEY])
        .await;
    let add = install
        .lab_apps(
            &["add", "other", "--address", "https://other.example.invalid"],
            [API_KEY, SIGNING_KEY],
        )
        .await;
    owner.abort();
    for result in [keys, add] {
        match result {
            Err(Failure::EngineUnreachable(why)) => {
                assert!(
                    why.to_string()
                        .contains("could not prove it is this Analyte engine"),
                    "{why}"
                );
            }
            Err(other) => return Err(format!("refused for another reason: {other}").into()),
            Ok(outcome) => return Err(format!("not refused: {outcome:?}").into()),
        }
    }
    let heard = String::from_utf8_lossy(&heard.lock().map_err(|_| "poisoned")?).into_owned();
    assert!(heard.contains("GET /proof?nonce="), "{heard}");
    assert!(
        !heard.to_ascii_lowercase().contains("authorization"),
        "{heard}"
    );
    for secret in [key.as_str(), API_KEY, SIGNING_KEY] {
        assert!(!heard.contains(secret), "a key was sent");
    }
    assert!(!heard.contains("/command/"), "{heard}");
    assert_eq!(install.settings()?, before);
    assert_eq!(Locked::new(&data).read()?.value("demo-api"), None);
    Ok(())
}

// ADR 0028 §5: with the engine stopped, the command stores the keys itself
// when this account may write the locked file (the tests above and
// `lab_apps_refuse_as_the_page_does_and_keep_what_is_not_given`); when it may
// not, the refusal says to start the engine and run the command again.
#[test]
fn a_stopped_engine_says_to_start_it() {
    let failure = Failure::EngineStopped(
        crate::installation::credentials::CredentialError::InvalidValue("demo-api".into()),
    );
    assert_eq!(
        failure.advice(),
        Some(
            "Start the engine with Start-Service Analyte in an administrator PowerShell, then run this command again."
        )
    );
    assert!(!failure.after_writing());
}

/// The `[serial]` table of machine `id`'s site file; none when it has none.
fn site_serial(install: &Install, id: &str) -> Result<Option<toml::Value>, Error> {
    let site: toml::Table =
        std::fs::read_to_string(install.root.join(format!("{id}.site.toml")))?.parse()?;
    Ok(site.get("serial").cloned())
}

/// The serial example definition without its flow control suggestion.
fn without_flow_suggestion(install: &Install) -> Result<(), Error> {
    std::fs::write(
        install
            .root
            .join("machines/example/serial-astm/machine.toml"),
        include_str!("../../../../../core/config/examples/astm.toml")
            .rewrite("flow_control = \"none\"\n", ""),
    )?;
    Ok(())
}

// ADR 0031 decision 6, ADR 0027 (one code path): `machines set --baud ...`
// writes the same [serial] table as step 2's Save with the same choices, keeps
// the values not given, names the cable in the audit row and changes the
// settings version; `machines list` shows what differs.
#[tokio::test(flavor = "multi_thread")]
async fn cable_options_save_what_step_two_saves() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let version = |install: &Install| -> Result<[u8; 32], Error> {
        Ok(Program::load(&install.config)?
            .machines()
            .first()
            .ok_or("machine missing")?
            .snapshot()
            .digest())
    };
    let suggested = toml::Value::Table(
        "baud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n"
            .parse()?,
    );
    assert_eq!(
        site_serial(&install, "cobas-1")?,
        Some(suggested),
        "set writes every value, the suggestion where none is given"
    );
    let before = version(&install)?;
    let set = ["set", "cobas-1", "--baud", "19200", "--parity", "even"];
    assert_eq!(install.machines(&set).await?, Outcome::Done);
    assert_eq!(
        site_serial(&install, "cobas-1")?,
        Some(toml::Value::Table(
            "baud = 19200\ndata_bits = 8\nparity = \"even\"\nstop_bits = 1\nflow_control = \"none\"\n"
                .parse()?
        ))
    );
    assert_ne!(version(&install)?, before);
    // A later change keeps the saved cable settings, as every saved choice is kept.
    assert_eq!(
        install
            .machines(&["set", "cobas-1", "--stop-bits", "2"])
            .await?,
        Outcome::Done
    );
    let serial = site_serial(&install, "cobas-1")?.ok_or("no [serial]")?;
    assert_eq!(serial.get("baud"), Some(&toml::Value::Integer(19200)));
    assert_eq!(serial.get("stop_bits"), Some(&toml::Value::Integer(2)));
    let audits = install.audits().await?;
    assert!(
        audits
            .iter()
            .any(|(_, action, _, reason)| action == "setup-save"
                && reason.contains("cable 19200 baud, 8 data bits, even parity, 2 stop bits")),
        "{audits:?}"
    );
    let installation = super::Installation::load(&install.config, install.seams())?;
    let rows = machines::settings_rows(&installation)?;
    let cable = rows
        .iter()
        .find(|row| row.id == "cobas-1")
        .and_then(|row| row.cable.as_ref())
        .ok_or("no cable row")?;
    assert_eq!(
        cable.words,
        concat!(
            "19200 baud, 8 data bits, even parity, 2 stop bits, no flow control; ",
            "differs from the manual's suggestion: baud rate 19200 (the manual suggests 9600), ",
            "parity even (the manual suggests none), stop bits 2 (the manual suggests 1)"
        )
    );
    Ok(())
}

// ADR 0031 decision 4: a value off the list is refused in the page's words
// and changes no file, on set and on add.
#[tokio::test(flavor = "multi_thread")]
async fn cable_options_refuse_values_off_the_list() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let before = install.settings()?;
    let error = refusal(
        install
            .machines(&["set", "cobas-1", "--baud", "14400"])
            .await,
    )?;
    assert!(
        error.to_string().contains("use a standard baud rate"),
        "{error}"
    );
    let error = refusal(
        install
            .machines(&["set", "cobas-1", "--parity", "mark"])
            .await,
    )?;
    assert!(
        matches!(error, SetupError::Connection(words) if words.contains("none, odd or even")),
        "{error}"
    );
    let error = refusal(
        install
            .machines(&["add", "cobas-2", "--data-bits", "9"])
            .await,
    )?;
    assert!(
        error.to_string().contains("use 5 through 8 data bits"),
        "{error}"
    );
    assert_eq!(install.settings()?, before);
    // A network model takes none; given anyway, it is refused, not ignored.
    assert_eq!(install.machines(&["add", "hl7-2"]).await?, Outcome::Done);
    let network = [
        "set",
        "hl7-2",
        "--model",
        "example/network-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        "5600",
        "--lab-app",
        "demo",
        "--baud",
        "9600",
    ];
    let error = refusal(install.machines(&network).await)?;
    assert!(
        error
            .to_string()
            .contains("only a machine on a serial cable has cable settings"),
        "{error}"
    );
    Ok(())
}

// ADR 0031 decisions 3 and 6: a setting the model does not suggest must be
// chosen; `add` keeps the chosen values for the set that follows.
#[tokio::test(flavor = "multi_thread")]
async fn a_cable_setting_without_suggestion_must_be_chosen() -> Result<(), Error> {
    let install = install()?;
    without_flow_suggestion(&install)?;
    let demo = ["add", "demo", "--address", "https://lab.example.invalid"];
    assert_eq!(
        install.lab_apps(&demo, [API_KEY, SIGNING_KEY]).await?,
        Outcome::Done
    );
    assert_eq!(install.machines(&["add", "cobas-1"]).await?, Outcome::Done);
    let set = [
        "set",
        "cobas-1",
        "--model",
        "example/serial-astm",
        "--usb-serial",
        "A10K3XYZ",
        "--lab-app",
        "demo",
    ];
    let error = refusal(install.machines(&set).await)?;
    assert!(
        matches!(
            error,
            SetupError::Config(analyte_config::Error::MissingSerialSetting {
                setting: analyte_config::SerialSetting::FlowControl,
                ..
            })
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("transport.serial.flow_control: no value"),
        "{error}"
    );
    assert_eq!(
        install
            .machines(&["add", "cobas-2", "--flow-control", "hardware"])
            .await?,
        Outcome::Done
    );
    assert_eq!(
        site_serial(&install, "cobas-2")?,
        Some(toml::Value::Table("flow_control = \"hardware\"\n".parse()?))
    );
    let set = [
        "set",
        "cobas-2",
        "--model",
        "example/serial-astm",
        "--usb-serial",
        "A10K3XYZ",
        "--lab-app",
        "demo",
    ];
    assert_eq!(install.machines(&set).await?, Outcome::Done);
    let serial = site_serial(&install, "cobas-2")?.ok_or("no [serial]")?;
    assert_eq!(
        serial.get("flow_control"),
        Some(&toml::Value::String("hardware".into()))
    );
    assert_eq!(serial.get("baud"), Some(&toml::Value::Integer(9600)));
    Ok(())
}

// ADR 0031 decision 1: `machines test` opens the fake port with the values
// given, not the saved ones, and saves nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_test_command_uses_the_chosen_cable_settings() -> Result<(), Error> {
    let install = install()?;
    install.set_up().await?;
    let before = install.settings()?;
    let test = [
        "test",
        "cobas-1",
        "--seconds",
        "1",
        "--baud",
        "38400",
        "--flow-control",
        "hardware",
    ];
    assert_eq!(install.machines(&test).await?, Outcome::Done);
    let opened = install.opened.lock().map_err(|_| "poisoned")?.clone();
    let settings = opened.last().ok_or("not opened")?;
    for expected in ["baud: 38400", "bits: Eight", "flow: Hardware"] {
        assert!(
            settings.contains(expected),
            "missing {expected}: {settings}"
        );
    }
    assert_eq!(install.settings()?, before);
    Ok(())
}

/// A model with its own choices (ADR 0032): the VIDAS teaching definition
/// whose software version only the lab can choose.
fn with_choices_model(install: &Install) -> Result<(), Error> {
    let folder = install.root.join("machines/example/vidas-choices");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("machine.toml"),
        include_str!("../../../../../core/config/examples/vidas_choices.toml"),
    )?;
    Ok(())
}

/// The `[choices]` table of machine `id`'s site file.
fn site_choices(install: &Install, id: &str) -> Result<Option<toml::Value>, Error> {
    let site: toml::Table =
        std::fs::read_to_string(install.root.join(format!("{id}.site.toml")))?.parse()?;
    Ok(site.get("choices").cloned())
}

/// `machines set vidas-1` on the choices model, listening on loopback, with `extra`.
async fn set_vidas(install: &Install, extra: &[&str]) -> Result<Outcome, Failure> {
    let mut args = vec![
        "set",
        "vidas-1",
        "--model",
        "example/vidas-choices",
        "--address",
        "127.0.0.1",
        "--port",
        "5601",
        "--lab-app",
        "demo",
        "--any-device",
    ];
    args.extend_from_slice(extra);
    install.machines(&args).await
}

// ADR 0032 decisions 3 and 6, ADR 0027 (one code path): `machines set
// --choice` writes the same [choices] table as step 2's Save, refuses a choice
// without a value by name, keeps saved values not given, names the choices in
// the audit row and changes the settings version; `machines list --json` shows
// each choice and which differ from the suggestion.
#[tokio::test(flavor = "multi_thread")]
async fn choice_options_save_what_step_two_saves() -> Result<(), Error> {
    let install = install()?;
    with_choices_model(&install)?;
    let demo = ["add", "demo", "--address", "https://lab.example.invalid"];
    assert_eq!(
        install.lab_apps(&demo, [API_KEY, SIGNING_KEY]).await?,
        Outcome::Done
    );
    assert_eq!(install.machines(&["add", "vidas-1"]).await?, Outcome::Done);
    let before = install.settings()?;
    let error = refusal(set_vidas(&install, &[]).await)?;
    assert!(
        matches!(
            &error,
            SetupError::Config(analyte_config::Error::MissingChoice { choice, .. })
                if choice.as_str() == "software_version"
        ),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("choices.software_version: no value"),
        "{error}"
    );
    assert_eq!(install.settings()?, before, "a refusal changes nothing");
    assert_eq!(
        set_vidas(&install, &["--choice", "software_version=v2"]).await?,
        Outcome::Done
    );
    assert_eq!(
        site_choices(&install, "vidas-1")?,
        Some(toml::Value::Table(
            "protocol_mode = \"literal\"\nsoftware_version = \"v2\"\n".parse()?
        )),
        "set writes every choice, the suggestion where none is given"
    );
    let version = |install: &Install| -> Result<[u8; 32], Error> {
        Ok(Program::load(&install.config)?
            .machines()
            .first()
            .ok_or("machine missing")?
            .snapshot()
            .digest())
    };
    let first = version(&install)?;
    let set = ["set", "vidas-1", "--choice", "protocol_mode=alternate"];
    assert_eq!(install.machines(&set).await?, Outcome::Done);
    assert_eq!(
        site_choices(&install, "vidas-1")?,
        Some(toml::Value::Table(
            "protocol_mode = \"alternate\"\nsoftware_version = \"v2\"\n".parse()?
        )),
        "the saved software version is kept"
    );
    assert_ne!(version(&install)?, first);
    let audits = install.audits().await?;
    assert!(
        audits
            .iter()
            .any(|(_, action, _, reason)| action == "setup-save"
                && reason
                    .contains("analyzer settings Protocol on the host link screen: Alternate")),
        "{audits:?}"
    );
    let installation = super::Installation::load(&install.config, install.seams())?;
    let rows = machines::settings_rows(&installation)?;
    let row = rows
        .iter()
        .find(|row| row.id == "vidas-1")
        .ok_or("no row")?;
    let json = serde_json::to_value(row)?;
    assert_eq!(
        json.get("choices"),
        Some(&serde_json::json!([
            {"name": "protocol_mode", "kind": "mode", "value": "alternate",
             "suggested": "literal", "selected": true, "differs_from_suggestion": true},
            {"name": "software_version", "kind": "software_version", "value": "v2",
             "suggested": null, "selected": true, "differs_from_suggestion": true}
        ]))
    );
    assert_eq!(
        row.choices_words.as_deref(),
        Some(concat!(
            "Protocol on the host link screen: Alternate (acknowledge after ETX) ",
            "(the manual suggests Literal (acknowledge after the checksum)); ",
            "Software version on the About screen: Version 2 or later (sample IDs up to 17 ",
            "characters) (the machine definition suggests none)"
        ))
    );
    Ok(())
}

// ADR 0032 decision 3: an undeclared choice, a value off the declared list or
// a malformed --choice is refused in the page's words and changes no file.
#[tokio::test(flavor = "multi_thread")]
async fn choice_options_refuse_what_the_model_does_not_offer() -> Result<(), Error> {
    let install = install()?;
    with_choices_model(&install)?;
    install.set_up().await?;
    assert_eq!(install.machines(&["add", "vidas-1"]).await?, Outcome::Done);
    let before = install.settings()?;
    let unknown = refusal(set_vidas(&install, &["--choice", "speed=fast"]).await)?;
    assert!(
        matches!(&unknown, SetupError::UnknownChoice(name) if name == "speed"),
        "{unknown}"
    );
    let value = refusal(set_vidas(&install, &["--choice", "software_version=v9"]).await)?;
    assert!(
        value
            .to_string()
            .contains("choose software_version from its list: v1, v2"),
        "{value}"
    );
    // The serial model declares no choices, so any --choice is refused there.
    let serial = ["set", "cobas-1", "--choice", "protocol_mode=literal"];
    let other = refusal(install.machines(&serial).await)?;
    assert!(matches!(other, SetupError::UnknownChoice(_)), "{other}");
    let malformed = install
        .machines(&["set", "vidas-1", "--choice", "software_version"])
        .await;
    assert!(
        matches!(&malformed, Err(Failure::Console(error)) if error.to_string().contains("NAME=VALUE")),
        "{malformed:?}"
    );
    assert_eq!(install.settings()?, before);
    Ok(())
}

// ADR 0034 decisions 1, 3 and 10: `machines set --record-only` writes what
// the Setup page's record-only save writes (the model's record.toml as the
// definition, [recording] and no lab app in the site file), a model not ready
// yet records only, and leaving record only needs a ready model and a lab app.
#[tokio::test(flavor = "multi_thread")]
async fn record_only_is_set_and_left_through_the_save_step() -> Result<(), Error> {
    let install = install()?;
    let blocked = install.root.join("machines/roche/cobas-e411");
    std::fs::write(
        blocked.join(analyte_config::RECORD_FILE),
        "recording_schema_version = 1\nprotocol = \"astm\"\n[transport]\nkind = \"serial\"\n[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\n",
    )?;
    assert_eq!(install.machines(&["add", "cobas-1"]).await?, Outcome::Done);
    let normal = [
        "set",
        "cobas-1",
        "--model",
        "roche/cobas-e411",
        "--usb-serial",
        "A10K3XYZ",
        "--flow-control",
        "none",
    ];
    let error = refusal(install.machines(&normal).await)?;
    assert!(matches!(error, SetupError::RecordOnlyModel(_)), "{error}");
    assert!(error.to_string().contains("Record only"), "{error}");
    let record = [
        "set",
        "cobas-1",
        "--model",
        "roche/cobas-e411",
        "--record-only",
        "--usb-serial",
        "A10K3XYZ",
        "--flow-control",
        "none",
    ];
    assert_eq!(install.machines(&record).await?, Outcome::Done);
    let site: toml::Table =
        std::fs::read_to_string(install.root.join("cobas-1.site.toml"))?.parse()?;
    assert!(site.contains_key("recording"), "{site}");
    assert!(!site.contains_key("lab_app") && !site.contains_key("destination"));
    let program_text = std::fs::read_to_string(&install.config)?;
    assert!(
        program_text.contains("definition = \"machines/roche/cobas-e411/record.toml\""),
        "{program_text}"
    );
    let program = Program::load(&install.config)?;
    let machine = program.machines().first().ok_or("machine")?;
    assert!(machine.snapshot().effective().records_only());
    assert_eq!(machine.model.as_str(), "roche/cobas-e411");
    assert!(machine.capture, "a record-only machine always captures");
    // A lab app cannot be named beside record only.
    assert!(
        install
            .machines(&["set", "cobas-1", "--record-only", "--lab-app", "demo"])
            .await
            .is_err()
    );
    // Leaving record only: the model must be ready, and a lab app is chosen.
    let demo = ["add", "demo", "--address", "https://lab.example.invalid"];
    assert_eq!(
        install.lab_apps(&demo, [API_KEY, SIGNING_KEY]).await?,
        Outcome::Done
    );
    let error = refusal(
        install
            .machines(&["set", "cobas-1", "--connect-normally", "--lab-app", "demo"])
            .await,
    )?;
    assert!(matches!(error, SetupError::RecordOnlyModel(_)), "{error}");
    let ready = [
        "set",
        "cobas-1",
        "--model",
        "example/serial-astm",
        "--connect-normally",
        "--lab-app",
        "demo",
    ];
    assert_eq!(install.machines(&ready).await?, Outcome::Done);
    let site: toml::Table =
        std::fs::read_to_string(install.root.join("cobas-1.site.toml"))?.parse()?;
    assert!(!site.contains_key("recording"), "{site}");
    assert_eq!(
        site.get("lab_app").and_then(toml::Value::as_str),
        Some("demo")
    );
    let program = Program::load(&install.config)?;
    assert!(
        !program
            .machines()
            .first()
            .ok_or("machine")?
            .snapshot()
            .effective()
            .records_only()
    );
    // Every save is audited with what it chose.
    let audits = install.audits().await?;
    assert!(
        audits
            .iter()
            .any(|(_, _, target, reason)| target == "cobas-1"
                && reason.contains("record only, no lab app")),
        "{audits:?}"
    );
    Ok(())
}

// ADR 0034 with ADR 0015; cobas e 411 HIM v2.3 Host communication setting
// (p. 7) and section 5 (p. 14): `machines set --analyzer-mode` writes the
// protocol type the record-only fixture asks, so packing follows the lab's
// answer, and a value the model does not offer changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn record_only_takes_the_cobas_protocol_type_from_the_command_line() -> Result<(), Error> {
    let install = install()?;
    let blocked = install.root.join("machines/roche/cobas-e411");
    std::fs::write(
        blocked.join(analyte_config::RECORD_FILE),
        analyte_fixtures::ASTM_PACKED_RECORD_ONLY,
    )?;
    assert_eq!(install.machines(&["add", "cobas-1"]).await?, Outcome::Done);
    let packed = |install: &Install| -> Result<(bool, Option<String>), Error> {
        let site: toml::Table =
            std::fs::read_to_string(install.root.join("cobas-1.site.toml"))?.parse()?;
        let program = Program::load(&install.config)?;
        let machine = program.machines().first().ok_or("machine")?;
        let analyte_config::Stack::Record(stack) = machine.snapshot().effective().stack() else {
            return Err("record-only stack".into());
        };
        let chosen = site
            .get("recording")
            .and_then(|recording| recording.get("analyzer_mode"))
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
        Ok((stack.packed_records(), chosen))
    };
    let record = [
        "set",
        "cobas-1",
        "--model",
        "roche/cobas-e411",
        "--record-only",
        "--usb-serial",
        "A10K3XYZ",
        "--flow-control",
        "none",
        "--analyzer-mode",
        "cobas",
    ];
    assert_eq!(install.machines(&record).await?, Outcome::Done);
    assert_eq!(packed(&install)?, (true, Some("cobas".to_owned())));
    let elecsys = ["set", "cobas-1", "--analyzer-mode", "elecsys"];
    assert_eq!(install.machines(&elecsys).await?, Outcome::Done);
    assert_eq!(packed(&install)?, (false, Some("elecsys".to_owned())));
    let before = install.settings()?;
    let unoffered = install
        .machines(&["set", "cobas-1", "--analyzer-mode", "hitachi"])
        .await;
    assert!(unoffered.is_err(), "{unoffered:?}");
    assert_eq!(install.settings()?, before);
    Ok(())
}

// ADR 0034: record-only detection uses the same size bound as site loading.
#[tokio::test]
async fn machine_commands_report_an_oversized_record_only_site() -> Result<(), Error> {
    let install = install()?;
    install.machines(&["add", "cobas-1"]).await?;
    let site = install.root.join("cobas-1.site.toml");
    let contents = format!(
        "[recording]\n#{}",
        "x".repeat(analyte_config::MAX_FILE_BYTES)
    );
    std::fs::write(&site, contents)?;
    for outcome in [
        install.machines(&["list"]).await,
        install.machines(&["set", "cobas-1", "--dry-run"]).await,
    ] {
        let error = outcome
            .err()
            .ok_or("command ignored an oversized site file")?;
        assert!(matches!(
            error,
            Failure::Setup(SetupError::SiteRead {
                path,
                source,
            }) if path == site
                && matches!(*source, crate::installation::config::Error::ConfigurationTooLarge)
        ));
    }
    Ok(())
}

// AGENTS.md: unreadable site settings cannot be treated as empty settings.
#[tokio::test]
async fn machine_and_lab_app_commands_report_an_unreadable_site() -> Result<(), Error> {
    let install = install()?;
    install.machines(&["add", "cobas-1"]).await?;
    let site = install.root.join("cobas-1.site.toml");
    std::fs::remove_file(&site)?;
    for outcome in [
        install.machines(&["list"]).await,
        install.machines(&["set", "cobas-1", "--dry-run"]).await,
        install.lab_apps(&["list"], [API_KEY, SIGNING_KEY]).await,
    ] {
        let error = outcome.err().ok_or("command ignored a missing site file")?;
        assert!(matches!(
            error,
            Failure::Setup(SetupError::SiteRead { path, .. }) if path == site
        ));
    }
    Ok(())
}
