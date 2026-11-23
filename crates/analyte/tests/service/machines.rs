//! Tests shipped machine definitions and recorded conversations.
use super::*;
use analyte_config::{Draft, EvidenceLevel};
use analyte_transport::recording::{self, Direction, Event, Header};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};
use tokio::io::AsyncWriteExt as _;

const SECTIONS: [&str; 8] = [
    "Machine",
    "Connection",
    "Settings on the machine",
    "Supported workflows",
    "Departures",
    "Evidence",
    "Open questions",
    "Sources",
];

fn definitions(root: &Path) -> Result<Vec<PathBuf>, Error> {
    for vendor in std::fs::read_dir(root)? {
        let vendor = vendor?;
        if !vendor.file_type()?.is_dir() {
            continue;
        }
        for model in std::fs::read_dir(vendor.path())? {
            let model = model?;
            if !model.file_type()?.is_dir() {
                continue;
            }
            let folder = model.path();
            let readme = folder.join("README.md");
            let ready = folder.join("machine.toml").is_file();
            let blocked = folder.join("record.toml").is_file();
            assert!(
                !(ready && blocked),
                "{}: both definitions exist",
                folder.display()
            );
            assert!(
                ready || blocked || !folder.join("recordings").exists(),
                "{}: recordings require a definition",
                folder.display()
            );
            if readme.is_file() && !ready {
                let text = std::fs::read_to_string(&readme)?;
                assert!(
                    text.split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(4)
                        .any(|words| words == ["a", "blocked", "machine", "definition,"]),
                    "{}: missing machine.toml without a blocked machine definition declaration",
                    folder.display()
                );
            }
        }
    }
    let mut pending = vec![root.to_owned()];
    let mut found = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if matches!(
                entry.file_name().to_str(),
                Some("machine.toml" | "record.toml")
            ) {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

fn readme(path: &Path, level: EvidenceLevel) -> Result<(), Error> {
    let text = std::fs::read_to_string(path)?.replace("\r\n", "\n");
    let headings: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .collect();
    assert_eq!(
        headings,
        SECTIONS,
        "{}: fixed README sections",
        path.display()
    );
    let evidence = text
        .split("## Evidence\n")
        .nth(1)
        .ok_or("Evidence section missing")?
        .split("## Open questions")
        .next()
        .ok_or("Evidence section empty")?;
    let levels: Vec<_> = evidence
        .lines()
        .filter_map(|line| line.strip_prefix("Level: "))
        .collect();
    let expected = match level {
        EvidenceLevel::Documented => "documented",
        EvidenceLevel::Simulated => "simulated",
        EvidenceLevel::Proven => "proven",
    };
    assert_eq!(
        levels,
        [expected],
        "{}: evidence must agree with TOML",
        path.display()
    );
    Ok(())
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaySettings {
    choices: Option<toml::Table>,
    recording: Option<toml::Table>,
    expected_gap: Option<ExpectedGap>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedGap {
    reason: String,
    reply_event: usize,
    failure: MissingReply,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum MissingReply {
    Deadline,
    Disconnect,
}

// ADR 0012; ADR 0009; ADR 0034 decisions 5 to 7.
#[tokio::test]
async fn every_machine_validates_and_replays() -> Result<(), Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines");
    for definition in definitions(&root)? {
        let folder = definition.parent().ok_or("machine directory missing")?;
        let model = folder.strip_prefix(&root)?;
        assert_eq!(
            model.components().count(),
            2,
            "machines/<vendor>/<model> required"
        );
        replay_model(folder, model, &definition)
            .await
            .map_err(|error| format!("{}: {error}", definition.display()))?;
    }
    Ok(())
}

async fn replay_model(folder: &Path, model: &Path, definition: &Path) -> Result<(), Error> {
    let record_only = definition.ends_with(analyte_config::RECORD_FILE);
    let directory = folder.join("recordings");
    let sidecar = folder.join("replay.toml");
    let mut settings: BTreeMap<String, ReplaySettings> = if sidecar.try_exists()? {
        toml::from_str(&std::fs::read_to_string(sidecar)?)?
    } else {
        BTreeMap::new()
    };
    if record_only && !directory.try_exists()? && settings.is_empty() {
        return Ok(());
    }
    let text = std::fs::read_to_string(definition)?;
    let mut paths = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    assert!(!paths.is_empty(), "recordings directory is empty");
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("recording name")?;
        let answers = settings.remove(name).unwrap_or_default();
        let result = async {
            if !path.is_file() || !name.ends_with(".rec.jsonl") {
                return Err("unexpected recordings entry".into());
            }
            let (header, events) =
                recording::read(std::fs::File::open(&path)?, recording::Limits::default())?;
            validate_gap(&events, answers.expected_gap.as_ref())?;
            if record_only {
                replay_record_only(model, &text, &header, &events, &answers).await
            } else {
                replay_ready(folder, model, &text, &header, &events, &answers).await
            }
        }
        .await;
        result.map_err(|error| format!("{name}: {error}"))?;
        tracing::info!(recording = %path.display(), "replayed machine recording");
    }
    assert!(
        settings.is_empty(),
        "replay.toml names unknown recordings: {:?}",
        settings.keys()
    );
    Ok(())
}

async fn replay_ready(
    folder: &Path,
    model: &Path,
    text: &str,
    header: &Header,
    events: &[Event],
    answers: &ReplaySettings,
) -> Result<(), Error> {
    if answers.recording.is_some() {
        return Err("use choices to supply site answers for machine.toml".into());
    }
    let raw: toml::Table = text.parse()?;
    let kind = transport_kind(&raw)?;
    let endpoint = match kind {
        "watched_folder" => std::env::temp_dir().to_string_lossy().into_owned(),
        "serial" => "COM3".into(),
        _ => "127.0.0.1:5600".into(),
    };
    let site = site(&endpoint, answers, kind == "serial")?;
    let checked = Draft::load(
        ("machine", text),
        ("site", &site),
        ("overrides", "schema_version=1"),
    )?
    .validate()?;
    readme(
        &folder.join("README.md"),
        checked.effective().machine().evidence.level,
    )?;
    let stack = checked.effective().stack();
    assert_eq!(header.stack, stack.layer_names(), "recording stack");
    replay_machine(model, text, events, header, stack, answers).await
}

fn transport_kind(table: &toml::Table) -> Result<&str, Error> {
    table
        .get("transport")
        .and_then(|transport| transport.get("kind"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "transport.kind missing".into())
}

const SERIAL_SETTINGS: &str =
    "baud=9600\ndata_bits=8\nparity='none'\nstop_bits=1\nflow_control='none'";

fn recording_site(endpoint: &str, answers: &ReplaySettings, serial: bool) -> Result<String, Error> {
    let mut site: toml::Table = toml::from_str("schema_version=1\nretention='keep'")?;
    site.insert("endpoint".into(), endpoint.into());
    site.insert(
        "recording".into(),
        answers.recording.clone().unwrap_or_default().into(),
    );
    if serial {
        site.insert(
            "serial".into(),
            SERIAL_SETTINGS.parse::<toml::Table>()?.into(),
        );
    }
    Ok(toml::to_string(&site)?)
}

async fn replay_record_only(
    model: &Path,
    text: &str,
    header: &Header,
    events: &[Event],
    answers: &ReplaySettings,
) -> Result<(), Error> {
    if answers.choices.is_some() {
        return Err("record.toml replay takes recording answers instead of choices".into());
    }
    let gap = answers.expected_gap.as_ref();
    let mut definition: toml::Table = text.parse()?;
    let serial = transport_kind(&definition)? == "serial";
    let definition_path = model.join("record.toml");
    let name = definition_path.to_string_lossy().replace('\\', "/");
    let original_site = recording_site(
        if serial { "COM3" } else { "127.0.0.1:5600" },
        answers,
        serial,
    )?;
    let validated = analyte_config::recording::load((&name, text), ("site", &original_site))?;
    assert!(validated.effective().records_only());
    let stack = validated.effective().stack();
    assert_eq!(header.stack, stack.layer_names(), "recording stack");

    // Keep the model key and protocol settings when replacing the physical link.
    definition.insert(
        "transport".into(),
        "kind='tcp_server'".parse::<toml::Table>()?.into(),
    );
    let temp = tempfile::tempdir()?;
    std::fs::create_dir_all(temp.path().join(model))?;
    std::fs::write(
        temp.path().join(definition_path),
        toml::to_string(&definition)?,
    )?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let endpoint = listener.local_addr()?;
    std::fs::write(
        temp.path().join("site.toml"),
        recording_site(&endpoint.to_string(), answers, false)?,
    )?;
    let path = temp.path().join("program.toml");
    std::fs::write(
        &path,
        analyte_fixtures::program(&format!(
            "[[machines]]\nid='fixture'\ndefinition='{name}'\nsite='site.toml'\n"
        )),
    )?;
    let mut program = Program::load(&path)?;
    program.set_loopback_listener_for_test("fixture", listener)?;
    let running = Running::start_program(program, "synthetic-api").await?;
    let result = async {
        let mut peer = connect(&path, "fixture").await?;
        let deadline = Duration::from_millis(u64::from(stack.operation_timeout_ms()));
        let replayed = analyte_sim::replay(&mut peer, events, &header.exceptions, deadline).await;
        check_replies(replayed, gap)?;
        peer.shutdown().await?;
        let mut extra = [0];
        let read = tokio::time::timeout(HANG_GUARD, peer.read(&mut extra)).await??;
        if read != 0 {
            return Err(format!("unscripted host byte: {extra:02x?}").into());
        }
        Ok::<_, Error>(())
    }
    .await;
    running.stop().await?;
    result?;
    assert_recorded(&path, events)
}

fn validate_gap(events: &[Event], gap: Option<&ExpectedGap>) -> Result<(), Error> {
    if let Some(gap) = gap {
        // The full inbound conversation must still reach the durable store.
        let last = events.iter().rposition(|event| event.link.is_none());
        if gap.reason.trim().is_empty()
            || last != Some(gap.reply_event)
            || !events
                .get(gap.reply_event)
                .is_some_and(|event| event.direction == Direction::HostToMachine)
        {
            return Err(
                "expected_gap needs a reason and must name the final wire event (a host reply)"
                    .into(),
            );
        }
    }
    Ok(())
}

fn check_replies(
    result: Result<(), analyte_sim::Error>,
    gap: Option<&ExpectedGap>,
) -> Result<(), Error> {
    match (result, gap) {
        (Ok(()), None) => Ok(()),
        (Ok(()), Some(gap)) => Err(format!("remove resolved expected gap: {}", gap.reason).into()),
        (Err(analyte_sim::Error::Deadline(event)), Some(gap))
            if event == gap.reply_event && matches!(gap.failure, MissingReply::Deadline) =>
        {
            tracing::warn!(event, reason = %gap.reason, "expected reply deadline");
            Ok(())
        }
        (Err(analyte_sim::Error::Io { event, source }), Some(gap))
            if event == gap.reply_event
                && matches!(gap.failure, MissingReply::Disconnect)
                && source.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
            tracing::warn!(event, reason = %gap.reason, "expected disconnect before reply");
            Ok(())
        }
        (Err(error), _) => Err(error.into()),
    }
}

// ADR 0009 replay amendment: a known gap expires as soon as the reply matches.
#[test]
fn expected_gap_requires_the_named_failure() {
    let gap = ExpectedGap {
        reason: "synthetic missing reply".into(),
        reply_event: 1,
        failure: MissingReply::Deadline,
    };
    assert!(check_replies(Err(analyte_sim::Error::Deadline(1)), Some(&gap)).is_ok());
    assert!(check_replies(Err(analyte_sim::Error::Deadline(0)), Some(&gap)).is_err());
    assert!(check_replies(Ok(()), Some(&gap)).is_err());
}

fn assert_recorded(path: &Path, events: &[Event]) -> Result<(), Error> {
    let expected: Vec<_> = events
        .iter()
        .filter(|event| event.link.is_none() && event.direction == Direction::MachineToHost)
        .flat_map(|event| event.bytes.iter().copied())
        .collect();
    let db = database(path)?;
    let mut query = db.prepare("SELECT metadata, raw FROM receipts ORDER BY rowid")?;
    let mut kept = Vec::new();
    for row in query.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })? {
        let (metadata, bytes) = row?;
        let metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&metadata)?;
        assert_eq!(
            metadata.driver.namespace.as_str(),
            analyte_config::recording::RECORD_DRIVER
        );
        kept.extend(bytes);
    }
    assert_eq!(kept, expected, "every inbound byte is kept in order");
    for sql in [
        "SELECT count(*) FROM interpretations",
        "SELECT count(*) FROM outbox",
        "SELECT count(*) FROM machine_orders",
    ] {
        let count: i64 = db.query_row(sql, [], |row| row.get(0))?;
        assert_eq!(count, 0, "{sql}");
    }
    Ok(())
}

async fn replay_machine(
    model: &Path,
    definition: &str,
    events: &[Event],
    header: &Header,
    stack: &analyte_config::Stack,
    answers: &ReplaySettings,
) -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let endpoint = listener.local_addr()?;
    let folder = stack.transport_kind() == analyte_config::TransportKind::WatchedFolder;
    let incoming = temp.path().join("incoming");
    std::fs::create_dir(&incoming)?;
    let endpoint = if folder {
        incoming.to_string_lossy().into_owned()
    } else {
        endpoint.to_string()
    };
    let definition_path = model.join("machine.toml");
    let name = definition_path.to_string_lossy().replace('\\', "/");
    std::fs::create_dir_all(temp.path().join(model))?;
    std::fs::write(temp.path().join(definition_path), definition)?;
    std::fs::write(
        temp.path().join("site.toml"),
        site(&endpoint, answers, false)?,
    )?;
    // Replace only physical I/O: the real framing, dialog, syntax, mapping and store run unchanged.
    std::fs::write(
        temp.path().join("overrides.toml"),
        if folder {
            "schema_version=1".into()
        } else {
            format!(
                "schema_version=1\n[transport]\nkind='tcp_server'\noperation_timeout_ms={}\n",
                stack.operation_timeout_ms()
            )
        },
    )?;
    let path = temp.path().join("program.toml");
    std::fs::write(
        &path,
        analyte_fixtures::program(&format!(
            "[[machines]]\nid='fixture'\ndefinition='{name}'\nsite='site.toml'\noverrides='overrides.toml'\n",
        )),
    )?;
    let mut program = Program::load(&path)?;
    if !folder {
        program.set_loopback_listener_for_test("fixture", listener)?;
    }
    let deadline = Duration::from_millis(u64::from(
        program
            .machines()
            .first()
            .ok_or("machine absent")?
            .snapshot()
            .effective()
            .stack()
            .timeouts()
            .send_timeout_ms(),
    ));
    let reference = Reference::start(
        &temp.path().join("reference.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    program.set_loopback_destination_for_test("demo", reference.url.clone())?;
    let running = Running::start_program(program, "synthetic-api").await?;
    let replayed = async {
        if folder {
            replay_file(&path, &incoming, events).await?;
        } else {
            let mut peer = connect(&path, "fixture").await?;
            let replayed =
                analyte_sim::replay(&mut peer, events, &header.exceptions, deadline).await;
            check_replies(replayed, answers.expected_gap.as_ref())?;
        }
        Ok::<_, Error>(())
    }
    .await;
    let stopped = running.stop().await;
    reference.close().await?;
    replayed?;
    stopped
}

fn site(endpoint: &str, answers: &ReplaySettings, serial: bool) -> Result<String, Error> {
    let mut site: toml::Table =
        toml::from_str(include_str!("../../../core/config/examples/site.toml"))?;
    site.insert("endpoint".into(), toml::Value::String(endpoint.into()));
    if let Some(choices) = &answers.choices {
        site.insert("choices".into(), choices.clone().into());
    }
    if serial {
        site.insert(
            "serial".into(),
            SERIAL_SETTINGS.parse::<toml::Table>()?.into(),
        );
    }
    Ok(toml::to_string(&site)?)
}

async fn replay_file(path: &Path, incoming: &Path, events: &[Event]) -> Result<(), Error> {
    let mut bytes = Vec::new();
    for event in events {
        assert!(
            event.link.is_none(),
            "file recording cannot contain stream link transitions"
        );
        assert_eq!(
            event.direction,
            Direction::MachineToHost,
            "file pickup cannot send wire responses"
        );
        bytes.extend_from_slice(&event.bytes);
    }
    std::fs::write(incoming.join("recording"), &bytes)?;
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let stored = database(path).is_ok_and(|db| {
                db.query_row(
                    "SELECT count(*) FROM receipts WHERE raw=?1",
                    [&bytes],
                    |row| row.get::<_, i64>(0),
                )
                .is_ok_and(|count| count > 0)
            });
            if stored {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

// ADR 0012 §5
#[test]
fn missing_definition_requires_blocked_declaration() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let model = temp.path().join("vendor/model");
    std::fs::create_dir_all(&model)?;
    std::fs::write(
        model.join("README.md"),
        "This is a blocked\nmachine definition, pending evidence.",
    )?;
    assert!(definitions(temp.path())?.is_empty());
    std::fs::write(model.join("machine.toml"), "synthetic")?;
    assert_eq!(definitions(temp.path())?, [model.join("machine.toml")]);
    Ok(())
}

// ADR 0012 §5
#[test]
#[should_panic(expected = "missing machine.toml")]
fn deleted_definition_is_rejected() {
    let temp = tempfile::tempdir().expect("temporary inventory");
    let model = temp.path().join("vendor/model");
    std::fs::create_dir_all(&model).expect("machine folder");
    std::fs::write(model.join("README.md"), "Level: documented").expect("machine README");
    definitions(temp.path()).expect("read inventory");
}

// ADR 0034 decision 3: every shipped record.toml loads through the real
// record-only loader with a synthetic site, names only its protocol and link,
// and never sits beside a machine.toml (one place per model).
#[test]
fn every_record_file_loads_as_a_recording() -> Result<(), Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines");
    let mut found = 0_usize;
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                pending.push(path);
                continue;
            }
            if entry.file_name() != analyte_config::RECORD_FILE {
                continue;
            }
            found = found.saturating_add(1);
            assert!(
                !directory.join("machine.toml").exists(),
                "{}: record.toml beside machine.toml",
                directory.display()
            );
            let text = std::fs::read_to_string(&path)?;
            let table: toml::Table = text.parse()?;
            let kind = table
                .get("transport")
                .and_then(|transport| transport.get("kind"))
                .and_then(toml::Value::as_str)
                .ok_or("transport.kind")?;
            let endpoint = if kind == "serial" {
                "COM3"
            } else {
                "127.0.0.1:5600"
            };
            let serial = if kind == "serial" {
                "[serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n"
            } else {
                ""
            };
            let answers = match table.get("protocol").and_then(toml::Value::as_str) {
                Some("vidas") => "acknowledgment = \"literal\"\n",
                Some("autolyser") => "poll_seconds = 30\ncontrols = false\n",
                _ => "",
            };
            let site = format!(
                "retention = \"keep\"\nschema_version = 1\nendpoint = \"{endpoint}\"\n{serial}[recording]\n{answers}"
            );
            // A model that asks its analyzer mode loads with each value it
            // offers; a departure applies only to the mode it names.
            let modes = table
                .get("analyzer_mode")
                .and_then(|mode| mode.get("values"))
                .and_then(toml::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.get("value")?.as_str())
                        .map(|value| Some(value.to_owned()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![None]);
            for mode in modes {
                let chosen = mode
                    .as_ref()
                    .map(|mode| format!("analyzer_mode = \"{mode}\"\n"))
                    .unwrap_or_default();
                let validated = analyte_config::recording::load(
                    (&path.display().to_string(), &text),
                    ("site.toml", &format!("{site}{chosen}")),
                )
                .map_err(|error| format!("{}: {error}", path.display()))?;
                let effective = validated.effective();
                assert!(effective.records_only());
                for departure in &effective.machine().deviations {
                    assert_eq!(
                        Some(departure.declaration().scope.mode.as_str()),
                        mode.as_deref(),
                        "{}",
                        path.display()
                    );
                }
            }
        }
    }
    assert!(found >= 6, "the six first-lab models record: found {found}");
    Ok(())
}
