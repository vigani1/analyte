//! Tests record-only snapshots: shipped capture profiles, the model's link and the lab's answers (ADR 0034).
use analyte_config::recording::{self, CaptureProtocol};
use analyte_config::{Error, LabApps, RecordLink, Stack, StreamKind};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SITE: &str = "retention = \"keep\"\nschema_version = 1\n";

fn record(protocol: &str, kind: &str) -> String {
    format!(
        "recording_schema_version = 1\nprotocol = \"{protocol}\"\n[transport]\nkind = \"{kind}\"\n"
    )
}

fn site(endpoint: &str, answers: &str) -> String {
    format!("{SITE}endpoint = \"{endpoint}\"\n[recording]\n{answers}")
}

fn load(model: &str, site: &str) -> Result<analyte_config::Validated, Error> {
    recording::load(("record.toml", model), ("site.toml", site))
}

fn refusal(model: &str, site: &str) -> Result<String, Box<dyn std::error::Error>> {
    match load(model, site) {
        Ok(_) => Err("accepted".into()),
        Err(error) => Ok(error.to_string()),
    }
}

// ADR 0034 decisions 2 to 4: each protocol's shipped profile loads with the
// model's link and the lab's answers; the snapshot has no lab app, no mappings
// and no orders, and its bytes name the profile.
#[test]
fn every_capture_profile_records_without_a_lab_app() -> TestResult {
    for (protocol, answers, expected) in [
        ("astm", "", CaptureProtocol::Astm),
        ("hl7", "", CaptureProtocol::Hl7),
        (
            "vidas",
            "acknowledgment = \"alternate\"\n",
            CaptureProtocol::Vidas,
        ),
        (
            "autolyser",
            "poll_seconds = 30\ncontrols = true\n",
            CaptureProtocol::Autolyser,
        ),
    ] {
        let validated = load(
            &record(protocol, "tcp_server"),
            &site("127.0.0.1:5600", answers),
        )?;
        let effective = validated.effective();
        assert!(effective.records_only(), "{protocol}");
        assert!(effective.site().destination.is_none(), "{protocol}");
        assert!(effective.machine().test_system().is_none());
        assert!(!validated.canonical().contains("[machine.mappings"));
        let Stack::Record(stack) = effective.stack() else {
            return Err(format!("{protocol}: not a record-only stack").into());
        };
        assert_eq!(stack.protocol(), expected);
        assert!(
            validated.canonical().contains("[recording.profile]"),
            "{protocol}"
        );
        assert!(!validated.canonical().contains("lab_app"), "{protocol}");
        // The same files give the same bytes.
        let again = load(
            &record(protocol, "tcp_server"),
            &site("127.0.0.1:5600", answers),
        )?;
        assert_eq!(again.digest(), validated.digest());
    }
    // An answer that changes is a new settings version.
    let thirty = load(
        &record("autolyser", "tcp_server"),
        &site("127.0.0.1:5600", "poll_seconds = 30\ncontrols = true\n"),
    )?;
    let sixty = load(
        &record("autolyser", "tcp_server"),
        &site("127.0.0.1:5600", "poll_seconds = 60\ncontrols = true\n"),
    )?;
    assert_ne!(thirty.digest(), sixty.digest());
    let Stack::Record(stack) = sixty.effective().stack() else {
        return Err("record-only stack".into());
    };
    let RecordLink::Autolyser(link) = stack.link() else {
        return Err("Autolyser link".into());
    };
    assert_eq!(link.poll_ms(), 60_000);
    assert!(link.controls());
    Ok(())
}

// CLSI LIS01-A2 6.3.1 and 6.5.2: the ASTM profile carries the standard's frame
// limit and timers.
#[test]
fn astm_profile_holds_the_lis01_frame_limit_and_timers() -> TestResult {
    let validated = load(&record("astm", "tcp_server"), &site("127.0.0.1:5600", ""))?;
    let Stack::Record(stack) = validated.effective().stack() else {
        return Err("record-only stack".into());
    };
    let RecordLink::Astm(link) = stack.link() else {
        return Err("ASTM link".into());
    };
    assert_eq!(link.frame_limit(), 64_000);
    assert_eq!(
        (
            link.establish_ms(),
            link.contention_ms(),
            link.frame_ms(),
            link.receive_ms(),
            link.commit_ms()
        ),
        (15_000, 20_000, 15_000, 30_000, 15_000)
    );
    Ok(())
}

// ADR 0034 decision 3 and ADR 0031: a serial link takes the manual's
// suggestion and the lab's [serial]; a value neither gives is refused by key.
#[test]
fn a_serial_recording_needs_every_cable_value() -> TestResult {
    let model = format!(
        "{}[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\n",
        record("astm", "serial")
    );
    let missing = refusal(&model, &site("COM3", ""))?;
    assert!(
        missing.contains("transport.serial.flow_control: no value"),
        "{missing}"
    );
    let chosen =
        format!("{SITE}endpoint = \"COM3\"\n[serial]\nflow_control = \"none\"\n[recording]\n");
    let validated = load(&model, &chosen)?;
    let Stack::Record(stack) = validated.effective().stack() else {
        return Err("record-only stack".into());
    };
    let StreamKind::Serial(serial) = stack.transport().kind() else {
        return Err("serial link".into());
    };
    assert_eq!(serial.baud, 9600);
    assert_eq!(
        validated
            .effective()
            .serial_suggestion()
            .and_then(|suggested| suggested.baud),
        Some(9600)
    );
    Ok(())
}

// ADR 0034 decisions 3 and 7, AGENTS rule 9: nothing is assumed; a missing
// answer is refused by name, an answer for another protocol is refused, and a
// record-only site names no lab app.
#[test]
fn record_only_refuses_what_it_cannot_honour() -> TestResult {
    let listen = "127.0.0.1:5600";
    for (model, site_text, key) in [
        (
            record("vidas", "tcp_server"),
            site(listen, ""),
            "recording.acknowledgment: no value",
        ),
        (
            record("autolyser", "tcp_server"),
            site(listen, "controls = false\n"),
            "recording.poll_seconds: no value",
        ),
        (
            record("autolyser", "tcp_server"),
            site(listen, "poll_seconds = 30\n"),
            "recording.controls: no value",
        ),
        (
            record("autolyser", "tcp_server"),
            site(listen, "poll_seconds = 0\ncontrols = false\n"),
            "recording.poll_seconds",
        ),
        (
            record("autolyser", "tcp_server"),
            site(listen, "poll_seconds = 3601\ncontrols = false\n"),
            "recording.poll_seconds",
        ),
        (
            record("astm", "tcp_server"),
            site(listen, "poll_seconds = 30\n"),
            "recording.poll_seconds: this protocol does not use this answer",
        ),
        (
            record("hl7", "tcp_server"),
            site(listen, "acknowledgment = \"literal\"\n"),
            "recording.acknowledgment: this protocol does not use this answer",
        ),
        (record("hl7", "serial"), site("COM3", ""), "transport.kind"),
        (
            record("astm", "watched_folder"),
            site("/tmp/in", ""),
            "transport.kind",
        ),
        (record("law", "tcp_server"), site(listen, ""), "protocol"),
        (record("poct", "tcp_server"), site(listen, ""), "protocol"),
        (
            record("astm", "tcp_server"),
            format!(
                "{}lab_app = \"main-lab\"\n[recording]\n",
                &site(listen, "")[..site(listen, "").len() - "[recording]\n".len()]
            ),
            "lab_app",
        ),
        (
            record("astm", "tcp_server"),
            format!("{}[choices]\nmode = \"a\"\n", site(listen, "")),
            "choices",
        ),
        (
            record("astm", "tcp_server").replace(
                "recording_schema_version = 1",
                "recording_schema_version = 2",
            ),
            site(listen, ""),
            "recording_schema_version",
        ),
    ] {
        let refused = refusal(&model, &site_text)?;
        assert!(refused.contains(key), "{key}: {refused}");
    }
    // An unknown answer is refused, not ignored.
    assert!(load(&record("astm", "tcp_server"), &site(listen, "guess = 1\n")).is_err());
    Ok(())
}

// ADR 0034 decision 3: a model's record.toml cannot be used for a normal
// connection, and the refusal names the way to record it.
#[test]
fn a_record_file_is_refused_for_a_normal_connection() -> TestResult {
    let normal = include_str!("../examples/site.toml");
    let refused = analyte_config::Draft::load_with(
        ("record.toml", &record("astm", "tcp_server")),
        ("site.toml", normal),
        ("overrides", "schema_version = 1"),
        &LabApps::none(analyte_config::LAB_APPS_FILE),
    )
    .err()
    .ok_or("a record file loaded as a definition")?
    .to_string();
    assert!(refused.contains("it can only record"), "{refused}");
    Ok(())
}

// ADR 0034 decisions 1 and 3: a ready definition records from its protocol
// and link only; the dispatcher refuses overrides beside a record-only site.
#[test]
fn a_ready_definition_records_from_its_protocol_and_link_only() -> TestResult {
    let definition = include_str!("../examples/astm.toml");
    let chosen = format!("{SITE}endpoint = \"COM3\"\n[recording]\n");
    let none = LabApps::none(analyte_config::LAB_APPS_FILE);
    let validated = analyte_config::load_machine(
        ("machine.toml", definition),
        ("site.toml", &chosen),
        None,
        &none,
    )?;
    let Stack::Record(stack) = validated.effective().stack() else {
        return Err("record-only stack".into());
    };
    assert_eq!(stack.protocol(), CaptureProtocol::Astm);
    assert!(!validated.canonical().contains("test_system"));
    let refused = analyte_config::load_machine(
        ("machine.toml", definition),
        ("site.toml", &chosen),
        Some(("overrides.toml", "schema_version = 1")),
        &none,
    )
    .err()
    .ok_or("overrides accepted")?
    .to_string();
    assert!(refused.contains("overrides"), "{refused}");
    // The same definition with a normal site still loads normally.
    let normal = analyte_config::load_machine(
        ("machine.toml", definition),
        (
            "site.toml",
            include_str!("../examples/site.toml")
                .replace("127.0.0.1:5600", "COM3")
                .as_str(),
        ),
        None,
        &none,
    )?;
    assert!(!normal.effective().records_only());
    Ok(())
}

/// The shipped cobas e 411 record file and where it sits, so its departure
/// names the model by its folder key (ADR 0033).
const COBAS: &str = include_str!("../../../../machines/roche/cobas-e411/record.toml");
const COBAS_PATH: &str = "machines/roche/cobas-e411/record.toml";

fn cobas_site(answers: &str) -> String {
    format!("{SITE}endpoint = \"COM3\"\n[serial]\nflow_control = \"none\"\n[recording]\n{answers}")
}

fn cobas(answers: &str) -> Result<analyte_config::Validated, Error> {
    recording::load((COBAS_PATH, COBAS), ("site.toml", &cobas_site(answers)))
}

fn packed(validated: &analyte_config::Validated) -> Result<bool, Box<dyn std::error::Error>> {
    let Stack::Record(stack) = validated.effective().stack() else {
        return Err("record-only stack".into());
    };
    Ok(stack.packed_records())
}

// cobas e 411 HIM v2.3 Host communication setting (p. 7) and section 5
// (p. 14), v2.2 B section 2 (Table B-3) and D section 1 (p. D-2): the cobas
// protocol type may pack several records in one frame, the Elecsys type
// never; ADR 0015 scopes the packing departure to the cobas mode only.
#[test]
fn cobas_record_file_packs_only_in_the_cobas_protocol_type() -> TestResult {
    let packing = cobas("analyzer_mode = \"cobas\"\n")?;
    assert!(packed(&packing)?);
    assert!(
        packing.canonical().contains("astm_packed_records_in_frame"),
        "the snapshot names the departure in use"
    );
    assert_eq!(packing.effective().machine().deviations.len(), 1);
    let strict = cobas("analyzer_mode = \"elecsys\"\n")?;
    assert!(!packed(&strict)?);
    assert!(strict.effective().machine().deviations.is_empty());
    assert!(!strict.canonical().contains("astm_packed_records_in_frame"));
    assert_ne!(packing.digest(), strict.digest());
    // Nothing is assumed: the protocol type must be answered, and only with
    // a value the model offers.
    let missing = cobas("").err().ok_or("no answer accepted")?;
    assert!(
        matches!(missing, Error::MissingAnalyzerMode { ref allowed, .. } if allowed == "elecsys, cobas"),
        "{missing}"
    );
    let unoffered = cobas("analyzer_mode = \"astm\"\n")
        .err()
        .ok_or("unoffered mode accepted")?;
    assert!(
        matches!(unoffered, Error::AnalyzerModeValue { .. }),
        "{unoffered}"
    );
    // A model that asks no analyzer mode refuses an answer to it, and the
    // shared ASTM profile never packs.
    let plain = refusal(
        &record("astm", "tcp_server"),
        &site("127.0.0.1:5600", "analyzer_mode = \"cobas\"\n"),
    )?;
    assert!(plain.contains("recording.analyzer_mode"), "{plain}");
    assert!(!packed(&load(
        &record("astm", "tcp_server"),
        &site("127.0.0.1:5600", "")
    )?)?);
    Ok(())
}

// ADR 0015 with ADR 0034: a record file can declare only packed ASTM records,
// for its own model, one offered mode, analyzer uploads and ASTM results,
// with documented evidence; each other declaration is refused by reason.
#[test]
fn record_file_departures_stay_narrow_and_evidenced() -> TestResult {
    use analyte_config::DepartureRefusal as R;
    let answer = cobas_site("analyzer_mode = \"cobas\"\n");
    let cases: [(&str, String, R); 8] = [
        (
            COBAS_PATH,
            COBAS.replace("astm_packed_records_in_frame", "astm_frame_limit"),
            R::RecordOnlyRule,
        ),
        (
            "machines/roche/cobas-6000/record.toml",
            COBAS.to_owned(),
            R::ScopeMismatch,
        ),
        (
            COBAS_PATH,
            COBAS.replace("mode = \"cobas\"", "mode = \"packed\""),
            R::ModeNotOffered,
        ),
        (
            COBAS_PATH,
            COBAS.replace("protocol = \"astm\"\n\n", "protocol = \"autolyser\"\n\n"),
            R::ProtocolMismatch,
        ),
        (
            COBAS_PATH,
            COBAS.replace("\"machine_to_host\"", "\"host_to_machine\""),
            R::DirectionMismatch,
        ),
        (
            COBAS_PATH,
            COBAS.replace("level = \"documented\"", "level = \"proven\""),
            R::LevelEvidence,
        ),
        (
            COBAS_PATH,
            COBAS.replace(
                "covers_unknown_firmware = true",
                "covers_unknown_firmware = false",
            ),
            R::UnknownFirmware,
        ),
        (
            COBAS_PATH,
            COBAS.replace(
                "guarantee = \"packed_records\"",
                "guarantee = \"compatibility\"",
            ),
            R::MissingRuleEvidence,
        ),
    ];
    for (path, model, expected) in cases {
        let error = recording::load((path, &model), ("site.toml", &answer))
            .err()
            .ok_or(format!("{expected:?}: accepted"))?;
        assert!(
            matches!(error, Error::Departure { reason, .. } if reason == expected),
            "{expected:?}: {error}"
        );
    }
    // Without [analyzer_mode] no departure can name an offered mode, so it
    // could never be selected by the lab's answer.
    let start = COBAS.find("[analyzer_mode]").ok_or("analyzer_mode")?;
    let end = COBAS
        .get(start..)
        .and_then(|rest| {
            rest.find(
                "]

",
            )
        })
        .and_then(|offset| start.checked_add(offset)?.checked_add(3))
        .ok_or("end of analyzer_mode")?;
    let without = format!(
        "{}{}",
        COBAS.get(..start).ok_or("head")?,
        COBAS.get(end..).ok_or("tail")?
    );
    assert!(!without.contains("analyzer_mode"));
    let error = recording::load((COBAS_PATH, &without), ("site.toml", &cobas_site("")))
        .err()
        .ok_or("a departure without a mode question accepted")?;
    assert!(
        matches!(
            error,
            Error::Departure {
                reason: R::ModeNotOffered,
                ..
            }
        ),
        "{error}"
    );
    Ok(())
}
