//! Tests step 2's analyzer settings (ADR 0032): one list per choice the selected model declares, filled in from the suggestion, saved in the lab's site file, carried through every step and refused when missing or not offered.
use super::*;

/// The choices model beside the fixture's settings: the VIDAS teaching
/// definition, whose software version only the lab can choose.
fn with_choices_model(fixture: &Fixture) -> Result<(), Error> {
    let folder = fixture.root().join("machines/example/vidas-choices");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("machine.toml"),
        include_str!("../../../../../../core/config/examples/vidas_choices.toml"),
    )?;
    Ok(())
}

/// A save form for the choices model, listening on loopback, with `fields`.
async fn choices_form(fixture: &Fixture, fields: &[(&str, &str)]) -> Result<String, Error> {
    let mut all = vec![
        ("definition", "example/vidas-choices"),
        ("host", "127.0.0.1"),
        ("tcp_port", "5601"),
        ("allowed_peers", ""),
    ];
    all.extend_from_slice(fields);
    fixture.form(&all).await
}

// ADR 0032 decision 1: step 2 asks exactly the choices the selected model
// declares, each from its own list, starting at the manual's suggestion and
// at "Choose" where the definition suggests none; a model without choices
// shows none.
#[tokio::test(flavor = "multi_thread")]
async fn step_two_asks_the_models_own_choices() -> Result<(), Error> {
    let fixture = start().await?;
    with_choices_model(&fixture)?;
    let page = fixture
        .get("/setup/synthetic/connection?definition=example/vidas-choices")
        .await?;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    for expected in [
        "<legend class=\"field__label wizard__legend\">Analyzer settings</legend>",
        "<label class=\"field__label\" for=\"s2-choice-protocol_mode\">Protocol on the host link screen</label>",
        "<select id=\"s2-choice-protocol_mode\" class=\"select\" name=\"choice.protocol_mode\" required>",
        "<option value=\"literal\" selected>Literal (acknowledge after the checksum)</option>",
        "<option value=\"alternate\">Alternate (acknowledge after ETX)</option>",
        "Match the setting on the analyzer’s own screen. The manual suggests Literal (acknowledge after the checksum).",
        "<select id=\"s2-choice-software_version\" class=\"select\" name=\"choice.software_version\" required><option value=\"\" selected>Choose</option>",
        "Read the version on the analyzer’s About screen. The machine’s definition suggests none.",
    ] {
        assert!(page.contains(expected), "missing {expected}: {page}");
    }
    let serial = fixture
        .get("/setup/synthetic/connection?definition=example/serial-astm")
        .await?;
    assert!(!serial.contains("Analyzer settings"), "{serial}");
    // The next steps carry the choices as hidden fields.
    let test = fixture
        .get("/setup/synthetic/test?definition=example%2Fvidas-choices&host=127.0.0.1&tcp_port=5601&lab_app=demo&choice.software_version=v2")
        .await?;
    assert!(test.starts_with("HTTP/1.1 200"), "{test}");
    assert!(
        test.contains("<input type=\"hidden\" name=\"choice.software_version\" value=\"v2\">"),
        "{test}"
    );
    Ok(())
}

// ADR 0032 (2026-11-26): the unchecked option can be selected and saved with
// the warning visible alongside it.
#[tokio::test(flavor = "multi_thread")]
async fn setup_saves_and_displays_the_unchecked_version() -> Result<(), Error> {
    let fixture = start().await?;
    let folder = fixture.root().join("machines/example/hl7-unchecked");
    std::fs::create_dir_all(&folder)?;
    let base = include_str!("../../../../../../core/config/examples/hl7.toml");
    let choice = include_str!("../../../../../../testing/fixtures/software-version-choices.toml");
    std::fs::write(folder.join("machine.toml"), format!("{base}\n{choice}"))?;
    let path = "/setup/synthetic/connection?definition=example/hl7-unchecked";
    let label = "Other version (not checked) (Software version not checked against a manual)";
    let page = fixture.get(path).await?;
    assert!(
        page.contains(&format!("<option value=\"other\">{label}</option>")),
        "{page}"
    );
    let form = fixture
        .form(&[
            ("definition", "example/hl7-unchecked"),
            ("host", "127.0.0.1"),
            ("tcp_port", "5601"),
            ("allowed_peers", ""),
            ("choice.software_version", "other"),
        ])
        .await?;
    let saved = fixture.post("/setup/synthetic", &form).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(saved.contains(label), "{saved}");
    let page = fixture.get(path).await?;
    assert!(
        page.contains(&format!(
            "<option value=\"other\" selected>{label}</option>"
        )),
        "{page}"
    );
    Ok(())
}

// ADR 0032 decisions 3 and 4, AGENTS.md "never invents data": a choice with
// neither a selection nor a suggestion is refused by name and changes no file;
// once chosen, Save writes every choice into the site file, the summary and
// audit name them, the settings version follows, and step 2 shows the saved
// value and what differs from the manual.
#[tokio::test(flavor = "multi_thread")]
async fn saving_keeps_every_choice_in_the_site_file() -> Result<(), Error> {
    let fixture = start().await?;
    with_choices_model(&fixture)?;
    let before = std::fs::read_to_string(&fixture.site)?;
    let refused = fixture
        .post("/setup/synthetic", &choices_form(&fixture, &[]).await?)
        .await?;
    assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
    assert!(
        refused.contains("choices.software_version: no value"),
        "{refused}"
    );
    assert!(
        refused.contains("Fix this in step 2: Connection."),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);
    let body = choices_form(
        &fixture,
        &[
            ("choice.protocol_mode", "alternate"),
            ("choice.software_version", "v2"),
        ],
    )
    .await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    assert!(
        saved.contains(concat!(
            "<dt>Analyzer settings</dt><dd>Protocol on the host link screen: Alternate ",
            "(acknowledge after ETX) (the manual suggests Literal (acknowledge after the ",
            "checksum)); Software version on the About screen: Version 2 or later (sample IDs ",
            "up to 17 characters) (the machine definition suggests none)</dd>"
        )),
        "{saved}"
    );
    let site: toml::Table = std::fs::read_to_string(&fixture.site)?.parse()?;
    assert_eq!(
        site.get("choices"),
        Some(&toml::Value::Table(
            "protocol_mode = \"alternate\"\nsoftware_version = \"v2\"\n".parse()?
        )),
        "the same table `machines set --choice` writes"
    );
    let machine = Program::load(&fixture.config)?;
    let effective = machine
        .machines()
        .first()
        .ok_or("machine missing")?
        .snapshot()
        .effective();
    let analyte_config::Stack::Vidas(stack) = effective.stack() else {
        return Err("a VIDAS stack was saved".into());
    };
    assert_eq!(stack.policy().ack_point, analyte_config::AckPoint::Etx);
    assert_eq!(
        stack.policy().sample_limit,
        analyte_config::VidasSampleLimit::Seventeen
    );
    let audits = fixture.audited("setup-save").await?;
    assert!(
        audits.iter().any(|reason| reason
            .contains("analyzer settings Protocol on the host link screen: Alternate")),
        "{audits:?}"
    );
    let page = fixture
        .get("/setup/synthetic/connection?definition=example/vidas-choices")
        .await?;
    assert!(
        page.contains(
            "<option value=\"alternate\" selected>Alternate (acknowledge after ETX)</option>"
        ),
        "{page}"
    );
    assert!(
        page.contains("Protocol on the host link screen <span class=\"tone-warn\">· differs from the manual</span>"),
        "{page}"
    );
    assert!(page.contains("<option value=\"v2\" selected>"), "{page}");
    Ok(())
}

// ADR 0032 decision 3: a choice the model does not declare, or a value it
// does not offer, is refused at step 2 and changes no file.
#[tokio::test(flavor = "multi_thread")]
async fn a_choice_the_model_does_not_offer_is_refused() -> Result<(), Error> {
    let fixture = start().await?;
    with_choices_model(&fixture)?;
    let before = std::fs::read_to_string(&fixture.site)?;
    for (fields, words) in [
        (
            [("choice.speed", "fast"), ("choice.software_version", "v1")],
            "this machine model has no choice called speed",
        ),
        (
            [
                ("choice.software_version", "v9"),
                ("choice.protocol_mode", "literal"),
            ],
            "choose software_version from its list: v1, v2",
        ),
    ] {
        let refused = fixture
            .post("/setup/synthetic", &choices_form(&fixture, &fields).await?)
            .await?;
        assert!(refused.starts_with("HTTP/1.1 409"), "{refused}");
        assert!(refused.to_lowercase().contains(words), "{words}: {refused}");
        assert!(
            refused.contains("Fix this in step 2: Connection."),
            "{refused}"
        );
    }
    let duplicate = format!(
        "{}&choice.software_version=v2",
        choices_form(&fixture, &[("choice.software_version", "v1")]).await?
    );
    let refused = fixture.post("/setup/synthetic", &duplicate).await?;
    assert!(!refused.starts_with("HTTP/1.1 200"), "{refused}");
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);
    assert!(fixture.backups()?.is_empty());
    Ok(())
}

// ADR 0032; Dialab Autolyser Operator's Manual Rev18 chapter 23.
#[tokio::test(flavor = "multi_thread")]
async fn step_two_requires_and_saves_the_labs_poll_interval() -> Result<(), Error> {
    let fixture = start().await?;
    let folder = fixture.root().join("machines/example/autolyser-poll");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("machine.toml"),
        include_str!("../../../../../../core/config/examples/autolyser.toml")
            .replace("poll_ms = 1000", "poll_ms = \"site\""),
    )?;
    let path = "/setup/synthetic/connection?definition=example/autolyser-poll";
    let page = fixture.get(path).await?;
    assert!(
        page.contains("name=\"poll_seconds\" type=\"number\""),
        "{page}"
    );
    let fields = [
        ("definition", "example/autolyser-poll"),
        ("host", "127.0.0.1"),
        ("tcp_port", "5601"),
        ("allowed_peers", ""),
    ];
    let before = std::fs::read_to_string(&fixture.site)?;
    let missing = fixture.form(&fields).await?;
    let refused = fixture.post("/setup/synthetic", &missing).await?;
    assert!(
        refused.contains("autolyser_poll_seconds: no value"),
        "{refused}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);

    let mut fields = fields.to_vec();
    fields.push(("poll_seconds", "42"));
    let body = fixture.form(&fields).await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    let site: toml::Table = std::fs::read_to_string(&fixture.site)?.parse()?;
    assert_eq!(
        site.get("autolyser_poll_seconds"),
        Some(&toml::Value::Integer(42))
    );
    let page = fixture.get(path).await?;
    assert!(page.contains("value=\"42\""), "{page}");
    Ok(())
}

// ADR 0035 (2026-11-26), ADR 0027 (one code path): step 2 asks how repeated
// results are handled, starting at "handle automatically (recommended)";
// Save writes `repeated_results = "always_ask"` as `machines set
// --repeated-results always_ask` does, the page shows it saved, a value off
// the list is refused and changes no file, and a POCT model is not asked.
#[tokio::test(flavor = "multi_thread")]
async fn step_two_saves_the_repeated_results_choice() -> Result<(), Error> {
    let fixture = start().await?;
    let path = "/setup/synthetic/connection?definition=example/network-hl7";
    let page = fixture.get(path).await?;
    for expected in [
        "<label class=\"field__label\" for=\"s2-repeated-results\">Repeated results</label>",
        "<select id=\"s2-repeated-results\" class=\"select\" name=\"repeated_results\" required>",
        "<option value=\"automatic\" selected>handle automatically (recommended)</option>",
        "<option value=\"always_ask\">always ask me</option>",
    ] {
        assert!(page.contains(expected), "missing {expected}: {page}");
    }
    let fields = [
        ("definition", "example/network-hl7"),
        ("host", "127.0.0.1"),
        ("tcp_port", "5601"),
        ("allowed_peers", ""),
    ];
    let before = std::fs::read_to_string(&fixture.site)?;
    let mut refused = fields.to_vec();
    refused.push(("repeated_results", "ask"));
    let body = fixture.form(&refused).await?;
    let answer = fixture.post("/setup/synthetic", &body).await?;
    assert!(answer.starts_with("HTTP/1.1 409"), "{answer}");
    assert!(
        answer.contains("Choose how repeated results are handled: automatic or always_ask."),
        "{answer}"
    );
    assert_eq!(std::fs::read_to_string(&fixture.site)?, before);

    let mut ask = fields.to_vec();
    ask.push(("repeated_results", "always_ask"));
    let body = fixture.form(&ask).await?;
    let saved = fixture.post("/setup/synthetic", &body).await?;
    assert!(saved.starts_with("HTTP/1.1 200"), "{saved}");
    let site: toml::Table = std::fs::read_to_string(&fixture.site)?.parse()?;
    assert_eq!(
        site.get("repeated_results"),
        Some(&toml::Value::String("always_ask".into())),
        "the same key `machines set --repeated-results always_ask` writes"
    );
    let audits = fixture.audited("setup-save").await?;
    assert!(
        audits
            .iter()
            .any(|reason| reason.contains("repeated results: always ask me")),
        "{audits:?}"
    );
    let page = fixture.get(path).await?;
    assert!(
        page.contains("<option value=\"always_ask\" selected>always ask me</option>"),
        "{page}"
    );

    let folder = fixture.root().join("machines/example/poct");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("machine.toml"),
        include_str!("../../../../../../core/config/examples/poct.toml"),
    )?;
    let poct = fixture
        .get("/setup/synthetic/connection?definition=example/poct")
        .await?;
    assert!(poct.starts_with("HTTP/1.1 200"), "{poct}");
    assert!(!poct.contains("name=\"repeated_results\""), "{poct}");
    Ok(())
}
