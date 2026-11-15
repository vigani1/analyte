//! ADR 0012 section 5: shipped machine definitions and ADR 0009 conversations.
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

#[tokio::test]
async fn adr0012_5_adr0009_every_machine_validates_and_replays() -> Result<(), Error> {
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
    std::fs::write(temp.path().join("machine.toml"), definition)?;
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
        "schema_version=1\ndata_directory='data'\n[[machines]]\nid='fixture'\ndefinition='machine.toml'\nsite='site.toml'\noverrides='overrides.toml'\n[store]\nreserve_bytes=1024\ndatabase_bytes=100000000\nrecord_bytes=2097152\npage_rows=100\nbusy_timeout_ms=1000\n",
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
    let mut site: toml::Table = toml::from_str(include_str!("../../../config/examples/site.toml"))?;
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
