//! Tests step 2's serial cable settings (ADR 0031): filled in from the machine definition's suggestion, chosen from the standard lists, saved in the lab's site file, used by the connection test and refused when missing or off the list.
use super::*;

/// The settings version the program file loads now for its only machine.
fn digest(config: &std::path::Path) -> Result<[u8; 32], Error> {
    Ok(Program::load(config)?
        .machines()
        .first()
        .ok_or("machine missing")?
        .snapshot()
        .digest())
}

/// The serial example's definition beside the fixture's settings.
fn definition(fixture: &Fixture) -> std::path::PathBuf {
    fixture
        .root()
        .join("machines/example/serial-astm/machine.toml")
}

// ADR 0031 decision 1: step 2 offers each cable setting from its standard
// list, chosen as the machine definition suggests, with the hint to match the
// analyzer's screen; nothing differs yet.
#[tokio::test(flavor = "multi_thread")]
async fn step_two_fills_the_cable_settings_from_the_definition() -> Result<(), Error> {
    let fixture = start().await?;
    let page = fixture
        .get("/setup/synthetic/connection?definition=example/serial-astm")
        .await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    for expected in [
        "Match the analyzer’s LIS/host settings screen.",
        "<select id=\"s2-baud\" class=\"select\" name=\"baud\" required>",
        "<option value=\"9600\" selected>9600</option>",
        "<option value=\"115200\">115200</option>",
        "<option value=\"8\" selected>8</option>",
        "<option value=\"none\" selected>None</option>",
        "<option value=\"even\">Even</option>",
        "<option value=\"1\" selected>1</option>",
        "<option value=\"hardware\">Hardware (RTS/CTS)</option>",
        "The manual suggests 9600.",
        "The manual suggests None.",
    ] {
        assert!(page.contains(expected), "missing {expected}: {page}");
    }
    assert!(!page.contains("14400"), "only the standard list is offered");
    assert!(!page.contains("differs from the manual"), "{page}");
    assert!(!page.contains(">Choose</option>"), "{page}");
    // A value the definition does not suggest starts at "Choose".
    std::fs::write(
        definition(&fixture),
        include_str!("../../../../../../core/config/examples/astm.toml")
            .rewrite("flow_control = \"none\"\n", ""),
    )?;
    let page = fixture
        .get("/setup/synthetic/connection?definition=example/serial-astm")
        .await?;
    assert!(
        page.contains("<option value=\"\" selected>Choose</option>"),
        "{page}"
    );
    assert!(
        page.contains("The machine’s definition suggests none: read it on the analyzer’s screen."),
        "{page}"
    );
    Ok(())
}

// ADR 0031 decisions 2 and 6: a changed value is saved in the lab's site file
// with every other value, step 2 and the summary mark what differs from the
// manual, the audit names it, and the settings version changes.
#[tokio::test(flavor = "multi_thread")]
async fn saving_a_changed_value_keeps_it_in_the_site_file() -> Result<(), Error> {
    let fixture = start().await?;
    let before = digest(&fixture.config)?;
    let review = fixture
        .get("/setup/synthetic/save?definition=example%2Fserial-astm&port=usb%3AA10K3XYZ&lab_app=demo&baud=19200&parity=even")
        .await?;
    assert!(
        review.contains(concat!(
            "<dt>Cable settings</dt><dd>19200 baud, 8 data bits, even parity, 1 stop bit, ",
            "no flow control; differs from the manual&#39;s suggestion: baud rate 19200 ",
            "(the manual suggests 9600), parity even (the manual suggests none)</dd>"
        )),
        "{review}"
    );
    let body = fixture
        .form(&[
            ("port", "usb:A10K3XYZ"),
            ("baud", "19200"),
            ("data_bits", "8"),
            ("parity", "even"),
            ("stop_bits", "1"),
            ("flow_control", "none"),
        ])
        .await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(
        saved.contains("<dt>Cable settings</dt><dd>19200 baud"),
        "{saved}"
    );
    let site: toml::Table = std::fs::read_to_string(&fixture.site)?.parse()?;
    let serial = site.get("serial").ok_or("no [serial] in the site file")?;
    assert_eq!(
        serial,
        &toml::Value::Table(
            "baud = 19200\ndata_bits = 8\nparity = \"even\"\nstop_bits = 1\nflow_control = \"none\"\n"
                .parse()?
        )
    );
    let definition_text = std::fs::read_to_string(definition(&fixture))?;
    assert!(
        definition_text.contains("baud = 9600"),
        "the definition is never written"
    );
    let after = digest(&fixture.config)?;
    assert_ne!(
        after, before,
        "a changed cable setting is a new settings version"
    );
    let audits = fixture.audited("setup-save").await?;
    assert!(
        audits
            .iter()
            .any(|reason| reason.contains("cable 19200 baud")),
        "{audits:?}"
    );
    // Step 2 now shows the saved choice and marks what differs.
    let page = fixture
        .get("/setup/synthetic/connection?definition=example/serial-astm")
        .await?;
    assert!(
        page.contains("<option value=\"19200\" selected>19200</option>"),
        "{page}"
    );
    assert!(
        page.contains("Baud rate <span class=\"tone-warn\">· differs from the manual</span>"),
        "{page}"
    );
    assert!(
        page.contains("Data bits</label>"),
        "an unchanged value is not marked: {page}"
    );
    Ok(())
}

// ADR 0031 decision 4: a value outside the standard list, or an unknown word,
// is refused at step 2 and changes no file.
#[tokio::test(flavor = "multi_thread")]
async fn a_value_outside_the_list_is_refused() -> Result<(), Error> {
    let fixture = start().await?;
    let before = std::fs::read_to_string(&fixture.site)?;
    for (field, value, words) in [
        ("baud", "14400", "use a standard baud rate"),
        ("data_bits", "9", "use 5 through 8 data bits"),
        ("parity", "mark", "choose the parity: none, odd or even"),
        ("flow_control", "dtr", "choose the flow control"),
    ] {
        let body = fixture
            .form(&[("port", "usb:A10K3XYZ"), (field, value)])
            .await?;
        let refused = fixture.post("/setup/synthetic", &body).await?;
        assert!(refused.starts_with("HTTP/1.1 409"), "{field}: {refused}");
        assert!(
            refused.contains("Fix this in step 2: Connection."),
            "{refused}"
        );
        assert!(refused.to_lowercase().contains(words), "{field}: {refused}");
    }
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);
    assert!(fixture.backups()?.is_empty());
    Ok(())
}

// ADR 0031 decision 3, AGENTS.md "never invents data": when the definition
// suggests no value and the lab chose none, the save is refused by the
// setting's name; once chosen it saves.
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_value_is_refused_by_name() -> Result<(), Error> {
    let fixture = start().await?;
    std::fs::write(
        definition(&fixture),
        include_str!("../../../../../../core/config/examples/astm.toml")
            .rewrite("flow_control = \"none\"\n", ""),
    )?;
    let before = std::fs::read_to_string(&fixture.site)?;
    let body = fixture
        .form(&[("port", "usb:A10K3XYZ"), ("flow_control", "")])
        .await?;
    let refused = fixture.post("/setup/synthetic", &body).await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("transport.serial.flow_control: no value"),
        "{refused}"
    );
    assert!(
        refused.contains("Fix this in step 2: Connection."),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);
    let body = fixture
        .form(&[("port", "usb:A10K3XYZ"), ("flow_control", "hardware")])
        .await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(
        saved.contains("flow control hardware (the machine definition suggests none)"),
        "{saved}"
    );
    Ok(())
}

// ADR 0031 decision 1: the listen-only test opens the fake port with the
// values chosen in step 2, not the definition's.
#[tokio::test(flavor = "multi_thread")]
async fn the_connection_test_uses_the_chosen_values() -> Result<(), Error> {
    let fixture = start().await?;
    let view = fixture
        .listen(
            &[
                ("port", "name:COM3"),
                ("baud", "19200"),
                ("data_bits", "7"),
                ("parity", "even"),
                ("stop_bits", "2"),
                ("flow_control", "software"),
            ],
            |view| view.contains("Port COM3 open since"),
        )
        .await?;
    assert!(
        view.contains("Serial port COM3, 19200 baud, 7 data bits, even parity, 2 stop bits, software flow control (XON/XOFF)"),
        "{view}"
    );
    let opened = fixture.opened.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(opened.len(), 1, "{opened:?}");
    let settings = opened.first().ok_or("not opened")?;
    for expected in [
        "baud: 19200",
        "bits: Seven",
        "parity: Even",
        "stops: Two",
        "flow: Software",
    ] {
        assert!(
            settings.contains(expected),
            "missing {expected}: {settings}"
        );
    }
    Ok(())
}

// ADR 0031 decision 5: a network machine has no cable settings; typing them
// for one is refused rather than ignored.
#[tokio::test(flavor = "multi_thread")]
async fn a_network_machine_takes_no_cable_settings() -> Result<(), Error> {
    let fixture = start().await?;
    let network = fixture
        .get("/setup/synthetic/connection?definition=example/network-hl7")
        .await?;
    assert!(!network.contains("name=\"baud\""), "{network}");
    let refused = fixture
        .get("/setup/synthetic/test?definition=example%2Fnetwork-hl7&host=127.0.0.1&tcp_port=5600&lab_app=demo&baud=9600")
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("Only a machine on a serial cable has cable settings"),
        "{refused}"
    );
    Ok(())
}
