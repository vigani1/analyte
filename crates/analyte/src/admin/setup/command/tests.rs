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
            // Validates on its own, but its frame bound is above the store's
            // record bound, so only the whole program's reload refuses it.
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
    })
}

impl Install {
    fn seams(&self) -> Seams {
        let written = self.written.clone();
        Seams {
            definitions: Some(self.root.join("machines")),
            ports: Arc::new(|| {
                Ok(vec![analyte_transport::serial::Listed {
                    name: "COM12".into(),
                    usb_serial: Some("A10K3XYZ".into()),
                    description: Some("FT232R USB UART".into()),
                }])
            }),
            open: Arc::new(move |_, _| {
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
// machine already uses, and a change no reload accepts (files put back).
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
    let too_big = &[
        "set",
        "hl7-2",
        "--model",
        "example/big-hl7",
        "--address",
        "127.0.0.1",
        "--port",
        "5601",
    ];
    let error = refusal(refused(too_big).await)?;
    assert!(matches!(error, SetupError::RolledBack(_)), "{error}");
    assert!(
        error.to_string().contains("previous files were put back"),
        "{error}"
    );
    assert_eq!(install.settings()?, before);
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
                    why.contains("could not prove it is this Analyte engine"),
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
