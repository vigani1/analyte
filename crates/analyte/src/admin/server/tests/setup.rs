//! Tests the setup page: definitions, detected ports, saving with backups, refusals, keys and the listen test.
use super::pages::{csrf, encode, login};
use super::*;
use std::sync::Mutex;

/// Synthetic key values; neither may ever appear in a page, audit row or log.
const API_KEY: &str = "sensitive-setup-api-key-5XK";
const SIGNING_KEY: &str = analyte_fixtures::SECRET;
/// Synthetic bytes a fake analyzer sends during the listen test.
const MARKER: &[u8] = b"SYN-MARK-731";

struct Fixture {
    _directory: tempfile::TempDir,
    config: std::path::PathBuf,
    site: std::path::PathBuf,
    data: std::path::PathBuf,
    store: Arc<analyte_store::Handle>,
    host: String,
    cookie: String,
    owner: tokio::task::JoinHandle<Result<(), crate::admin::Error>>,
    _stop: watch::Sender<bool>,
    written: Arc<Mutex<Vec<u8>>>,
}

/// A fake serial port: sends `MARKER` once, then only times out; records writes.
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

fn seams(root: &std::path::Path, written: Arc<Mutex<Vec<u8>>>) -> crate::admin::setup::Seams {
    use analyte_transport::serial::Listed;
    crate::admin::setup::Seams {
        definitions: Some(root.join("machines")),
        // #251's listing seam: one FTDI adapter with a factory serial number and
        // one adapter whose number Windows made up.
        ports: Arc::new(|| {
            Ok(vec![
                Listed {
                    name: "COM12".into(),
                    usb_serial: Some("A10K3XYZ".into()),
                    description: Some("FT232R USB UART".into()),
                },
                Listed {
                    name: "COM3".into(),
                    usb_serial: Some("6&2A9C1F&0&1".into()),
                    description: Some("USB-SERIAL CH340".into()),
                },
            ])
        }),
        open: Arc::new(move |_, _| {
            Ok(Box::new(FakePort {
                input: MARKER.to_vec(),
                written: written.clone(),
            }) as Box<dyn analyte_transport::serial::Port>)
        }),
        limits: crate::admin::setup::Limits {
            duration: Duration::from_secs(20),
            bytes: 1024,
        },
    }
}

async fn start() -> Result<Fixture, Error> {
    start_with(
        "[[machines]]\nid = 'synthetic'\ndefinition = 'machines/example/serial-astm/machine.toml'\nsite = 'site.toml'\n",
        &include_str!("../../../../../core/config/examples/site.toml")
            .rewrite("endpoint = \"127.0.0.1:5600\"", "usb_serial = \"A10K3XYZ\""),
    )
    .await
}

/// A fixture whose program file lists `machines` and whose `site.toml` holds `site_text`.
async fn start_with(machines: &str, site_text: &str) -> Result<Fixture, Error> {
    let directory = crate::admin::private_tempdir()?;
    let root = directory.path();
    let serial = root.join("machines/example/serial-astm");
    std::fs::create_dir_all(&serial)?;
    std::fs::write(
        serial.join("machine.toml"),
        include_str!("../../../../../core/config/examples/astm.toml"),
    )?;
    let network = root.join("machines/example/network-hl7");
    std::fs::create_dir_all(&network)?;
    std::fs::write(
        network.join("machine.toml"),
        include_str!("../../../../../core/config/examples/hl7.toml"),
    )?;
    let blocked = root.join("machines/roche/cobas-e411");
    std::fs::create_dir_all(&blocked)?;
    std::fs::write(blocked.join("README.md"), "A blocked machine definition.\n")?;
    let site = root.join("site.toml");
    std::fs::write(&site, site_text)?;
    let config = root.join("program.toml");
    std::fs::write(
        &config,
        analyte_fixtures::program_with_limits(
            machines,
            &analyte_store::Limits {
                reserve_bytes: 1,
                database_bytes: 100_000_000,
                record_bytes: 2_097_152,
                page_rows: 100,
                busy_timeout: Duration::from_millis(100),
            },
        ),
    )?;
    let program = Program::load(&config)?;
    let data = program.data_directory().to_owned();
    let store = Arc::new(
        crate::runtime::service::open_store_with_deadline(
            &program,
            Box::new(Space),
            fixture_deadline()?,
        )
        .await
        .map_err(|error| format!("setup fixture migration: {error}"))?,
    );
    let bearer = crate::admin::rotate_token(&data)?;
    let written = Arc::new(Mutex::new(Vec::new()));
    let setup = crate::admin::setup::Setup::new(&program, seams(root, written.clone()));
    let restart = setup.restart_requested();
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let host = socket.local_addr()?.to_string();
    let (released, release) = tokio::sync::oneshot::channel();
    let state = Arc::new(Context {
        alert_endpoints: vec![],
        retention: crate::runtime::retention::policies(&program),
        config_path: config.clone(),
        token: Token::load(&data)?,
        sessions: Sessions::default(),
        origin: format!("http://{host}"),
        host: host.clone(),
        store: store.clone(),
        machines: Vec::new(),
        destinations: Vec::new(),
        directory: data.clone(),
        reserve: 1,
        setup,
        _released: released,
    });
    let server = Server {
        listener: Listener {
            socket,
            slots: Arc::new(Semaphore::new(32)),
        },
        app: router(state),
        restart,
        released: release,
        owners: BTreeMap::new(),
    };
    let (stop, stopped) = watch::channel(false);
    let owner = tokio::spawn(server.run(stopped));
    let cookie = login(&host, &bearer).await?;
    Ok(Fixture {
        _directory: directory,
        config,
        site,
        data,
        store,
        host,
        cookie,
        owner,
        _stop: stop,
        written,
    })
}

impl Fixture {
    async fn get(&self, path: &str) -> Result<String, Error> {
        request(
            &self.host,
            path,
            &format!("Cookie: {}\r\n", self.cookie),
            None,
        )
        .await
    }

    async fn post(&self, path: &str, body: &str) -> Result<String, Error> {
        request(
            &self.host,
            path,
            &format!(
                "Cookie: {}\r\nContent-Type: application/x-www-form-urlencoded\r\n",
                self.cookie
            ),
            Some(body),
        )
        .await
    }

    /// The setup form's fields with `fields` replacing or adding values.
    async fn form(&self, fields: &[(&str, &str)]) -> Result<String, Error> {
        let page = self.get("/setup/synthetic").await?;
        let mut values = vec![
            ("csrf", csrf(&page)?),
            ("definition", "example/serial-astm".to_owned()),
            ("port_name", String::new()),
            ("base_url", "https://lab.example.invalid/v1".to_owned()),
            ("destination", "demo".to_owned()),
            ("api_key", String::new()),
            ("signing_key", String::new()),
        ];
        for (name, value) in fields {
            values.retain(|(existing, _)| existing != name);
            values.push((name, (*value).to_owned()));
        }
        Ok(values
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect::<Vec<_>>()
            .join("&"))
    }

    fn backups(&self) -> Result<Vec<std::path::PathBuf>, Error> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(self.site.parent().ok_or("site folder")?)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "bak") {
                found.push(path);
            }
        }
        Ok(found)
    }
}

// Owner request 2026-11-18 (setup page); machines/README.md: the page lists the
// shipped definitions with name and evidence, blocked ones as blocked, and the
// detected ports with adapter name and USB serial number.
#[tokio::test(flavor = "multi_thread")]
async fn page_lists_definitions_and_detected_adapters() -> Result<(), Error> {
    let fixture = start().await?;
    let index = fixture.get("/setup").await?;
    assert!(index.starts_with("HTTP/1.1 200"), "{index}");
    for expected in [
        "example/serial-astm",
        "generic-synthetic",
        "documented",
        "roche/cobas-e411",
        "Blocked: facts or recordings are missing",
        "href=\"/setup/synthetic\"",
    ] {
        assert!(index.contains(expected), "missing {expected}");
    }
    let machine = fixture.get("/setup/synthetic").await?;
    assert!(machine.starts_with("HTTP/1.1 200"), "{machine}");
    for expected in [
        "value=\"usb:A10K3XYZ\"> COM12 — FT232R USB UART — saved as serial number A10K3XYZ",
        "value=\"name:COM3\"> COM3 — USB-SERIAL CH340 — no serial number; saved as the port name",
        "value=\"roche/cobas-e411\" disabled",
        "Analyte saves that number",
        "may contain patient data",
    ] {
        assert!(machine.contains(expected), "missing {expected}");
    }
    let network = fixture
        .get("/setup/synthetic?definition=example/network-hl7")
        .await?;
    assert!(network.contains("name=\"host\""), "{network}");
    assert!(!network.contains("name=\"port\""));
    Ok(())
}

// Owner request 2026-11-18 (installers): the installed starter lists a machine
// without a definition; the page shows it as not set up, and a save adds the
// definition line, so the program then runs it with the stored keys.
#[tokio::test(flavor = "multi_thread")]
async fn waiting_machine_is_set_up_from_the_starter() -> Result<(), Error> {
    let fixture = start_with(
        "[admin]\nport = 8080\n[[machines]]\nid = 'machine-1'\nsite = 'site.toml'\n",
        include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml"),
    )
    .await?;
    let before = Program::load(&fixture.config)?;
    assert!(before.machines().is_empty());
    assert_eq!(before.awaiting_setup().len(), 1);
    let index = fixture.get("/setup").await?;
    assert!(
        index.contains("<td>machine-1</td><td>Not set up yet</td>"),
        "{index}"
    );
    let first = fixture.get("/setup/machine-1").await?;
    assert!(first.starts_with("HTTP/1.1 200"), "{first}");
    assert!(first.contains("This machine is not set up yet"));
    assert!(!first.contains("name=\"base_url\""));
    let chosen = fixture
        .get("/setup/machine-1?definition=example/network-hl7")
        .await?;
    assert!(chosen.contains("name=\"host\""), "{chosen}");
    assert!(chosen.contains("lab-app-api: not set"), "{chosen}");
    let body = [
        ("csrf", csrf(&chosen)?),
        ("definition", "example/network-hl7".into()),
        ("host", "127.0.0.1".into()),
        ("tcp_port", "5600".into()),
        ("base_url", "https://lab.example.invalid/v1".into()),
        ("destination", "lab-app".into()),
        ("api_key", API_KEY.into()),
        ("signing_key", SIGNING_KEY.into()),
    ]
    .iter()
    .map(|(name, value)| format!("{name}={}", encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    let saved = fixture.post("/setup/machine-1", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains("Running now <code>none: not set up yet</code>"));
    let program = std::fs::read_to_string(&fixture.config)?;
    assert!(
        program.contains(
            "id = 'machine-1'\ndefinition = \"machines/example/network-hl7/machine.toml\"\nsite = 'site.toml'\n"
        ),
        "{program}"
    );
    let after = Program::load(&fixture.config)?;
    assert!(after.awaiting_setup().is_empty());
    let machine = after.machines().first().ok_or("machine missing")?;
    assert_eq!(machine.id().as_str(), "machine-1");
    let stored = crate::installation::credentials::Locked::new(&fixture.data).read()?;
    assert!(
        crate::installation::secrets::Environment::load_layered(&after, |_| None, &stored).is_ok(),
        "the stored keys must satisfy startup"
    );
    assert!(!saved.contains(API_KEY) && !saved.contains(SIGNING_KEY));
    Ok(())
}

// Owner request 2026-11-18 (setup page): an invalid lab app address is refused
// before any file, backup or key is written.
#[tokio::test(flavor = "multi_thread")]
async fn invalid_address_is_refused_before_anything_is_written() -> Result<(), Error> {
    let fixture = start().await?;
    let before = std::fs::read_to_string(&fixture.site)?;
    let body = fixture
        .form(&[
            ("port", "usb:A10K3XYZ"),
            ("base_url", "http://lab.example.invalid"),
            ("api_key", API_KEY),
        ])
        .await?;
    let response = fixture.post("/setup/synthetic", &body).await?;
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(response.contains("lab app address is not accepted"));
    assert!(!response.contains(API_KEY));
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);
    assert!(fixture.backups()?.is_empty());
    assert!(!fixture.data.join("credentials").exists());
    Ok(())
}

// Owner request 2026-11-18 (setup page, locked file); AGENTS.md rule 10: a save
// writes a site file the program loader (check-config) accepts, keeps the old
// one, stores the keys in the locked file, and no key reaches a page, an audit
// row or a log line.
#[tokio::test(flavor = "multi_thread")]
async fn save_writes_checked_site_with_backup_and_locked_keys() -> Result<(), Error> {
    let logs = crate::runtime::log_capture::init();
    let fixture = start().await?;
    // An adapter without its own serial number is saved by port name.
    let by_name = fixture.form(&[("port", "name:COM3")]).await?;
    let named = fixture.post("/setup/synthetic", &by_name).await?;
    assert!(named.starts_with("HTTP/1.1 200"), "{named}");
    let site = std::fs::read_to_string(&fixture.site)?;
    assert!(site.contains("endpoint = \"COM3\""), "{site}");
    assert!(!site.contains("usb_serial"));
    let before = site;
    let body = fixture
        .form(&[
            ("port", "usb:A10K3XYZ"),
            ("api_key", API_KEY),
            ("signing_key", SIGNING_KEY),
        ])
        .await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains("usb_serial = A10K3XYZ"));
    assert!(saved.contains("Restart now"));
    let program = Program::load(&fixture.config)?;
    let machine = program.machines().first().ok_or("machine missing")?;
    let digest: String = machine
        .snapshot()
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert!(saved.contains(&digest), "saved page lacks the new digest");
    assert_eq!(
        machine.snapshot().effective().site().endpoint,
        analyte_config::Endpoint::UsbSerial(analyte_config::UsbSerial::new("A10K3XYZ".into())?)
    );
    let site = std::fs::read_to_string(&fixture.site)?;
    assert!(site.contains("usb_serial = \"A10K3XYZ\""), "{site}");
    assert!(!site.contains("endpoint"));
    assert!(site.contains("retention = \"keep\""));
    let backups = fixture.backups()?;
    assert_eq!(backups.len(), 2);
    assert!(
        backups
            .iter()
            .any(|path| std::fs::read_to_string(path).is_ok_and(|text| text == before))
    );
    let stored = crate::installation::credentials::Locked::new(&fixture.data).read()?;
    assert_eq!(
        stored.by_variable("ANALYTE_SECRET_DEMO_API").as_deref(),
        Some(API_KEY)
    );
    assert_eq!(
        stored.by_variable("ANALYTE_SECRET_DEMO_SIGNING").as_deref(),
        Some(SIGNING_KEY)
    );
    let file = fixture.data.join("credentials/credentials.toml");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&file)?.permissions().mode() & 0o777,
            0o600
        );
    }
    #[cfg(windows)]
    crate::runtime::service::acl::check(&file, crate::runtime::service::acl::Readers::ServiceOnly)?;
    let again = fixture.get("/setup/synthetic").await?;
    assert!(again.contains("demo-api: set on "), "{again}");
    assert!(again.contains("demo-signing: set on "));
    let audits = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.audits(0)?))
        .await?;
    assert!(
        audits
            .iter()
            .any(|(_, audit)| audit.action.as_str() == "setup-save")
    );
    let logged = String::from_utf8_lossy(&logs.lock().map_err(|_| "log poisoned")?).into_owned();
    for page in [&named, &saved, &again] {
        assert!(!page.contains(API_KEY) && !page.contains(SIGNING_KEY));
    }
    for (_, audit) in &audits {
        let text = format!("{} {}", audit.action.as_str(), audit.reason.as_str());
        assert!(!text.contains(API_KEY) && !text.contains(SIGNING_KEY));
    }
    assert!(!logged.contains(API_KEY) && !logged.contains(SIGNING_KEY));
    Ok(())
}

// Owner request 2026-11-18 (setup page); AGENTS.md rules 7 and 10: the listen
// test shows the bytes on the page only, writes nothing to the port, refuses a
// port the running service owns, and stores and logs nothing.
#[tokio::test(flavor = "multi_thread")]
async fn listen_test_shows_bytes_only_on_the_page() -> Result<(), Error> {
    let logs = crate::runtime::log_capture::init();
    let fixture = start().await?;
    // The running machine names its adapter by serial number; the fake listing
    // puts that adapter on COM12, so COM12 is busy by number and by name.
    for busy in ["usb:A10K3XYZ", "name:COM12"] {
        let form = fixture.form(&[("port", busy)]).await?;
        let refused = fixture.post("/setup/synthetic/listen", &form).await?;
        assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
        assert!(refused.contains("machine synthetic is using this connection"));
    }
    let body = fixture.form(&[("port", "name:COM3")]).await?;
    let started = fixture.post("/setup/synthetic/listen", &body).await?;
    assert!(started.starts_with("HTTP/1.1 303"), "{started}");
    let mut view = String::new();
    for _ in 0..100 {
        view = fixture.get("/setup/listen").await?;
        if view.contains("SYN-MARK-731") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        view.contains("53 59 4e 2d 4d 41 52 4b 2d 37 33 31"),
        "{view}"
    );
    assert!(view.contains("http-equiv=\"refresh\""));
    assert!(view.contains("Analyte sends nothing"));
    let token = csrf(&view)?;
    let stopped = fixture
        .post("/setup/listen/stop", &format!("csrf={}", encode(&token)))
        .await?;
    assert!(stopped.starts_with("HTTP/1.1 303"), "{stopped}");
    let ended = fixture.get("/setup/listen").await?;
    assert!(ended.contains("the bytes were discarded"), "{ended}");
    assert!(!ended.contains("SYN-MARK-731"));
    assert!(fixture.written.lock().map_err(|_| "poisoned")?.is_empty());
    let logged = String::from_utf8_lossy(&logs.lock().map_err(|_| "log poisoned")?).into_owned();
    assert!(!logged.contains("SYN-MARK-731"));
    let marker = MARKER;
    for entry in std::fs::read_dir(&fixture.data)? {
        let path = entry?.path();
        if path.is_file() {
            let bytes = std::fs::read(&path)?;
            assert!(
                !bytes.windows(marker.len()).any(|w| w == marker),
                "{} holds the listened bytes",
                path.display()
            );
        }
    }
    let receipts = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.parked(0)?.len()))
        .await?;
    assert_eq!(receipts, 0);
    Ok(())
}

// Owner request 2026-11-18 (setup page): "Restart now" is audited and stops the
// admin server with the restart outcome the service turns into a managed restart.
#[tokio::test(flavor = "multi_thread")]
async fn restart_now_is_audited_and_stops_the_server() -> Result<(), Error> {
    let fixture = start().await?;
    let page = fixture.get("/setup/synthetic").await?;
    let response = fixture
        .post("/setup/restart", &format!("csrf={}", encode(&csrf(&page)?)))
        .await?;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("Restarting"));
    let outcome = tokio::time::timeout(Duration::from_secs(10), fixture.owner).await??;
    assert!(matches!(
        outcome,
        Err(crate::admin::Error::RestartRequested)
    ));
    let audits = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.audits(0)?))
        .await?;
    assert!(
        audits
            .iter()
            .any(|(_, audit)| audit.action.as_str() == "setup-restart")
    );
    Ok(())
}

// Owner request 2026-11-18 (setup page): choosing another shipped definition
// changes only that entry's `definition` in the program file, with a backup; a
// choice the whole program then refuses (here the store record bound) puts every
// previous file back.
#[tokio::test(flavor = "multi_thread")]
async fn definition_change_edits_program_and_rolls_back_on_refusal() -> Result<(), Error> {
    let fixture = start().await?;
    let root = fixture.config.parent().ok_or("root")?;
    let big = root.join("machines/example/big-hl7");
    std::fs::create_dir_all(&big)?;
    std::fs::write(
        big.join("machine.toml"),
        include_str!("../../../../../core/config/examples/hl7.toml")
            .rewrite("bytes = 1048576", "bytes = 4194304"),
    )?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    let site_before = std::fs::read_to_string(&fixture.site)?;
    let too_big = fixture
        .form(&[
            ("definition", "example/big-hl7"),
            ("host", "127.0.0.1"),
            ("tcp_port", "5600"),
        ])
        .await?;
    let refused = fixture.post("/setup/synthetic", &too_big).await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("previous files were put back"),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    assert_eq!(std::fs::read_to_string(&fixture.site)?, site_before);
    let body = fixture
        .form(&[
            ("definition", "example/network-hl7"),
            ("host", "127.0.0.1"),
            ("tcp_port", "5600"),
        ])
        .await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    let program = std::fs::read_to_string(&fixture.config)?;
    assert!(
        program.contains("definition = \"machines/example/network-hl7/machine.toml\""),
        "{program}"
    );
    assert!(program.contains("site = 'site.toml'"));
    let loaded = Program::load(&fixture.config)?;
    let machine = loaded.machines().first().ok_or("machine missing")?;
    assert_eq!(
        machine.snapshot().effective().site().endpoint,
        analyte_config::Endpoint::Fixed("127.0.0.1:5600".into())
    );
    let backups = fixture.backups()?;
    assert!(
        backups
            .iter()
            .any(|path| std::fs::read_to_string(path).is_ok_and(|text| text == program_before))
    );
    Ok(())
}

/// A program with the setup page, the running serial machine `synthetic` and the
/// waiting `machine-2`, whose starter site file is written beside it.
async fn start_two() -> Result<Fixture, Error> {
    let fixture = start_with(
        concat!(
            "[admin]\nport = 8080\n",
            "[[machines]]\nid = 'synthetic'\ndefinition = 'machines/example/serial-astm/machine.toml'\nsite = 'site.toml'\n",
            "[[machines]]\nid = 'machine-2'\nsite = 'machine-2.site.toml'\n"
        ),
        &include_str!("../../../../../core/config/examples/site.toml")
            .rewrite("endpoint = \"127.0.0.1:5600\"", "usb_serial = \"A10K3XYZ\""),
    )
    .await?;
    std::fs::write(
        fixture.root().join("machine-2.site.toml"),
        include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml"),
    )?;
    Ok(fixture)
}

impl Fixture {
    fn root(&self) -> &std::path::Path {
        self.config
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
    }

    /// Posts `fields` with the CSRF value of the page at `page`.
    async fn submit(
        &self,
        page: &str,
        path: &str,
        fields: &[(&str, &str)],
    ) -> Result<String, Error> {
        let form = self.get(page).await?;
        let mut body = vec![format!("csrf={}", encode(&csrf(&form)?))];
        body.extend(
            fields
                .iter()
                .map(|(name, value)| format!("{name}={}", encode(value))),
        );
        self.post(path, &body.join("&")).await
    }

    async fn unfinished(&self, machine: &str) -> Result<analyte_store::Unfinished, Error> {
        let machine = Identifier::new(machine)?;
        Ok(self
            .store
            .request::<_, FixtureError>(fixture_deadline()?, move |s| Ok(s.unfinished(&machine)?))
            .await?)
    }

    async fn audited(&self, action: &str) -> Result<Vec<String>, Error> {
        let audits = self
            .store
            .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.audits(0)?))
            .await?;
        Ok(audits
            .into_iter()
            .filter(|(_, audit)| audit.action.as_str() == action)
            .map(|(_, audit)| {
                let targets: Vec<&str> = audit.targets.iter().map(Identifier::as_str).collect();
                format!(
                    "{} {} {}",
                    audit.actor.as_str(),
                    targets.join(","),
                    audit.reason.as_str()
                )
            })
            .collect())
    }
}

/// Stores, for `machine`, one unread receipt with an unresolved flagged item and
/// one uncertain machine order with its order event waiting in the outbox.
async fn seed_unfinished(
    store: &analyte_store::Handle,
    machine: &'static str,
) -> Result<(), Error> {
    store
        .request::<_, FixtureError>(fixture_deadline()?, move |store| {
            let id = |value| Identifier::new(value).map_err(FixtureError::Text);
            let raw = b"protected-raw-never-log";
            let receipt = analyte_model::ReceiptId::try_from(uuid::Uuid::now_v7())
                .map_err(FixtureError::Identity)?;
            store.receive(
                &analyte_model::ReceiptMetadata {
                    id: receipt,
                    raw_digest: analyte_model::Digest(analyte_contract::content_digest(raw)),
                    connection: id("synthetic")?,
                    machine: id(machine)?,
                    driver: analyte_model::NamespacedId {
                        namespace: id("synthetic")?,
                        value: id("1")?,
                    },
                    config_version: id("1")?,
                    config_digest: analyte_model::Digest(analyte_contract::content_digest(b"c")),
                    received_at: jiff::Timestamp::from_second(100)
                        .map_err(FixtureError::EvidenceTimestamp)?,
                },
                raw,
                b"c",
            )?;
            store.park(&analyte_store::Parked {
                kind: id("synthetic")?,
                reason: Reason::new("synthetic flagged item").map_err(FixtureError::Text)?,
                targets: vec![
                    Identifier::new(&uuid::Uuid::from(receipt).to_string())
                        .map_err(FixtureError::Text)?,
                ],
            })?;
            store.order_page(&analyte_store::OrderPage {
                destination: id("demo")?,
                cursor: id("cursor")?,
                complete: true,
                polled: 100,
                orders: vec![analyte_store::CachedOrder {
                    id: id("order-1")?,
                    sample: analyte_model::SampleId::new("0042").map_err(FixtureError::Text)?,
                    document: "{\"version\":1,\"state\":\"requested\",\"tombstone\":false,\"tests\":[{\"namespace\":\"local\",\"value\":\"test\"}]}".into(),
                    tombstone: false,
                    expiry: 200,
                }],
            })?;
            let key = analyte_store::MachineOrderKey {
                destination: id("demo")?,
                order: id("order-1")?,
                machine: id(machine)?,
            };
            let reserved = store
                .reserve_send(
                    &analyte_store::OrderReservation {
                        identity: &key,
                        version: std::num::NonZeroU64::MIN,
                        now: 101,
                        freshness: 30,
                    },
                    analyte_store::ReserveOptions::default(),
                )?
                .map_err(|_| FixtureError::Conflict)?;
            store.transition_order(
                reserved.id,
                OrderState::Uncertain,
                analyte_model::SourceText::new("timeout").map_err(FixtureError::Text)?,
                102,
                |order| {
                    Ok(analyte_store::OrderEventDelivery {
                        id: analyte_model::DeliveryId::try_from(uuid::Uuid::now_v7())
                            .map_err(analyte_store::OrderError::Identity)?,
                        destination: order.key.destination.clone(),
                        order_id: order.key.order.clone(),
                        payload: b"protected-payload-never-log".to_vec(),
                        contract_version: 1,
                    })
                },
            )?;
            Ok::<_, FixtureError>(())
        })
        .await?;
    Ok(())
}

// Owner request 2026-11-18 (add machines); plan §11; ADR 0003 rule 2: "Add
// machine" writes one entry and the installers' starter site file (key names
// only), keeps a copy of the program file, passes check-config, is audited, and
// the page lists the new machine as not set up yet.
#[tokio::test(flavor = "multi_thread")]
async fn add_machine_writes_a_checked_entry_and_starter_site() -> Result<(), Error> {
    let fixture = start_with(
        "[admin]\nport = 8080\n[[machines]]\nid = 'machine-1'\nsite = 'site.toml'\n",
        include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml"),
    )
    .await?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    let index = fixture.get("/setup").await?;
    assert!(
        index.contains("action=\"/setup/add\"><button>Add machine</button>"),
        "{index}"
    );
    let form = fixture.get("/setup/add").await?;
    assert!(
        form.contains("name=\"name\" required maxlength=\"64\" value=\"machine-2\""),
        "{form}"
    );
    assert!(form.contains("joins that stored history"), "{form}");
    let added = fixture
        .submit("/setup/add", "/setup/add", &[("name", "machine-2")])
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    assert!(added.contains("Restart now"), "{added}");
    let program = Program::load(&fixture.config)?;
    let waiting: Vec<&str> = program
        .awaiting_setup()
        .iter()
        .map(Identifier::as_str)
        .collect();
    assert_eq!(waiting, ["machine-1", "machine-2"]);
    let text = std::fs::read_to_string(&fixture.config)?;
    let head = program_before
        .get(..program_before.find("[store]").ok_or("store")?)
        .ok_or("head")?;
    assert!(text.starts_with(head), "{text}");
    assert!(
        text.contains("[[machines]]\nid = \"machine-2\"\nsite = \"machine-2.site.toml\"\n"),
        "{text}"
    );
    let site = std::fs::read_to_string(fixture.root().join("machine-2.site.toml"))?;
    let starter: toml::Table =
        include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml").parse()?;
    assert_eq!(
        site.parse::<toml::Table>()?,
        starter,
        "same settings as the installers' starter"
    );
    assert!(
        fixture
            .backups()?
            .iter()
            .any(|path| std::fs::read_to_string(path).is_ok_and(|t| t == program_before)),
        "the previous program file is kept"
    );
    let index = fixture.get("/setup").await?;
    assert!(
        index.contains("<td>machine-2</td><td>Not set up yet</td>"),
        "{index}"
    );
    let page = fixture.get("/setup/machine-2").await?;
    assert!(page.contains("This machine is not set up yet"), "{page}");
    let audits = fixture.audited("setup-add").await?;
    assert_eq!(audits.len(), 1);
    assert!(
        audits[0].contains(" machine-2 setup page added machine machine-2"),
        "{audits:?}"
    );
    Ok(())
}

// Owner request 2026-11-18 (add machines): a duplicate or invalid name is refused
// before anything is written, and a name with stored history is added only when
// the operator confirms it is the same analyzer (safety rule 4: never join two
// analyzers' results by accident).
#[tokio::test(flavor = "multi_thread")]
async fn add_refuses_duplicate_and_invalid_names_before_writing() -> Result<(), Error> {
    let fixture = start_two().await?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    let files_before = std::fs::read_dir(fixture.root())?.count();
    for (name, expected) in [
        ("machine-2", "already listed"),
        ("SYNTHETIC", "already listed"),
        ("two words", "only letters"),
        ("../x", "only letters"),
        ("listen", "setup page uses itself"),
        ("", "not accepted"),
    ] {
        let refused = fixture
            .submit("/setup/add", "/setup/add", &[("name", name)])
            .await?;
        assert!(refused.starts_with("HTTP/1.1 409"), "{name}: {refused}");
        assert!(refused.contains(expected), "{name}: {refused}");
    }
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    assert_eq!(std::fs::read_dir(fixture.root())?.count(), files_before);
    assert!(fixture.backups()?.is_empty());
    seed_unfinished(&fixture.store, "old-analyzer").await?;
    let unconfirmed = fixture
        .submit("/setup/add", "/setup/add", &[("name", "old-analyzer")])
        .await?;
    assert!(unconfirmed.starts_with("HTTP/1.1 409"), "{unconfirmed}");
    assert!(
        unconfirmed.contains("has stored results or history"),
        "{unconfirmed}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    let confirmed = fixture
        .submit(
            "/setup/add",
            "/setup/add",
            &[("name", "old-analyzer"), ("same_analyzer", "yes")],
        )
        .await?;
    assert!(confirmed.starts_with("HTTP/1.1 200"), "{confirmed}");
    assert!(confirmed.contains("joins the stored history"));
    assert!(fixture.audited("setup-add").await?[0].contains("same analyzer confirmed"));
    Ok(())
}

// Owner request 2026-11-18 (remove machines); AGENTS.md safety rules 2 and 8;
// ADR 0003 rule 2: removing asks plainly, warns about unfinished outbox, order and
// flagged work and needs "Remove anyway" for it, takes only the program entry out,
// keeps the site file as a dated copy and never touches the store. Removing the
// last machine is allowed and the setup page keeps working.
#[tokio::test(flavor = "multi_thread")]
async fn remove_warns_then_keeps_store_and_site_backup() -> Result<(), Error> {
    let fixture = start_two().await?;
    seed_unfinished(&fixture.store, "synthetic").await?;
    let open = fixture.unfinished("synthetic").await?;
    assert_eq!(
        open,
        analyte_store::Unfinished {
            undelivered: 1,
            open_orders: 1,
            unread: 1,
            flagged: 1,
        }
    );
    let site_before = std::fs::read_to_string(&fixture.site)?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    let confirm = fixture.get("/setup/synthetic/remove").await?;
    assert!(
        confirm.contains(concat!(
            "Remove synthetic?</strong> Its stored results, flagged items and history stay in ",
            "the database and remain visible to staff; the machine just stops being served."
        )),
        "{confirm}"
    );
    for expected in [
        "1 results or order messages not yet accepted by the lab app",
        "1 orders sent to the machine and not finished",
        "1 received messages not read yet",
        "1 unresolved flagged items linked to this machine",
        "No other set-up machine uses lab app destination demo",
        "<button>Remove anyway</button>",
    ] {
        assert!(confirm.contains(expected), "missing {expected}: {confirm}");
    }
    let refused = fixture
        .submit("/setup/synthetic/remove", "/setup/synthetic/remove", &[])
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(refused.contains("still has unfinished work"), "{refused}");
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    assert!(fixture.backups()?.is_empty());
    let parked_before = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.parked(0)?.len()))
        .await?;
    let removed = fixture
        .submit(
            "/setup/synthetic/remove",
            "/setup/synthetic/remove",
            &[("anyway", "yes")],
        )
        .await?;
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    assert!(removed.contains("Restart now"));
    let program = Program::load(&fixture.config)?;
    assert!(program.machines().is_empty());
    let waiting: Vec<&str> = program
        .awaiting_setup()
        .iter()
        .map(Identifier::as_str)
        .collect();
    assert_eq!(waiting, ["machine-2"]);
    assert!(!fixture.site.exists(), "the site file moved to its backup");
    let backups = fixture.backups()?;
    assert!(
        backups
            .iter()
            .any(|p| std::fs::read_to_string(p).is_ok_and(|t| t == site_before))
    );
    assert!(
        backups
            .iter()
            .any(|p| std::fs::read_to_string(p).is_ok_and(|t| t == program_before))
    );
    // The store is untouched: the same work is still open and still listed.
    assert_eq!(fixture.unfinished("synthetic").await?, open);
    let parked_after = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.parked(0)?.len()))
        .await?;
    assert_eq!(parked_after, parked_before);
    let receipts = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.receipts("")?))
        .await?;
    assert!(receipts.iter().any(|r| r.machine.as_str() == "synthetic"));
    let parked_page = fixture.get("/parked").await?;
    assert!(
        parked_page.contains("synthetic flagged item"),
        "{parked_page}"
    );
    let index = fixture.get("/setup").await?;
    assert!(
        index.contains("<td>synthetic</td><td>Removed: runs until the restart</td>"),
        "{index}"
    );
    let audits = fixture.audited("setup-remove").await?;
    assert!(audits[0].contains("store untouched") && audits[0].contains("removed anyway: yes"));
    Ok(())
}

// Owner request 2026-11-18 (remove machines): removing the last machine is
// allowed; the service then lists no machines, the setup page keeps working and
// a machine can be added again.
#[tokio::test(flavor = "multi_thread")]
async fn removing_the_last_machine_keeps_the_setup_page() -> Result<(), Error> {
    let fixture = start_with(
        "[admin]
port = 8080
[[machines]]
id = 'machine-1'
site = 'site.toml'
",
        include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml"),
    )
    .await?;
    // Nothing is unfinished, so the button is a plain "Remove".
    let last = fixture.get("/setup/machine-1/remove").await?;
    assert!(
        last.contains("Nothing is unfinished") && last.contains("This is the last machine"),
        "{last}"
    );
    assert!(last.contains("<button>Remove</button>"));
    let removed = fixture
        .submit("/setup/machine-1/remove", "/setup/machine-1/remove", &[])
        .await?;
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    let empty = Program::load(&fixture.config)?;
    assert!(empty.machines().is_empty() && empty.awaiting_setup().is_empty());
    let index = fixture.get("/setup").await?;
    assert!(
        index.starts_with("HTTP/1.1 200") && index.contains("Add machine"),
        "{index}"
    );
    let added = fixture
        .submit(
            "/setup/add",
            "/setup/add",
            &[("name", "machine-1"), ("same_analyzer", "yes")],
        )
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    assert_eq!(Program::load(&fixture.config)?.awaiting_setup().len(), 1);
    Ok(())
}

// Owner request 2026-11-18 (add and remove machines): when the whole program no
// longer loads after the write, add and remove put every file back as it was;
// an added starter site file does not stay behind, a removed one is not moved.
#[tokio::test(flavor = "multi_thread")]
async fn add_and_remove_roll_back_when_the_reload_fails() -> Result<(), Error> {
    let fixture = start_two().await?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    // An alert policy names synthetic, so a program without it does not load.
    let with_alert = format!(
        "{program_before}\n[[alerts]]\nkind = 'machine_silent'\nthreshold = 60\nresponder = 'duty'\nmachine = 'synthetic'\n"
    );
    std::fs::write(&fixture.config, &with_alert)?;
    let refused = fixture
        .submit("/setup/synthetic/remove", "/setup/synthetic/remove", &[])
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("previous files were put back"),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.config)?, with_alert);
    assert!(
        fixture.site.exists(),
        "the site file stays when nothing was removed"
    );
    std::fs::write(&fixture.config, &program_before)?;
    // A broken site file of another machine makes every reload fail.
    std::fs::write(&fixture.site, "schema_version = 1\nendpoint = 5\n")?;
    let refused = fixture
        .submit("/setup/add", "/setup/add", &[("name", "machine-3")])
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("previous files were put back"),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    assert!(!fixture.root().join("machine-3.site.toml").exists());
    Ok(())
}

// Owner request 2026-11-18 (add machines): completing a machine with a serial
// number another machine already uses is refused before anything is written,
// and check-config refuses such a program written by hand.
#[tokio::test(flavor = "multi_thread")]
async fn two_machines_cannot_share_a_port_or_serial_number() -> Result<(), Error> {
    let fixture = start_two().await?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    let page = "/setup/machine-2?definition=example/serial-astm";
    let fields = |port| {
        [
            ("definition", "example/serial-astm"),
            ("port", port),
            ("base_url", "https://lab.example.invalid/v1"),
            ("destination", "lab-app"),
        ]
    };
    let refused = fixture
        .submit(page, "/setup/machine-2", &fields("usb:A10K3XYZ"))
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("machines synthetic and machine-2 both use USB serial number A10K3XYZ"),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    assert!(fixture.backups()?.is_empty());
    let saved = fixture
        .submit(page, "/setup/machine-2", &fields("name:COM3"))
        .await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    // The same adapter written by hand: the program loader refuses it.
    let second = fixture.root().join("machine-2.site.toml");
    let text = std::fs::read_to_string(&second)?;
    std::fs::write(
        &second,
        text.replace("endpoint = \"COM3\"", "usb_serial = \"a10k3xyz\""),
    )?;
    assert!(matches!(
        Program::load(&fixture.config),
        Err(crate::installation::config::Error::SharedConnection { .. })
    ));
    Ok(())
}
