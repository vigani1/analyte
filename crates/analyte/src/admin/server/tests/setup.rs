//! Tests the setup page, its steps and the Lab app page: definitions, detected ports, saving with backups, lab app moves, refusals, keys and the listen test with its verdict.
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
    /// What the fake serial port sends once after it is opened; `MARKER` by default.
    input: Arc<Mutex<Vec<u8>>>,
}

/// A fake serial port: sends its input once, then only times out; records writes.
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

/// The fixture's test bounds: long enough for a test to read and stop it.
const LIMITS: crate::admin::setup::Limits = crate::admin::setup::Limits {
    duration: Duration::from_secs(20),
    bytes: 1024,
};

fn seams(
    root: &std::path::Path,
    written: Arc<Mutex<Vec<u8>>>,
    input: Arc<Mutex<Vec<u8>>>,
    limits: crate::admin::setup::Limits,
) -> crate::admin::setup::Seams {
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
                input: input
                    .lock()
                    .map_err(|_| std::io::Error::other("poisoned"))?
                    .clone(),
                written: written.clone(),
            }) as Box<dyn analyte_transport::serial::Port>)
        }),
        limits,
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

/// [`start`] with the listen test bounded by `limits`.
async fn start_limited(limits: crate::admin::setup::Limits) -> Result<Fixture, Error> {
    start_with_limits(
        "[[machines]]
id = 'synthetic'
definition = 'machines/example/serial-astm/machine.toml'
site = 'site.toml'
",
        &include_str!("../../../../../core/config/examples/site.toml")
            .rewrite("endpoint = \"127.0.0.1:5600\"", "usb_serial = \"A10K3XYZ\""),
        limits,
    )
    .await
}

/// A fixture whose program file lists `machines` and whose `site.toml` holds `site_text`.
async fn start_with(machines: &str, site_text: &str) -> Result<Fixture, Error> {
    start_with_limits(machines, site_text, LIMITS).await
}

async fn start_with_limits(
    machines: &str,
    site_text: &str,
    limits: crate::admin::setup::Limits,
) -> Result<Fixture, Error> {
    let program = analyte_fixtures::program_with_limits(
        machines,
        &analyte_store::Limits {
            reserve_bytes: 1,
            database_bytes: 100_000_000,
            record_bytes: 2_097_152,
            page_rows: 100,
            busy_timeout: Duration::from_millis(100),
        },
    );
    start_program_with(&program, "site.toml", site_text, limits).await
}

/// A fixture with the whole program file `program_text` and the site file
/// `site_name` holding `site_text`, beside the shipped example definitions.
async fn start_program(
    program_text: &str,
    site_name: &str,
    site_text: &str,
) -> Result<Fixture, Error> {
    start_program_with(program_text, site_name, site_text, LIMITS).await
}

/// [`start_program`] with the listen test bounded by `limits`.
async fn start_program_with(
    program_text: &str,
    site_name: &str,
    site_text: &str,
    limits: crate::admin::setup::Limits,
) -> Result<Fixture, Error> {
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
    std::fs::write(
        blocked.join("README.md"),
        "# Roche cobas e 411\n\nA blocked machine definition.\n",
    )?;
    let site = root.join(site_name);
    std::fs::write(&site, site_text)?;
    let config = root.join("program.toml");
    std::fs::write(&config, program_text)?;
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
    let input = Arc::new(Mutex::new(MARKER.to_vec()));
    let setup = crate::admin::setup::Setup::new(
        &program,
        seams(root, written.clone(), input.clone(), limits),
    );
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
        retired: crate::admin::Draining::new(),
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
        input,
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

    /// The setup steps' fields for `synthetic` with `fields` replacing or adding
    /// values, and the session's CSRF value, as the test and Save post them.
    async fn form(&self, fields: &[(&str, &str)]) -> Result<String, Error> {
        let page = self.get("/setup/lab-apps/new").await?;
        let mut values = vec![
            ("csrf", csrf(&page)?),
            ("definition", "example/serial-astm".to_owned()),
            ("port_name", String::new()),
            ("lab_app", "demo".to_owned()),
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
// detected ports with adapter name and USB serial number. Owner request
// 2026-11-19 (item 8): models show the README title, or a tidied key without
// one, with the key on hover; the settings version is short with the digest on hover.
#[tokio::test(flavor = "multi_thread")]
async fn page_lists_definitions_and_detected_adapters() -> Result<(), Error> {
    let fixture = start().await?;
    let index = fixture.get("/setup").await?;
    assert!(index.starts_with("HTTP/1.1 200"), "{index}");
    for expected in [
        "<span title=\"example/serial-astm (name generic-synthetic)\">Example serial astm</span>",
        "<span title=\"Evidence level: documented\">Built from the manuals</span>",
        "<span title=\"roche/cobas-e411\">Roche cobas e 411</span>",
        "<span class=\"pill warn\">Not ready yet</span>",
        "Facts or recordings are still missing",
        concat!(
            "<td data-label=\"Model\"><span title=\"machines/example/serial-astm/machine.toml\">",
            "Example serial astm</span></td>",
        ),
        "href=\"/setup/synthetic\"",
    ] {
        assert!(index.contains(expected), "missing {expected}");
    }
    assert!(!index.contains("class=\"digest\""));
    let model = fixture.get("/setup/synthetic").await?;
    assert!(model.starts_with("HTTP/1.1 200"), "{model}");
    assert!(
        model.contains("value=\"roche/cobas-e411\" required disabled"),
        "{model}"
    );
    assert!(
        model.contains("value=\"example/serial-astm\" required checked"),
        "the installed model is chosen: {model}"
    );
    let machine = fixture
        .get("/setup/synthetic/connection?definition=example/serial-astm")
        .await?;
    assert!(machine.starts_with("HTTP/1.1 200"), "{machine}");
    for expected in [
        "value=\"usb:A10K3XYZ\" checked> COM12 — FT232R USB UART — saved as serial number A10K3XYZ",
        "value=\"name:COM3\"> COM3 — USB-SERIAL CH340 — no serial number; saved as the port name",
        "Analyte saves that number",
    ] {
        assert!(machine.contains(expected), "missing {expected}");
    }
    let test = fixture
        .get(
            "/setup/synthetic/test?definition=example/serial-astm&port=usb%3AA10K3XYZ&lab_app=demo",
        )
        .await?;
    assert!(test.contains("may contain patient data"), "{test}");
    let network = fixture
        .get("/setup/synthetic/connection?definition=example/network-hl7")
        .await?;
    assert!(network.contains("name=\"host\""), "{network}");
    assert!(!network.contains("name=\"port\""));
    Ok(())
}

/// The packaging workflow's check, in plain code: one table row of `page` names
/// `machine` as a cell's whole text and, later in the same row, says "Not set up
/// yet". Cell markup inside the row may change.
fn row_not_set_up(page: &str, machine: &str) -> bool {
    let cell = format!(">{machine}<");
    page.split("<tr").skip(1).any(|row| {
        let row = row.split("</tr>").next().unwrap_or_default();
        row.find(&cell).is_some_and(|at| {
            row.get(at..)
                .is_some_and(|rest| rest.contains("Not set up yet"))
        })
    })
}

// Owner request 2026-11-19 (lab app set once; PR #260 packaging failure): the
// installers' exact starter files load, the machine not set up needs no lab app,
// and the setup page lists machine-1 as not set up, as the packaging workflow
// checks after install, with a link to add the first lab app.
#[tokio::test(flavor = "multi_thread")]
async fn starter_setup_page_matches_the_packaging_check() -> Result<(), Error> {
    // Both installer jobs use this row-level match, not exact cell markup.
    let workflow = include_str!("../../../../../../.github/workflows/packaging.yml");
    assert_eq!(
        workflow
            .matches(">machine-1<(?:(?!</tr>).)*Not set up yet")
            .count(),
        2,
        "the Windows and Linux checks match one row"
    );
    let link = "No lab app yet: <a href=\"/setup/lab-apps/new\">add one</a>";
    for program in [
        include_str!("../../../../../../packaging/linux/starter/analyte.toml"),
        include_str!("../../../../../../packaging/windows/starter/analyte.toml"),
    ] {
        // Only the data folder moves into the test's private folder.
        let program: String = program
            .lines()
            .map(|line| {
                if line.starts_with("data_directory") {
                    "data_directory = 'data'\n".to_owned()
                } else {
                    format!("{line}\n")
                }
            })
            .collect();
        let fixture = start_program(
            &program,
            "machine-1.site.toml",
            include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml"),
        )
        .await?;
        let loaded = Program::load(&fixture.config)?;
        assert!(loaded.machines().is_empty() && loaded.lab_apps().is_empty());
        let waiting: Vec<&str> = loaded
            .awaiting_setup()
            .iter()
            .map(Identifier::as_str)
            .collect();
        assert_eq!(waiting, ["machine-1"]);
        let index = fixture.get("/setup").await?;
        assert!(index.starts_with("HTTP/1.1 200"), "{index}");
        assert!(row_not_set_up(&index, "machine-1"), "{index}");
        assert!(!row_not_set_up(&index, "machine-2"), "{index}");
        assert!(index.contains(link), "missing {link}: {index}");
        let lab_apps = fixture.get("/setup/lab-apps").await?;
        assert!(lab_apps.contains("No lab app is set up yet"), "{lab_apps}");
    }
    Ok(())
}

// Owner request 2026-11-18 (installers) and 2026-11-19 (lab app set once): the
// installed starter lists a machine without a definition; with no lab app the
// form links to the Lab app page, which stores the keys once; the machine then
// picks the only lab app by default, and a save adds the definition line, so
// the program runs it with the stored keys.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario: from the starter through the Lab app page and the steps to the saved summary"
)]
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
        index.contains(
            "<td><strong>machine-1</strong></td><td data-label=\"Model\"><span class=\"muted\">Not chosen yet</span></td><td data-label=\"Status\"><span class=\"pill\">Not set up yet</span></td>"
        ),
        "{index}"
    );
    let first = fixture.get("/setup/machine-1").await?;
    assert!(first.starts_with("HTTP/1.1 200"), "{first}");
    assert!(first.contains(STEP_ONE), "{first}");
    assert!(
        !first.contains(" checked"),
        "no model is chosen yet: {first}"
    );
    assert!(!first.contains("name=\"lab_app\""));
    let page = "/setup/machine-1/connection?definition=example/network-hl7";
    let empty = fixture.get(page).await?;
    assert!(empty.contains("name=\"host\""), "{empty}");
    assert!(
        empty.contains(
            "No lab app is set up yet. <a href=\"/setup/lab-apps/new\">Add a lab app</a>"
        ),
        "{empty}"
    );
    assert!(
        !empty.contains("name=\"api_key\""),
        "keys are not asked per machine"
    );
    let added = fixture
        .submit(
            "/setup/lab-apps/new",
            "/setup/lab-apps/new",
            &[
                ("name", "lab-app"),
                ("base_url", "https://lab.example.invalid/v1"),
                ("freshness_seconds", "30"),
                ("request_timeout_seconds", "10"),
                ("api_key", API_KEY),
                ("signing_key", SIGNING_KEY),
            ],
        )
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    assert!(added.contains("Key stored: lab-app-api"), "{added}");
    let chosen = fixture.get(page).await?;
    assert!(
        chosen.contains(
            "<option value=\"lab-app\" selected>lab-app (https://lab.example.invalid/v1)</option>"
        ),
        "the only lab app is the default: {chosen}"
    );
    let review = fixture
        .get("/setup/machine-1/save?definition=example%2Fnetwork-hl7&host=127.0.0.1&tcp_port=5600&lab_app=lab-app")
        .await?;
    assert!(review.starts_with("HTTP/1.1 200"), "{review}");
    let body = [
        ("csrf", csrf(&review)?),
        ("definition", "example/network-hl7".into()),
        ("host", "127.0.0.1".into()),
        ("tcp_port", "5600".into()),
        ("lab_app", "lab-app".into()),
    ]
    .iter()
    .map(|(name, value)| format!("{name}={}", encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    let saved = fixture.post("/setup/machine-1", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains("Running now</dt><dd><span class=\"muted\">Not running yet</span>"));
    for shown in [
        "<dt>Machine</dt><dd><strong>machine-1</strong></dd>",
        "<dt>Model</dt><dd>Example network hl7</dd>",
        "Analyte listens on 127.0.0.1:5600; the machine connects to it",
        "<dt>Lab app</dt><dd>lab-app (https://lab.example.invalid/v1)</dd>",
        "Restart now",
    ] {
        assert!(saved.contains(shown), "missing {shown}: {saved}");
    }
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
    let site = std::fs::read_to_string(&fixture.site)?;
    assert!(site.contains("lab_app = \"lab-app\""), "{site}");
    assert!(!site.contains("[destination]"), "{site}");
    let stored = crate::installation::credentials::Locked::new(&fixture.data).read()?;
    assert!(
        crate::installation::secrets::Environment::load_layered(&after, |_| None, &stored).is_ok(),
        "the stored keys must satisfy startup"
    );
    assert!(!saved.contains(API_KEY) && !saved.contains(SIGNING_KEY));
    Ok(())
}

// Owner request 2026-11-18 (setup page) and 2026-11-19 (lab app set once): an
// invalid lab app address, missing keys, or an unknown lab app picked for a
// machine are refused before any file, backup or key is written.
#[tokio::test(flavor = "multi_thread")]
async fn invalid_address_is_refused_before_anything_is_written() -> Result<(), Error> {
    let fixture = start().await?;
    let before = std::fs::read_to_string(&fixture.site)?;
    let lab_apps = fixture.root().join("lab-apps.toml");
    for (fields, refusal) in [
        (
            [
                ("base_url", "http://lab.example.invalid"),
                ("signing_key", SIGNING_KEY),
            ],
            "lab app address is not accepted",
        ),
        (
            [
                ("base_url", "https://lab.example.invalid"),
                ("signing_key", ""),
            ],
            "enter both keys for a new lab app",
        ),
    ] {
        let mut posted = vec![("name", "second"), ("api_key", API_KEY)];
        posted.extend(fields);
        let response = fixture
            .submit("/setup/lab-apps/new", "/setup/lab-apps/new", &posted)
            .await?;
        assert!(response.starts_with("HTTP/1.1 409"), "{response}");
        assert!(response.contains(refusal), "{response}");
        assert!(!response.contains(API_KEY) && !response.contains(SIGNING_KEY));
    }
    let body = fixture
        .form(&[("port", "usb:A10K3XYZ"), ("lab_app", "nowhere")])
        .await?;
    let response = fixture.post("/setup/synthetic", &body).await?;
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(
        response.contains("there is no lab app called nowhere"),
        "{response}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);
    assert!(fixture.backups()?.is_empty());
    assert!(!lab_apps.exists());
    assert!(!fixture.data.join("credentials").exists());
    Ok(())
}

// Owner request 2026-11-18 (setup page) and 2026-11-19 (lab app set once,
// items 4 and 6): a save writes a site file the program loader (check-config)
// accepts and keeps the old one; the site file's own [destination] moves into
// lab-apps.toml and the site file names it, and with the same values the
// effective config digest does not change.
#[tokio::test(flavor = "multi_thread")]
async fn save_moves_the_lab_app_and_keeps_the_digest() -> Result<(), Error> {
    let fixture = start().await?;
    let original = std::fs::read_to_string(&fixture.site)?;
    let digest = |program: &Program| -> Result<String, Error> {
        Ok(program
            .machines()
            .first()
            .ok_or("machine missing")?
            .snapshot()
            .digest()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    };
    let old = digest(&Program::load(&fixture.config)?)?;
    let form = fixture
        .get("/setup/synthetic/connection?definition=example/serial-astm")
        .await?;
    assert!(
        form.contains(
            "<option value=\"demo\" selected>demo (https://lab.example.invalid)</option>"
        ),
        "{form}"
    );
    assert!(
        !form.contains("name=\"api_key\""),
        "keys are not asked per machine"
    );
    // The same connection and lab app: only where the lab app is written moves.
    let same = fixture.form(&[("port", "usb:A10K3XYZ")]).await?;
    let saved = fixture.post("/setup/synthetic", &same).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains("This lab app is now kept on the"), "{saved}");
    assert!(saved.contains("Restart now"));
    let program = Program::load(&fixture.config)?;
    assert_eq!(digest(&program)?, old, "same values, same digest");
    assert!(saved.contains(&old), "saved page lacks the digest");
    let site = std::fs::read_to_string(&fixture.site)?;
    assert!(site.contains("lab_app = \"demo\""), "{site}");
    assert!(!site.contains("[destination]"), "{site}");
    assert!(site.contains("usb_serial = \"A10K3XYZ\""), "{site}");
    assert!(site.contains("retention = \"keep\""));
    let lab_apps = analyte_config::LabApps::load(
        "lab-apps.toml",
        &std::fs::read_to_string(fixture.root().join("lab-apps.toml"))?,
    )?;
    let site_table: toml::Table = original.parse()?;
    let written: analyte_config::Destination = site_table
        .get("destination")
        .ok_or("destination")?
        .clone()
        .try_into()?;
    assert_eq!(lab_apps.listed(), [written]);
    assert!(
        fixture
            .backups()?
            .iter()
            .any(|path| std::fs::read_to_string(path).is_ok_and(|text| text == original)),
        "the old site file is kept"
    );
    // An adapter without its own serial number is saved by port name; nothing
    // more moves, and the lab app file stays as it is.
    let lab_apps_text = std::fs::read_to_string(fixture.root().join("lab-apps.toml"))?;
    let by_name = fixture.form(&[("port", "name:COM3")]).await?;
    let named = fixture.post("/setup/synthetic", &by_name).await?;
    assert!(named.starts_with("HTTP/1.1 200"), "{named}");
    assert!(!named.contains("is now kept on the"), "{named}");
    let site = std::fs::read_to_string(&fixture.site)?;
    assert!(site.contains("endpoint = \"COM3\""), "{site}");
    assert!(!site.contains("usb_serial"));
    assert_eq!(
        std::fs::read_to_string(fixture.root().join("lab-apps.toml"))?,
        lab_apps_text
    );
    assert_eq!(fixture.backups()?.len(), 2, "one site backup per save");
    let audits = fixture.audited("setup-save").await?;
    assert!(
        audits[0].contains("lab app demo (https://lab.example.invalid); its lab app settings moved to lab-apps.toml"),
        "{audits:?}"
    );
    Ok(())
}

// Owner request 2026-11-19 (lab app set once): the Lab app page stores each key
// once in the locked file and then shows it only as "set on <date>"; a new lab
// app gets its own key names; no key reaches a page, an audit row or a log
// line. A lab app still written in a machine's site file keeps its address
// until that machine is saved.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario: refuse, store, show, add, then check every page, audit row and log line"
)]
async fn lab_app_page_stores_keys_once() -> Result<(), Error> {
    let logs = crate::runtime::log_capture::init();
    let fixture = start().await?;
    let list = fixture.get("/setup/lab-apps").await?;
    assert!(
        list.contains(">Still kept in the settings of synthetic</span></small>"),
        "{list}"
    );
    assert!(
        list.contains("<span title=\"demo-api\">API key</span>: not set"),
        "{list}"
    );
    let moved_first = fixture
        .submit(
            "/setup/lab-apps/demo",
            "/setup/lab-apps/demo",
            &[
                ("base_url", "https://moved.example.invalid"),
                ("api_key", API_KEY),
            ],
        )
        .await?;
    assert!(moved_first.starts_with("HTTP/1.1 409"), "{moved_first}");
    assert!(
        moved_first.contains("still kept in the settings of synthetic; save that machine"),
        "{moved_first}"
    );
    assert!(!fixture.data.join("credentials").exists());
    let saved = fixture
        .submit(
            "/setup/lab-apps/demo",
            "/setup/lab-apps/demo",
            &[
                ("base_url", "https://lab.example.invalid"),
                ("freshness_seconds", "30"),
                ("request_timeout_seconds", "10"),
                ("api_key", API_KEY),
                ("signing_key", SIGNING_KEY),
            ],
        )
        .await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains("Key stored: demo-api") && saved.contains("Key stored: demo-signing"));
    // The same values beside the site file's own copy: the program still loads.
    Program::load(&fixture.config)?;
    let stored = crate::installation::credentials::Locked::new(&fixture.data).read()?;
    assert_eq!(stored.value("demo-api"), Some(API_KEY));
    assert_eq!(stored.value("demo-signing"), Some(SIGNING_KEY));
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
    let again = fixture.get("/setup/lab-apps/demo").await?;
    assert!(
        again.contains("<span title=\"demo-api\">API key</span>: set on "),
        "{again}"
    );
    assert!(again.contains("<span title=\"demo-signing\">Signing key</span>: set on "));
    assert!(again.contains("Leave a box empty to keep the stored key"));
    let added = fixture
        .submit(
            "/setup/lab-apps/new",
            "/setup/lab-apps/new",
            &[
                ("name", "second"),
                ("base_url", "https://second.example.invalid"),
                ("api_key", API_KEY),
                ("signing_key", SIGNING_KEY),
            ],
        )
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    let stored = crate::installation::credentials::Locked::new(&fixture.data).read()?;
    assert!(stored.set_at("second-api").is_some() && stored.set_at("second-signing").is_some());
    let program = Program::load(&fixture.config)?;
    let names: Vec<&str> = program
        .lab_apps()
        .iter()
        .map(|known| known.settings().name.as_str())
        .collect();
    assert_eq!(names, ["demo", "second"]);
    let list = fixture.get("/setup/lab-apps").await?;
    let machine = fixture.get("/setup/synthetic").await?;
    let audits = fixture
        .store
        .request::<_, FixtureError>(fixture_deadline()?, |s| Ok(s.audits(0)?))
        .await?;
    assert!(
        audits
            .iter()
            .any(|(_, a)| a.action.as_str() == "lab-app-save")
    );
    assert!(
        audits
            .iter()
            .any(|(_, a)| a.action.as_str() == "lab-app-add")
    );
    let logged = String::from_utf8_lossy(&logs.lock().map_err(|_| "log poisoned")?).into_owned();
    for page in [&moved_first, &saved, &again, &added, &list, &machine] {
        assert!(!page.contains(API_KEY) && !page.contains(SIGNING_KEY));
    }
    for (_, audit) in &audits {
        let text = format!("{} {}", audit.action.as_str(), audit.reason.as_str());
        assert!(!text.contains(API_KEY) && !text.contains(SIGNING_KEY));
    }
    assert!(!logged.contains(API_KEY) && !logged.contains(SIGNING_KEY));
    Ok(())
}

// Owner request 2026-11-19 (lab app set once): removing a lab app is refused
// while a machine uses it, and allowed, with a backup, when nothing uses it.
#[tokio::test(flavor = "multi_thread")]
async fn lab_app_in_use_is_not_removed() -> Result<(), Error> {
    let fixture = start().await?;
    let confirm = fixture.get("/setup/lab-apps/demo/remove").await?;
    assert!(confirm.contains("It cannot be removed now"), "{confirm}");
    assert!(
        confirm.contains("Machines send results to it: synthetic."),
        "{confirm}"
    );
    assert!(!confirm.contains("<button class=\"danger\">Remove</button>"));
    let refused = fixture
        .submit("/setup/lab-apps/new", "/setup/lab-apps/demo/remove", &[])
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("lab app demo cannot be removed"),
        "{refused}"
    );
    assert!(!fixture.root().join("lab-apps.toml").exists());
    let added = fixture
        .submit(
            "/setup/lab-apps/new",
            "/setup/lab-apps/new",
            &[
                ("name", "spare"),
                ("base_url", "https://spare.example.invalid"),
                ("api_key", API_KEY),
                ("signing_key", SIGNING_KEY),
            ],
        )
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    let confirm = fixture.get("/setup/lab-apps/spare/remove").await?;
    assert!(
        confirm.contains("<button class=\"danger\">Remove</button>"),
        "{confirm}"
    );
    let removed = fixture
        .submit(
            "/setup/lab-apps/spare/remove",
            "/setup/lab-apps/spare/remove",
            &[],
        )
        .await?;
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    let program = Program::load(&fixture.config)?;
    assert!(program.lab_apps().is_empty(), "spare is gone");
    assert!(fixture.backups()?.iter().any(|path| {
        std::fs::read_to_string(path).is_ok_and(|text| text.contains("spare.example.invalid"))
    }));
    assert_eq!(fixture.audited("lab-app-remove").await?.len(), 1);
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
    let response = fixture
        .submit("/setup/lab-apps/new", "/setup/restart", &[])
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

/// The site file's `[destination]` moved into lab-apps.toml, not into the
/// program file, so its waiting results stay deliverable after the removal, and
/// the page says so.
fn assert_destination_kept(
    removed: &str,
    program: &Program,
    site_before: &str,
    root: &std::path::Path,
) -> Result<(), Error> {
    let kept = "Lab app demo stays on the Lab app page; results still waiting for it are sent";
    assert!(
        removed.contains("Restart now") && removed.contains(kept),
        "{removed}"
    );
    let site: toml::Table = site_before.parse()?;
    let expected: analyte_config::Destination = site
        .get("destination")
        .ok_or("destination")?
        .clone()
        .try_into()?;
    let kept: Vec<&analyte_config::Destination> = program
        .lab_apps()
        .iter()
        .map(crate::installation::config::LabApp::settings)
        .collect();
    assert_eq!(kept, [&expected]);
    let listed = analyte_config::LabApps::load(
        "lab-apps.toml",
        &std::fs::read_to_string(root.join("lab-apps.toml"))?,
    )?;
    assert_eq!(listed.listed(), [expected]);
    let text = std::fs::read_to_string(root.join("program.toml"))?;
    assert!(!text.contains("retired_destinations"), "{text}");
    Ok(())
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
    // Owner request 2026-11-19 (item 3): "Add machine" goes straight into step 1.
    assert!(added.starts_with("HTTP/1.1 303"), "{added}");
    assert!(
        added
            .to_ascii_lowercase()
            .contains("\r\nlocation: /setup/machine-2\r\n"),
        "{added}"
    );
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
        index.contains(
            "<td><strong>machine-2</strong></td><td data-label=\"Model\"><span class=\"muted\">Not chosen yet</span></td><td data-label=\"Status\"><span class=\"pill\">Not set up yet</span></td>"
        ),
        "{index}"
    );
    let page = fixture.get("/setup/machine-2").await?;
    assert!(page.contains(STEP_ONE), "{page}");
    assert!(page.contains("<h1>Set up machine-2</h1>"), "{page}");
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
    assert!(confirmed.starts_with("HTTP/1.1 303"), "{confirmed}");
    assert!(fixture.audited("setup-add").await?[0].contains("same analyzer confirmed"));
    Ok(())
}

// Owner request 2026-11-19 (add machines); AGENTS.md safety rule 4; plan §11: a
// name already listed is refused as listed before its stored history is looked
// up. The store's only owner thread is held busy, so a history lookup could not
// answer; the refusal must come back anyway, and say "already listed".
#[tokio::test(flavor = "multi_thread")]
async fn listed_name_refused_before_history_lookup() -> Result<(), Error> {
    let fixture = start_two().await?;
    // The listed machine also has history, so a lookup would ask to confirm it.
    seed_unfinished(&fixture.store, "synthetic").await?;
    let form = fixture.get("/setup/add").await?;
    let body = format!("csrf={}&name=SYNTHETIC", encode(&csrf(&form)?));
    let (started, busy) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let store = fixture.store.clone();
    let holder = tokio::spawn(async move {
        store
            .request::<_, FixtureError>(fixture_deadline()?, move |_| {
                let _ = started.send(());
                // Bounded: the test releases it, or it gives up by itself.
                let _ = released.recv_timeout(Duration::from_secs(20));
                Ok(())
            })
            .await
            .map_err(Error::from)
    });
    busy.await?;
    let refused = tokio::time::timeout(Duration::from_secs(2), fixture.post("/setup/add", &body))
        .await
        .map_err(|_| "the listed name waited for the store: history was looked up first")?;
    release.send(())?;
    holder.await??;
    let refused = refused?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(refused.contains("already listed"), "{refused}");
    assert!(
        !refused.contains("has stored results or history"),
        "{refused}"
    );
    Ok(())
}

// Owner request 2026-11-18 (remove machines); AGENTS.md safety rules 2 and 8;
// ADR 0003 rule 2: removing asks plainly, warns about unfinished outbox, order and
// flagged work and needs "Remove anyway" for it, takes only the program entry out,
// keeps the site file as a dated copy and never touches the store. Removing the
// last machine is allowed and the setup page keeps working.
#[tokio::test(flavor = "multi_thread")]
#[expect(
    clippy::too_many_lines,
    reason = "one scenario: warn, refuse, remove anyway, then check files, store and lab app"
)]
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
            "Remove synthetic?</strong> Its results, items to review and history are kept and ",
            "stay visible here; Analyte just stops connecting to it."
        )),
        "{confirm}"
    );
    for expected in [
        "1 result or order message not yet accepted by the lab app",
        "1 order sent to the machine and not finished",
        "1 received message not read yet",
        "1 item waiting for review linked to this machine (Needs review page)",
        "the lab app\">Its 1 waiting result will still be sent to demo.</span></p>",
        "<button class=\"danger\">Remove anyway</button>",
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
    let program = Program::load(&fixture.config)?;
    assert!(program.machines().is_empty());
    assert_destination_kept(&removed, &program, &site_before, fixture.root())?;
    // Owner request 2026-11-19 (lab app set once): no machine uses demo now, but
    // a result still waits for it, so the Lab app page refuses to remove it.
    let refused = fixture
        .submit("/setup/lab-apps/new", "/setup/lab-apps/demo/remove", &[])
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("1 result or order message still wait for it"),
        "{refused}"
    );
    assert!(
        !refused.contains("Machines send results to it"),
        "{refused}"
    );
    assert_eq!(Program::load(&fixture.config)?.lab_apps().len(), 1);
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
        index.contains(concat!(
            "<td><strong>synthetic</strong></td><td data-label=\"Model\"></td>",
            "<td data-label=\"Status\"><span class=\"pill warn\">",
            "Removed: stops at the restart</span></td>"
        )),
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
    assert!(last.contains("<button class=\"danger\">Remove</button>"));
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
    assert!(added.starts_with("HTTP/1.1 303"), "{added}");
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
    let page = "/setup/lab-apps/new";
    let fields = |port| {
        [
            ("definition", "example/serial-astm"),
            ("port", port),
            ("lab_app", "demo"),
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
    // demo, written only in synthetic's site file, was copied into lab-apps.toml.
    assert_eq!(Program::load(&fixture.config)?.lab_apps().len(), 1);
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

/// Step 1 marked as the current step.
const STEP_ONE: &str = "<li aria-current=\"step\"><span class=\"number\">1</span>Machine</li>";

/// The choices for `synthetic` on the serial example with a detected adapter, as a query.
const DRAFT: &str = "definition=example%2Fserial-astm&port=name%3ACOM3&lab_app=demo";

/// The indicator's item for the current step.
fn current(number: &str, label: &str) -> String {
    format!("<li aria-current=\"step\"><span class=\"number\">{number}</span>{label}</li>")
}

// Owner request 2026-11-19 (item 3): setup is four steps, one per screen, each
// marking itself in the indicator; Back and Next carry every choice, as hidden
// fields on the steps that do not show it; the summary says what will be saved,
// and nothing is written before Save.
#[tokio::test(flavor = "multi_thread")]
async fn each_step_shows_its_indicator_and_keeps_the_choices() -> Result<(), Error> {
    let fixture = start().await?;
    let site_before = std::fs::read_to_string(&fixture.site)?;
    for (path, number, label) in [
        (format!("/setup/synthetic?{DRAFT}"), "1", "Machine"),
        (
            format!("/setup/synthetic/connection?{DRAFT}"),
            "2",
            "Connection",
        ),
        (format!("/setup/synthetic/test?{DRAFT}"), "3", "Test"),
        (format!("/setup/synthetic/save?{DRAFT}"), "4", "Save"),
    ] {
        let page = fixture.get(&path).await?;
        assert!(page.starts_with("HTTP/1.1 200"), "{path}: {page}");
        assert!(page.contains(&current(number, label)), "{path}: {page}");
        assert_eq!(page.matches("aria-current=\"step\"").count(), 1, "{path}");
    }
    // Back to step 1: the model stays chosen and the later choices ride hidden.
    let first = fixture.get(&format!("/setup/synthetic?{DRAFT}")).await?;
    for kept in [
        "value=\"example/serial-astm\" required checked",
        "<input type=\"hidden\" name=\"port\" value=\"name:COM3\">",
        "<input type=\"hidden\" name=\"lab_app\" value=\"demo\">",
        "<form method=\"get\" action=\"/setup/synthetic/connection\">",
        "<button>Next: Connection</button>",
    ] {
        assert!(first.contains(kept), "missing {kept}: {first}");
    }
    // Step 2 shows its own choices and carries the model; Back goes to step 1.
    let second = fixture
        .get(&format!("/setup/synthetic/connection?{DRAFT}"))
        .await?;
    for kept in [
        "value=\"name:COM3\" checked> COM3",
        "<input type=\"hidden\" name=\"definition\" value=\"example/serial-astm\">",
        "<option value=\"demo\" selected>",
        "<h2>Lab app</h2><label class=\"field\"><span>Send results to</span>",
        "formaction=\"/setup/synthetic\" formnovalidate>Back</button>",
        "<button>Next: Test</button>",
    ] {
        assert!(second.contains(kept), "missing {kept}: {second}");
    }
    assert!(
        !second.contains("type=\"hidden\" name=\"port\""),
        "{second}"
    );
    assert!(
        !second.contains("value=\"usb:A10K3XYZ\" checked"),
        "the draft's port wins over the saved one"
    );
    // Steps 3 and 4 carry every choice; the summary names them in plain words.
    let review = fixture
        .get(&format!("/setup/synthetic/save?{DRAFT}"))
        .await?;
    for shown in [
        "<dt>Machine</dt><dd><strong>synthetic</strong></dd>",
        "<dt>Model</dt><dd>Example serial astm</dd>",
        "<span title=\"endpoint = COM3\">Serial port COM3</span>",
        "<dt>Send results to</dt><dd>demo (https://lab.example.invalid)</dd>",
        "<input type=\"hidden\" name=\"port\" value=\"name:COM3\">",
        "<form method=\"post\" action=\"/setup/synthetic\">",
        "<button>Save</button>",
    ] {
        assert!(review.contains(shown), "missing {shown}: {review}");
    }
    assert_eq!(std::fs::read_to_string(&fixture.site)?, site_before);
    assert!(
        fixture.backups()?.is_empty(),
        "nothing is written before Save"
    );
    Ok(())
}

// Owner request 2026-11-19 (item 3 with overview item 2): the first-run button
// opens step 1 for the machine waiting for setup, and Add machine when none is
// listed; "first" is a name the setup page keeps for itself.
#[tokio::test(flavor = "multi_thread")]
async fn first_run_button_opens_step_one() -> Result<(), Error> {
    let fixture = start_with(
        "[admin]\nport = 8080\n[[machines]]\nid = 'machine-1'\nsite = 'site.toml'\n",
        include_str!("../../../../../../packaging/linux/starter/machine-1.site.toml"),
    )
    .await?;
    let location = |response: &str| {
        response
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("location: ")
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    };
    let waiting = fixture.get("/setup/first").await?;
    assert!(waiting.starts_with("HTTP/1.1 303"), "{waiting}");
    assert_eq!(location(&waiting), "/setup/machine-1");
    let step = fixture.get("/setup/machine-1").await?;
    assert!(step.contains(STEP_ONE), "{step}");
    let removed = fixture
        .submit("/setup/machine-1/remove", "/setup/machine-1/remove", &[])
        .await?;
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    let none = fixture.get("/setup/first").await?;
    assert_eq!(location(&none), "/setup/add");
    let reserved = fixture
        .submit("/setup/add", "/setup/add", &[("name", "first")])
        .await?;
    assert!(
        reserved.starts_with("HTTP/1.1 409") && reserved.contains("setup page uses itself"),
        "{reserved}"
    );
    Ok(())
}

// Owner request 2026-11-19 (item 3): a step cannot be skipped and hidden fields
// are never trusted. Save and every step check all earlier choices again, and a
// missing or tampered one is refused with the step to fix, before anything is
// written.
#[tokio::test(flavor = "multi_thread")]
async fn skipped_or_tampered_steps_are_refused_with_the_step_to_fix() -> Result<(), Error> {
    let fixture = start().await?;
    let site_before = std::fs::read_to_string(&fixture.site)?;
    let program_before = std::fs::read_to_string(&fixture.config)?;
    let machine = "Fix this in step 1: Machine.";
    let connection = "Fix this in step 2: Connection.";
    for (fields, step, why) in [
        (
            vec![("definition", "")],
            machine,
            "choose one of the listed machine models",
        ),
        (
            vec![("definition", "roche/cobas-e411")],
            machine,
            "is not ready yet",
        ),
        (
            vec![("definition", "../../etc")],
            machine,
            "choose one of the listed machine models",
        ),
        (vec![], connection, "choose a serial port"),
        (
            vec![("definition", "example/network-hl7"), ("host", "not-an-ip")],
            connection,
            "enter an IP address",
        ),
        (
            vec![("port", "name:COM3"), ("lab_app", "")],
            connection,
            "choose the lab app this machine sends results to",
        ),
        (
            vec![("port", "name:COM3"), ("lab_app", "nowhere")],
            connection,
            "there is no lab app called nowhere",
        ),
    ] {
        let body = fixture.form(&fields).await?;
        let refused = fixture.post("/setup/synthetic", &body).await?;
        assert!(refused.starts_with("HTTP/1.1 409"), "{fields:?}: {refused}");
        assert!(refused.contains(step), "{fields:?}: {refused}");
        assert!(refused.contains(why), "{fields:?}: {refused}");
    }
    // A tampered model in a later step's hidden fields: refused, choices kept.
    let tampered = fixture
        .get("/setup/synthetic/test?definition=roche%2Fcobas-e411&port=name%3ACOM3&lab_app=demo")
        .await?;
    assert!(tampered.starts_with("HTTP/1.1 409"), "{tampered}");
    for expected in [
        machine,
        "<form method=\"get\" action=\"/setup/synthetic\">",
        "<input type=\"hidden\" name=\"port\" value=\"name:COM3\">",
        "<button>Go to step 1: Machine</button>",
    ] {
        assert!(
            tampered.contains(expected),
            "missing {expected}: {tampered}"
        );
    }
    let skipped = fixture
        .get("/setup/synthetic/save?definition=example%2Fserial-astm&lab_app=demo")
        .await?;
    assert!(
        skipped.starts_with("HTTP/1.1 409") && skipped.contains(connection),
        "{skipped}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.site)?, site_before);
    assert_eq!(std::fs::read_to_string(&fixture.config)?, program_before);
    assert!(fixture.backups()?.is_empty());
    assert!(fixture.audited("setup-save").await?.is_empty());
    Ok(())
}

/// Starts the listen test for `synthetic` on COM3 and returns the test page once `ready` holds.
async fn listen_until(fixture: &Fixture, ready: impl Fn(&str) -> bool) -> Result<String, Error> {
    let body = fixture.form(&[("port", "name:COM3")]).await?;
    let started = fixture.post("/setup/synthetic/listen", &body).await?;
    assert!(started.starts_with("HTTP/1.1 303"), "{started}");
    let mut view = String::new();
    for _ in 0..200 {
        view = fixture.get("/setup/listen").await?;
        if ready(&view) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(view)
}

// Owner request 2026-11-19 (item 5); CLSI LIS01-A2 §6.2 and §6.3.1.1 (ENQ, then
// STX and a frame number): the test page leads with a plain verdict, keeps the
// bytes as hex and text under a closed Details element, offers the next step,
// and sends nothing to the machine.
#[tokio::test(flavor = "multi_thread")]
async fn test_verdict_comes_first_with_the_bytes_under_details() -> Result<(), Error> {
    let fixture = start().await?;
    *fixture.input.lock().map_err(|_| "poisoned")? = b"\x05\x021H|\\^&|||SYN\r".to_vec();
    let view = listen_until(&fixture, |view| view.contains("looks like ASTM")).await?;
    let verdict = concat!(
        "<p class=\"verdict notice ok\">",
        "Receiving data from the machine, looks like ASTM.</p>"
    );
    let at = view.find(verdict).ok_or("verdict missing")?;
    assert!(view.contains(&current("3", "Test")), "{view}");
    let opened = view
        .find("<details class=\"support raw\">")
        .ok_or("closed details missing")?;
    assert!(at < opened, "the verdict comes before the details");
    let details = view
        .get(opened..)
        .and_then(|rest| rest.split("</details>").next())
        .ok_or("details missing")?;
    assert!(details.contains("05 02 31 48 7c 5c 5e 26"), "{details}");
    assert!(details.contains("..1H|\\^&amp;|||SYN."), "{details}");
    assert!(view.contains("<button>Next: Save</button>"), "{view}");
    assert!(view.contains("<form method=\"get\" action=\"/setup/synthetic/save\">"));
    let open = fixture.get("/setup/listen?details=open").await?;
    assert!(
        open.contains("<details class=\"support raw\" open>"),
        "{open}"
    );
    assert!(
        fixture.written.lock().map_err(|_| "poisoned")?.is_empty(),
        "the test sent bytes to the machine"
    );
    Ok(())
}

// Owner request 2026-11-19 (item 5); MLLP Release 1 §1.2: HL7 from a machine
// whose model speaks ASTM is named, and the mismatch is said plainly.
#[tokio::test(flavor = "multi_thread")]
async fn test_warns_when_the_machine_speaks_another_protocol() -> Result<(), Error> {
    let fixture = start().await?;
    *fixture.input.lock().map_err(|_| "poisoned")? = b"\x0bMSH|^~\\&|SYN|LAB\r".to_vec();
    let view = listen_until(&fixture, |view| view.contains("looks like HL7")).await?;
    let warning = concat!(
        "<p class=\"verdict notice\">Receiving data from the machine. It looks like HL7, ",
        "but this model uses ASTM: check the model or the machine&#39;s setting.</p>"
    );
    assert!(view.contains(warning), "{view}");
    assert!(fixture.written.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

// Owner request 2026-11-19 (item 5): when nothing arrives before the time
// limit, the verdict says so and what to check; nothing was sent, and the test
// can be run again from the same choices.
#[tokio::test(flavor = "multi_thread")]
async fn test_with_no_bytes_says_nothing_received() -> Result<(), Error> {
    let fixture = start_limited(crate::admin::setup::Limits {
        duration: Duration::from_secs(1),
        bytes: 1024,
    })
    .await?;
    fixture.input.lock().map_err(|_| "poisoned")?.clear();
    let view = listen_until(&fixture, |view| view.contains("ended after its time limit")).await?;
    assert!(view.contains("ended after its time limit"), "{view}");
    assert!(
        view.contains(concat!(
            "<p class=\"verdict notice\">Nothing received yet: check the cable and the ",
            "machine&#39;s host settings.</p>"
        )),
        "{view}"
    );
    assert!(view.contains("Bytes received: 0."), "{view}");
    assert!(!view.contains("<details class=\"support raw\""), "{view}");
    assert!(!view.contains("http-equiv=\"refresh\""), "{view}");
    assert!(
        view.contains("<form method=\"post\" action=\"/setup/synthetic/listen\">"),
        "{view}"
    );
    assert!(view.contains("<button>Test again</button>"), "{view}");
    assert!(fixture.written.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}
