//! Tests shipped machine definitions and recorded conversations.
use super::*;
use analyte_config::{Draft, EvidenceLevel};
use std::path::PathBuf;

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
            if readme.is_file() && !folder.join("machine.toml").is_file() {
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
            } else if entry.file_name() == "machine.toml" {
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

// ADR 0012 §5; ADR 0009
#[tokio::test]
async fn every_machine_validates_and_replays() -> Result<(), Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines");
    let found = definitions(&root)?;
    if found.is_empty() {
        println!("No machine definitions found; validation and replay inventory is empty.");
    }
    for definition in found {
        let folder = definition.parent().ok_or("machine directory missing")?;
        assert_eq!(
            folder.strip_prefix(&root)?.components().count(),
            2,
            "machines/<vendor>/<model> required"
        );
        let text = std::fs::read_to_string(&definition)?;
        let raw: toml::Value = toml::from_str(&text)?;
        let folder_transport = raw
            .get("transport")
            .and_then(|t| t.get("kind"))
            .and_then(toml::Value::as_str)
            == Some("watched_folder");
        let endpoint = if folder_transport {
            std::env::temp_dir().to_string_lossy().into_owned()
        } else {
            "127.0.0.1:5600".into()
        };
        let site = site(&endpoint)?;
        let checked = Draft::load(
            ("machine", &text),
            ("site", &site),
            ("overrides", "schema_version=1"),
        )?
        .validate()?;
        readme(
            &folder.join("README.md"),
            checked.effective().machine().evidence.level,
        )?;
        let mut recordings = std::fs::read_dir(folder.join("recordings"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?;
        recordings.sort();
        for recording in recordings {
            assert!(
                recording.to_string_lossy().ends_with(".rec.jsonl"),
                "unexpected recording entry: {}",
                recording.display()
            );
            let (header, events) = analyte_transport::recording::read(
                std::fs::File::open(&recording)?,
                analyte_transport::recording::Limits::default(),
            )?;
            assert_eq!(
                header.stack,
                checked.effective().stack().layer_names(),
                "{}: recording stack",
                recording.display()
            );
            replay_machine(&text, &events, &header, checked.effective().stack()).await?;
            println!(
                "Validated {} and replayed {}",
                definition.display(),
                recording.display()
            );
        }
    }
    Ok(())
}

async fn replay_machine(
    definition: &str,
    events: &[analyte_transport::recording::Event],
    header: &analyte_transport::recording::Header,
    stack: &analyte_config::Stack,
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
    analyte_fixtures::write_definition(temp.path(), "synthetic", definition)?;
    std::fs::write(temp.path().join("site.toml"), site(&endpoint)?)?;
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
        analyte_fixtures::program(
            "[[machines]]\nid='fixture'\ndefinition='test/synthetic/machine.toml'\nsite='site.toml'\noverrides='overrides.toml'\n",
        ),
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
            analyte_sim::replay(&mut peer, events, &header.exceptions, deadline).await?;
        }
        Ok::<_, Error>(())
    }
    .await;
    let stopped = running.stop().await;
    reference.close().await?;
    replayed?;
    stopped
}

fn site(endpoint: &str) -> Result<String, Error> {
    let mut site: toml::Table =
        toml::from_str(include_str!("../../../core/config/examples/site.toml"))?;
    site.insert("endpoint".into(), toml::Value::String(endpoint.into()));
    Ok(toml::to_string(&site)?)
}

async fn replay_file(
    path: &Path,
    incoming: &Path,
    events: &[analyte_transport::recording::Event],
) -> Result<(), Error> {
    use analyte_transport::recording::Direction;
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
