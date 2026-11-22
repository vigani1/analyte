//! Tests the restyled Setup pages through the real handlers: the start page, the add form, each step, the live test in every state, the save with Restart now, and their refusals.
use super::*;

/// The fields for `synthetic` on the network example, Analyte listening on `port`.
fn network(port: &str) -> [(&'static str, String); 3] {
    [
        ("definition", "example/network-hl7".to_owned()),
        ("host", "127.0.0.1".to_owned()),
        ("tcp_port", port.to_owned()),
    ]
}

/// A port no socket uses now; free again once the probe closes.
fn free_port() -> Result<String, Error> {
    let probe = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(probe.local_addr()?.port().to_string())
}

impl Fixture {
    /// Starts the listen test for `synthetic` with `fields` and returns the
    /// test page once `ready` holds, or the last page seen.
    pub(super) async fn listen(
        &self,
        fields: &[(&str, &str)],
        ready: impl Fn(&str) -> bool,
    ) -> Result<String, Error> {
        let body = self.form(fields).await?;
        let started = self.post("/setup/synthetic/listen", &body).await?;
        assert!(started.starts_with("HTTP/1.1 303"), "{started}");
        let mut view = String::new();
        for _ in 0..200 {
            view = self.get("/setup/listen").await?;
            if ready(&view) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(view)
    }

    /// A model where Analyte connects to the machine: the HL7 example as a
    /// TCP client.
    fn add_client_model(&self) -> Result<(), Error> {
        let folder = self.root().join("machines/example/client-hl7");
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join("machine.toml"),
            include_str!("../../../../../../core/config/examples/hl7.toml")
                .rewrite("kind = \"tcp_server\"", "kind = \"tcp_client\""),
        )?;
        Ok(())
    }

    /// An absolute folder beside the settings, encoded for a step's query.
    fn incoming(&self) -> String {
        encode(&self.root().join("incoming").display().to_string())
    }

    /// A watched-folder model beside the shipped examples: the HL7 example
    /// reading a folder instead of listening.
    fn add_folder_model(&self) -> Result<(), Error> {
        let folder = self.root().join("machines/example/folder-hl7");
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join("machine.toml"),
            include_str!("../../../../../../core/config/examples/hl7.toml")
                .rewrite("kind = \"tcp_server\"", "kind = \"watched_folder\"")
                .rewrite(
                    "acknowledgment = \"store_and_forward\"
",
                    "",
                ),
        )?;
        Ok(())
    }
}

// Owner decisions 2026-11-20 (admin restyle b4): the start page shows the lab
// apps, each machine with its model, status and settings version and the way
// into its steps, and each shipped model with how it was checked; a model that
// is not ready says why under its status.
#[tokio::test(flavor = "multi_thread")]
async fn start_page_lists_lab_apps_machines_and_models() -> Result<(), Error> {
    let fixture = start().await?;
    let page = fixture.get("/setup").await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    let row = machine_row(&page, "synthetic");
    for expected in [
        "<a href=\"/setup/synthetic\">synthetic</a>",
        "<span class=\"status status--good\">",
        "</svg>Running</span>",
        "<a class=\"btn btn--outline btn--sm\" href=\"/setup/synthetic\">Set up</a>",
        "<a class=\"btn btn--danger-quiet btn--sm\" href=\"/setup/synthetic/remove\">Remove</a>",
    ] {
        assert!(row.contains(expected), "missing {expected}: {row}");
    }
    let version = row
        .split("<span class=\"mono\" title=\"")
        .nth(1)
        .and_then(|rest| rest.split_once("\">"))
        .ok_or("no settings version")?;
    assert_eq!(version.0.len(), 64, "the full digest on hover");
    assert!(version.1.starts_with(version.0.get(..12).ok_or("short")?));
    for expected in [
        "<h2 id=\"st-labs\" class=\"label\">Lab apps</h2>",
        "href=\"/setup/lab-apps\">Open Lab apps</a>",
        "<h2 id=\"st-machines\" class=\"label\">Your machines</h2>",
        "<h2 id=\"st-models\" class=\"label\">Supported machine models</h2>",
        "</svg>Ready</span>",
        "<span class=\"status status--off\">",
        "<div class=\"cell-sub cell-note\">Facts or recordings are still missing; its README lists them.</div>",
    ] {
        assert!(page.contains(expected), "missing {expected}: {page}");
    }
    // The installers' check reads this page with the admin key alone.
    let bearer = request(&fixture.host, "/setup", &fixture.bearer_header()?, None).await?;
    assert!(bearer.starts_with("HTTP/1.1 200"), "{bearer}");
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b4): every refusal and failure of
// the start page stays: no machine listed, settings and models that cannot be
// read, each shown in words in the new look.
#[tokio::test(flavor = "multi_thread")]
async fn start_page_keeps_its_empty_and_unreadable_states() -> Result<(), Error> {
    let fixture = start_with("[admin]\nport = 8080\n", "").await?;
    let empty = fixture.get("/setup").await?;
    assert!(empty.contains("No machine is listed"), "{empty}");
    assert!(empty.contains("Add the first machine; its setup steps follow."));
    let fixture = start().await?;
    std::fs::remove_dir_all(fixture.root().join("machines"))?;
    let no_models = fixture.get("/setup").await?;
    assert!(
        no_models.contains("<p class=\"alert__title\">The machine models could not be listed</p>"),
        "{no_models}"
    );
    let refused = fixture.get("/setup/synthetic").await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("Fix this in step 1: Machine."),
        "{refused}"
    );
    std::fs::write(&fixture.config, "[[machines]\nnot toml")?;
    let unreadable = fixture.get("/setup").await?;
    assert!(unreadable.starts_with("HTTP/1.1 200"), "{unreadable}");
    for expected in [
        "<p class=\"alert__title\">Analyte’s settings could not be read</p>",
        "<p class=\"alert__title\">The lab apps file could not be read</p>",
    ] {
        assert!(
            unreadable.contains(expected),
            "missing {expected}: {unreadable}"
        );
    }
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b4): the add form keeps its fixed
// name rules and the same-analyzer question; an audit that cannot be written
// is shown on the page that says what was added, with Restart now and the
// paths under Details for support; Setup pages that change settings ask to
// sign in first.
#[tokio::test(flavor = "multi_thread")]
async fn add_form_and_its_audit_failure_in_the_new_look() -> Result<(), Error> {
    let fixture = start_two().await?;
    let form = fixture.get("/setup/add").await?;
    for expected in [
        "<ol class=\"crumbs\"><li><a href=\"/setup\">Setup</a></li></ol>",
        "<h1>Add a machine</h1>",
        "name=\"name\" required maxlength=\"64\" value=\"machine-1\"",
        "<input type=\"checkbox\" name=\"same_analyzer\" value=\"yes\">",
        "<a class=\"btn btn--outline\" href=\"/setup\">Cancel</a>",
        "<button type=\"submit\" class=\"btn btn--primary\">Add a machine</button>",
    ] {
        assert!(form.contains(expected), "missing {expected}: {form}");
    }
    fixture.refuse_audits()?;
    let added = fixture
        .submit("/setup/add", "/setup/add", &[("name", "machine-3")])
        .await?;
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    for expected in [
        "<p class=\"alert__title\">machine-3 is added</p>",
        "It is listed and not set up yet.",
        "<p class=\"alert__title\">The record of who made this change could not be written: ",
        "<a class=\"btn btn--primary\" href=\"/setup/machine-3\">Set it up now</a>",
        "Restart now",
        "A dated copy of the old settings was kept.",
        "Its settings file: ",
        "Copy of the old settings: ",
    ] {
        assert!(added.contains(expected), "missing {expected}: {added}");
    }
    for path in [
        "/setup/add",
        "/setup/synthetic",
        "/setup/listen",
        "/setup/synthetic/save",
    ] {
        let page = request(&fixture.host, path, &fixture.bearer_header()?, None).await?;
        assert!(page.starts_with("HTTP/1.1 403"), "{path}: {page}");
        assert!(
            page.contains("Setup changes settings, so its pages need a signed-in browser session"),
            "{path}: {page}"
        );
    }
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b4); Setup mockups: each step has
// the stepper, the design's choices and fields, Back and Next. A port another
// running machine holds is shown In use and cannot be picked; the machine's
// name is fixed; the lab app is picked here; values are escaped.
#[tokio::test(flavor = "multi_thread")]
async fn steps_draw_the_design_choices_and_fields() -> Result<(), Error> {
    let fixture = start_two().await?;
    let first = fixture.get("/setup/machine-2").await?;
    for expected in [
        "<p class=\"eyebrow\">Set up a machine</p><h1>machine-2</h1>",
        "<a class=\"btn btn--quiet\" href=\"/setup\">Cancel</a>",
        "<h2 id=\"s1\" class=\"wizard__title\">Which machine is it?</h2>",
        "<button type=\"button\" class=\"btn btn--outline\" disabled>Back</button><span class=\"subtle wizard__count\">Step 1 of 4</span>",
        "value=\"roche/cobas-e411\" required disabled>",
        "title=\"Facts or recordings are still missing; its README lists them.\">",
    ] {
        assert!(first.contains(expected), "missing {expected}: {first}");
    }
    let second = fixture
        .get("/setup/machine-2/connection?definition=example%2Fserial-astm")
        .await?;
    for expected in [
        "<p class=\"page-head__lede\">Example serial astm</p>",
        "<li class=\"step step--done\"><span class=\"step__n\"><svg",
        "</svg>Step 1 · done</span>",
        "value=\"usb:A10K3XYZ\" disabled><span><span class=\"choice__title mono\">COM12</span>",
        "– FT232R USB UART – used by synthetic</span></span><span class=\"status status--off\">",
        "</svg>In use</span>",
        "<input id=\"s2-name\" class=\"input\" value=\"machine-2\" readonly>",
        "Step 2 of 4",
    ] {
        assert!(second.contains(expected), "missing {expected}: {second}");
    }
    assert!(
        !second.contains("value=\"usb:A10K3XYZ\" checked"),
        "{second}"
    );
    let listening = fixture
        .get("/setup/machine-2/connection?definition=example%2Fnetwork-hl7")
        .await?;
    assert!(listening.contains("The machine connects to this PC over the lab network."));
    assert!(
        listening
            .contains("<legend class=\"field__label wizard__legend\">This PC’s address</legend>")
    );
    fixture.add_folder_model()?;
    let folder = fixture
        .get("/setup/machine-2/connection?definition=example%2Ffolder-hl7")
        .await?;
    assert!(
        folder.contains("Analyte reads its results from a folder."),
        "{folder}"
    );
    assert!(folder.contains("name=\"folder\" required"), "{folder}");
    let hostile = fixture
        .get("/setup/machine-2?definition=example%2Fserial-astm&port=usb%3A%22%3E%3Cscript%3E")
        .await?;
    assert!(
        hostile.contains(
            "<input type=\"hidden\" name=\"port\" value=\"usb:&#34;&#62;&#60;script&#62;\">"
        ),
        "{hostile}"
    );
    assert!(!other_script(&hostile), "{hostile}");
    std::fs::write(
        fixture.root().join("lab-apps.toml"),
        "[[lab_apps]]\nunknown = 1\n",
    )?;
    let unreadable = fixture
        .get("/setup/machine-2/connection?definition=example%2Fserial-astm")
        .await?;
    assert!(
        unreadable.contains("<p class=\"alert__title\">The lab apps could not be read</p>"),
        "{unreadable}"
    );
    Ok(())
}

// Owner request 2026-11-21 (network setup): when the machine connects to
// Analyte, step 2 lists this PC's addresses with their adapter and every
// network card, none chosen until the lab manager picks one, and says what to
// type into the machine; a saved address the PC no longer has, or this PC
// only, stays visible and chosen. When Analyte connects, step 2 asks for the
// machine's own address and port.
#[tokio::test(flavor = "multi_thread")]
async fn network_step_lists_this_pcs_addresses_and_says_what_to_type() -> Result<(), Error> {
    let fixture = start_two().await?;
    let step = "/setup/machine-2/connection?definition=example%2Fnetwork-hl7";
    let fresh = fixture.get(step).await?;
    let radio = |value: &str, checked: bool, meta: &str| {
        format!(
            "<input type=\"radio\" name=\"host\" value=\"{value}\" required{}><span><span class=\"choice__title mono\">{value}</span> <span class=\"choice__meta\">– {meta}</span>",
            if checked { " checked" } else { "" }
        )
    };
    let every = "every network card of this PC";
    for expected in [
        radio("192.168.1.20", false, "Ethernet"),
        radio("10.0.0.7", false, "Wi-Fi"),
        radio("0.0.0.0", false, every),
        "type that address and the port into the machine’s host (LIS) settings.".to_owned(),
        "Type the chosen address into the machine’s host (LIS) address setting.".to_owned(),
        "Type the same port number into the machine’s host (LIS) settings.".to_owned(),
    ] {
        assert!(fresh.contains(&expected), "missing {expected}: {fresh}");
    }
    // This PC's addresses come first, every network card last.
    let order: Vec<usize> = ["192.168.1.20", "10.0.0.7", "0.0.0.0"]
        .iter()
        .filter_map(|ip| fresh.find(&format!("value=\"{ip}\"")))
        .collect();
    assert!(order.len() == 3 && order.is_sorted(), "{fresh}");
    assert!(!fresh.contains("id=\"s2-host\""), "{fresh}");
    let picked = fixture.get(&format!("{step}&host=10.0.0.7")).await?;
    assert!(
        picked.contains(&radio("10.0.0.7", true, "Wi-Fi")),
        "{picked}"
    );
    assert!(picked.contains(&radio("0.0.0.0", false, every)), "{picked}");
    let gone = fixture.get(&format!("{step}&host=172.16.0.9")).await?;
    assert!(
        gone.contains(&radio(
            "172.16.0.9",
            true,
            "saved before; not an address of this PC now"
        )),
        "{gone}"
    );
    let local = fixture.get(&format!("{step}&host=127.0.0.1")).await?;
    assert!(
        local.contains(&radio(
            "127.0.0.1",
            true,
            "this PC only: no machine on the network can reach it"
        )),
        "{local}"
    );
    fixture.add_client_model()?;
    let client = fixture
        .get("/setup/machine-2/connection?definition=example%2Fclient-hl7&host=192.168.1.50&tcp_port=5100")
        .await?;
    for expected in [
        "Analyte connects to the machine over the lab network.",
        "<label class=\"field__label\" for=\"s2-host\">Machine’s IP address</label><input id=\"s2-host\" class=\"input mono\" name=\"host\" required maxlength=\"64\" value=\"192.168.1.50\">",
        "<label class=\"field__label\" for=\"s2-tcp\">Machine’s port</label>",
    ] {
        assert!(client.contains(expected), "missing {expected}: {client}");
    }
    assert!(!client.contains("type=\"radio\" name=\"host\""), "{client}");
    Ok(())
}

// Owner request 2026-11-21 (network setup), item 4: when Analyte connects to
// the machine, step 3 says it connects (not listens), and a test that cannot
// reach the machine, or reaches one that sends nothing, says what to check.
#[tokio::test(flavor = "multi_thread")]
async fn test_step_for_a_machine_analyte_connects_to() -> Result<(), Error> {
    let fixture = start().await?;
    fixture.add_client_model()?;
    let closed = free_port()?;
    let before = fixture
        .get(&format!(
            "/setup/synthetic/test?definition=example%2Fclient-hl7&host=127.0.0.1&tcp_port={closed}&lab_app=demo"
        ))
        .await?;
    for expected in [
        "Analyte connects to the machine and only reads.",
        "<p class=\"eyebrow listen__target\">Connects to</p>",
        &format!("Analyte connects to the machine at 127.0.0.1:{closed}"),
        "It cannot test a machine the running service already connects to.",
    ] {
        assert!(before.contains(expected), "missing {expected}: {before}");
    }
    let fields = [
        ("definition", "example/client-hl7"),
        ("host", "127.0.0.1"),
        ("tcp_port", closed.as_str()),
    ];
    let refused = fixture
        .listen(&fields, |view| view.contains("could not run"))
        .await?;
    for expected in [
        "<p class=\"alert__title\">The test could not run</p>",
        "Check the machine’s address and port, that it is switched on and on the network",
        "Analyte connects to the machine and only reads. It sends nothing to the machine.",
        "<p class=\"eyebrow listen__target\">Connecting to 127.0.0.1:",
    ] {
        assert!(refused.contains(expected), "missing {expected}: {refused}");
    }
    // A machine that accepts the connection and sends nothing.
    let machine = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = machine.local_addr()?.port().to_string();
    let fields = [
        ("definition", "example/client-hl7"),
        ("host", "127.0.0.1"),
        ("tcp_port", port.as_str()),
    ];
    let silent = fixture
        .listen(&fields, |view| view.contains("Connected, nothing"))
        .await?;
    assert!(
        silent.contains("<p class=\"alert__title\">Connected, nothing received yet</p>"),
        "{silent}"
    );
    assert!(
        silent.contains("The machine connected but sent nothing."),
        "{silent}"
    );
    drop(machine);
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b4); SetupStep3 mockups: before a
// test, step 3 says what it listens on and starts it; a watched folder has
// nothing to test. While it runs, the page counts the bytes and says what it
// observed in the words used for a machine's link ("Listening on port N,
// nothing connected since …"); a test that cannot run says so; with nothing
// started the page says so.
#[tokio::test(flavor = "multi_thread")]
async fn test_step_says_what_it_observed_in_each_state() -> Result<(), Error> {
    let fixture = start().await?;
    let none = fixture.get("/setup/listen").await?;
    assert!(none.starts_with("HTTP/1.1 200"), "{none}");
    assert!(none.contains("<p class=\"alert__title\">No test has been started</p>"));
    let before = fixture
        .get("/setup/synthetic/test?definition=example%2Fserial-astm&port=name%3ACOM3&lab_app=demo")
        .await?;
    for expected in [
        "<p class=\"eyebrow listen__target\">Listens on</p>",
        "<span title=\"endpoint = COM3\">Serial port COM3</span>",
        "For up to 20 seconds.",
        "<form class=\"card card--pad listen\" method=\"post\" action=\"/setup/synthetic/listen\">",
        "<button type=\"submit\" class=\"btn btn--primary\">Start the test</button>",
        "<button type=\"submit\" class=\"btn btn--outline\">Next</button>",
    ] {
        assert!(before.contains(expected), "missing {expected}: {before}");
    }
    fixture.add_folder_model()?;
    let folder = fixture
        .get(&format!(
            "/setup/synthetic/test?definition=example%2Ffolder-hl7&folder={}&lab_app=demo",
            fixture.incoming()
        ))
        .await?;
    assert!(
        folder.contains("Nothing to test for a watched folder"),
        "{folder}"
    );
    assert!(folder.contains("<button type=\"submit\" class=\"btn btn--primary\">Next</button>"));
    let serial = fixture
        .listen(&[("port", "name:COM3")], |view| {
            view.contains("Port COM3 open since")
        })
        .await?;
    for expected in [
        // ADR 0031: the test names the cable settings it opens the port with.
        "<p class=\"eyebrow listen__target\">Serial port COM3, 9600 baud, 8 data bits, no parity, 1 stop bit, no flow control</p>",
        "<p class=\"countdown listen__clock\" aria-live=\"polite\"><span class=\"sr-only\">Time left </span>0:",
        "<progress class=\"meter\" max=\"20\" value=\"",
        "</svg>Connected</span> Port COM3 open since ",
        "<form method=\"post\" action=\"/setup/listen/stop\">",
    ] {
        assert!(serial.contains(expected), "missing {expected}: {serial}");
    }
    let port = free_port()?;
    let fields = network(&port);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let waiting = fixture
        .listen(&pairs, |view| view.contains("nothing connected since"))
        .await?;
    assert!(
        waiting.contains(&format!(
            "</svg>Waiting</span> Listening on port {port}, nothing connected since "
        )),
        "{waiting}"
    );
    assert!(waiting.contains("<p class=\"eyebrow listen__target\">Listening on 127.0.0.1:"));
    // Owner request 2026-11-21 (network setup): nothing connected says where
    // Analyte listens, what to set on the machine and that a firewall may block it.
    for expected in [
        "<p class=\"alert__title\">The machine has not connected yet</p>".to_owned(),
        format!(
            "Analyte listens on 127.0.0.1, port {port}. Set the machine’s host (LIS) address to 127.0.0.1 and its host port to {port}"
        ),
        "If it still does not connect, a firewall may block it".to_owned(),
    ] {
        assert!(waiting.contains(&expected), "missing {expected}: {waiting}");
    }
    let held = std::net::TcpListener::bind("127.0.0.1:0")?;
    let taken = held.local_addr()?.port().to_string();
    let fields = network(&taken);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let failed = fixture
        .listen(&pairs, |view| view.contains("could not run"))
        .await?;
    for expected in [
        "<p class=\"alert__title\">The test could not run</p>",
        "Check that the address is one of this PC’s and that no other program uses the port, or go back and choose another.",
        "<span class=\"status status--bad\">",
        &format!("Not listening on port {taken} since "),
        "<button type=\"submit\" class=\"btn btn--outline\">Test again</button>",
    ] {
        assert!(failed.contains(expected), "missing {expected}: {failed}");
    }
    drop(held);
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b4); SetupStep4 mockup: step 4
// says what will be saved, the lab app with the keys it still lacks, and the
// last test with exactly these choices; once saved it says so with Restart
// now and Later, a dated copy under Details for support, and an audit that
// could not be written; Done goes to the machine's page.
#[tokio::test(flavor = "multi_thread")]
async fn save_step_says_what_was_tested_and_saved() -> Result<(), Error> {
    let fixture = start().await?;
    let path =
        "/setup/synthetic/save?definition=example%2Fserial-astm&port=name%3ACOM3&lab_app=demo";
    let review = fixture.get(path).await?;
    for expected in [
        "<h2 id=\"s4\" class=\"wizard__title\">Check and save</h2>",
        "Keys missing: API key and signing key. Waiting items go out after Analyte restarts with the keys.",
        "<dt>Connection test</dt><dd><span class=\"subtle\">Not run with these settings</span>",
        "<span class=\"subtle wizard__count\">Step 4 of 4</span>",
    ] {
        assert!(review.contains(expected), "missing {expected}: {review}");
    }
    *fixture.input.lock().map_err(|_| "poisoned")? = b"\x05\x021H|\\^&\r".to_vec();
    let view = fixture
        .listen(&[("port", "name:COM3")], |view| {
            view.contains("looks like ASTM")
        })
        .await?;
    assert!(view.contains("looks like ASTM"), "{view}");
    let tested = fixture.get(path).await?;
    assert!(
        tested.contains("<dt>Connection test</dt><dd><span class=\"with-icon tone-good\">"),
        "{tested}"
    );
    assert!(
        tested.contains("data, looks like ASTM · <time datetime=\""),
        "{tested}"
    );
    assert!(tested.contains(" · <time datetime=\""), "{tested}");
    let other = fixture
        .get("/setup/synthetic/save?definition=example%2Fserial-astm&port=usb%3AA10K3XYZ&lab_app=demo")
        .await?;
    assert!(other.contains("Not run with these settings"), "{other}");
    fixture.add_folder_model()?;
    let folder = fixture
        .get(&format!(
            "/setup/synthetic/save?definition=example%2Ffolder-hl7&folder={}&lab_app=demo",
            fixture.incoming()
        ))
        .await?;
    assert!(
        folder.contains("Nothing to test for a watched folder"),
        "{folder}"
    );
    fixture.refuse_audits()?;
    let body = fixture.form(&[("port", "name:COM3")]).await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    for expected in [
        "<p class=\"alert__title\">Settings saved</p>",
        "<a class=\"btn btn--quiet btn--sm\" href=\"/setup\">Later</a>",
        "<form method=\"post\" action=\"/setup/restart\">",
        "Restart now</button>",
        "A dated copy of the old settings was kept.",
        "Copy of the old settings: ",
        "<p class=\"alert__title\">The record of who made this change could not be written: ",
        "<a class=\"btn btn--primary\" href=\"/machines/synthetic\">Done</a>",
        "<dt>New settings version</dt><dd><span class=\"mono\" title=\"",
        "</svg>Step 4 · done</span>",
    ] {
        assert!(saved.contains(expected), "missing {expected}: {saved}");
    }
    assert!(
        !saved.contains("aria-current=\"step\""),
        "all four are done"
    );
    assert!(
        !saved.contains("btn btn--quiet\" href=\"/setup\">Cancel"),
        "{saved}"
    );
    Ok(())
}

// Owner decisions 2026-11-20 (admin restyle b4): an unknown machine and a form
// that cannot be read are refused in the new look; the unknown machine has no
// setup steps to link back to. A refusal reads as a sentence, but a model key or
// machine name in it keeps its case (regression: "Acme/bg-3" in the b4 shots).
#[tokio::test(flavor = "multi_thread")]
async fn unknown_machines_and_unreadable_forms_are_refused() -> Result<(), Error> {
    let fixture = start().await?;
    let unknown = fixture.get("/setup/nowhere").await?;
    assert!(unknown.starts_with("HTTP/1.1 409"), "{unknown}");
    assert!(
        unknown.contains("No installed machine is called nowhere."),
        "{unknown}"
    );
    assert!(!unknown.contains("href=\"/setup/nowhere\""), "{unknown}");
    let blocked = fixture
        .get("/setup/synthetic/test?definition=roche%2Fcobas-e411&port=name%3ACOM3&lab_app=demo")
        .await?;
    assert!(
        blocked.contains("Model roche/cobas-e411 is not ready yet"),
        "{blocked}"
    );
    let body = format!("{}&surprise=1", fixture.form(&[]).await?);
    let incomplete = fixture.post("/setup/synthetic", &body).await?;
    assert!(incomplete.starts_with("HTTP/1.1 409"), "{incomplete}");
    assert!(
        incomplete.contains("The form was incomplete; start again from the setup page."),
        "{incomplete}"
    );
    Ok(())
}

// Owner request 2026-11-18 (setup page): Restart now answers in the new look
// and reloads Setup once the service manager has started Analyte again.
#[tokio::test(flavor = "multi_thread")]
async fn restart_answer_reloads_setup() -> Result<(), Error> {
    let fixture = start().await?;
    let answer = fixture.submit("/setup/add", "/setup/restart", &[]).await?;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    for expected in [
        "<meta http-equiv=\"refresh\" content=\"15;url=/setup\">",
        "<h1>Restarting</h1>",
        "<p class=\"alert__title\">Analyte is stopping</p>",
    ] {
        assert!(answer.contains(expected), "missing {expected}: {answer}");
    }
    Ok(())
}

// ADR 0026 decision 2: step 3 shows the device that connected; step 4 offers
// "Only accept this device" with its address, ticked; saving with the box
// ticked writes it as `allowed_peers`, and saving with it unticked removes
// the key and says that any device is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn save_offers_and_writes_the_device_seen_in_the_test() -> Result<(), Error> {
    let fixture = start().await?;
    let port = free_port()?;
    let fields = network(&port);
    let fields: Vec<(&str, &str)> = fields.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let address = format!("127.0.0.1:{port}");
    let machine = async {
        for _ in 0..200 {
            if let Ok(mut socket) = tokio::net::TcpStream::connect(&address).await {
                socket.write_all(b"MSH|").await?;
                return Ok::<_, Error>(socket);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err("the test never listened".into())
    };
    let (view, socket) = tokio::join!(
        fixture.listen(&fields, |view| view.contains("Connected from")),
        machine
    );
    let (view, _socket) = (view?, socket?);
    assert!(
        view.contains("<p class=\"note\">Connected from <span class=\"mono\">127.0.0.1</span></p>"),
        "{view}"
    );
    let review = fixture
        .get(&format!(
            "/setup/synthetic/save?definition=example%2Fnetwork-hl7&host=127.0.0.1&tcp_port={port}&lab_app=demo"
        ))
        .await?;
    for expected in [
        "<dt>Accepts connections from</dt><dd><label class=\"check\"><input type=\"checkbox\" name=\"allowed_peers\" value=\"127.0.0.1\" form=\"s4-save\" checked><span>Only accept this device: 127.0.0.1</span></label>",
        "Check that this is the machine’s own address in its network settings.",
        "<form id=\"s4-save\" method=\"post\" action=\"/setup/synthetic\">",
    ] {
        assert!(review.contains(expected), "missing {expected}: {review}");
    }
    let save_form = review
        .split("<form id=\"s4-save\"")
        .nth(1)
        .and_then(|form| form.split("</form>").next())
        .ok_or("save form missing")?;
    assert!(
        !save_form.contains("allowed_peers"),
        "only the tick box sends it: {save_form}"
    );
    let mut ticked = fields.clone();
    ticked.push(("allowed_peers", "127.0.0.1"));
    let saved = fixture
        .post("/setup/synthetic", &fixture.form(&ticked).await?)
        .await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(
        saved.contains(
            "<dt>Accepts connections from</dt><dd><span class=\"mono\">Only 127.0.0.1</span>"
        ),
        "{saved}"
    );
    let site: toml::Table = std::fs::read_to_string(&fixture.site)?.parse()?;
    assert_eq!(
        site.get("allowed_peers"),
        Some(&toml::Value::Array(vec![toml::Value::String(
            "127.0.0.1".into()
        )]))
    );
    let unticked = fixture
        .post("/setup/synthetic", &fixture.form(&fields).await?)
        .await?;
    assert!(
        unticked
            .contains("<dd><span class=\"tone-warn\">Accepts connections from any device</span>"),
        "{unticked}"
    );
    let site: toml::Table = std::fs::read_to_string(&fixture.site)?.parse()?;
    assert!(site.get("allowed_peers").is_none(), "{site:?}");
    Ok(())
}

// ADR 0026 decision 2: step 2 shows the saved devices to change, and a typed
// list is checked once like the site file: a range is refused with the step
// to fix, and a machine that does not listen cannot be limited.
#[tokio::test(flavor = "multi_thread")]
async fn typed_devices_are_kept_and_checked() -> Result<(), Error> {
    let fixture = start().await?;
    let port = free_port()?;
    let step = format!(
        "/setup/synthetic/connection?definition=example%2Fnetwork-hl7&host=127.0.0.1&tcp_port={port}&lab_app=demo&allowed_peers=192.168.1.50%2C+192.168.1.51"
    );
    let typed = fixture.get(&step).await?;
    assert!(
        typed.contains(
            "name=\"allowed_peers\" maxlength=\"1024\" value=\"192.168.1.50, 192.168.1.51\""
        ),
        "{typed}"
    );
    let review = fixture
        .get(&format!(
            "/setup/synthetic/save?definition=example%2Fnetwork-hl7&host=127.0.0.1&tcp_port={port}&lab_app=demo&allowed_peers=192.168.1.50%2C192.168.1.51"
        ))
        .await?;
    assert!(
        review.contains("value=\"192.168.1.50, 192.168.1.51\" form=\"s4-save\" checked><span>Only accept these devices: 192.168.1.50, 192.168.1.51</span>"),
        "{review}"
    );
    let range = fixture
        .get(&format!(
            "/setup/synthetic/save?definition=example%2Fnetwork-hl7&host=127.0.0.1&tcp_port={port}&lab_app=demo&allowed_peers=192.168.1.0%2F24"
        ))
        .await?;
    assert!(range.starts_with("HTTP/1.1 409"), "{range}");
    assert!(range.contains("without a name, port or range"), "{range}");
    assert!(range.contains("Go to step 2: Connection"), "{range}");
    let serial = fixture
        .get("/setup/synthetic/save?definition=example%2Fserial-astm&port=name%3ACOM3&lab_app=demo&allowed_peers=192.168.1.50")
        .await?;
    assert!(serial.starts_with("HTTP/1.1 409"), "{serial}");
    assert!(
        serial.contains("Only a machine that connects to this PC can be limited"),
        "{serial}"
    );
    Ok(())
}
