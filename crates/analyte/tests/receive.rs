//! Synthetic localhost receive-path tests, tied to ADR 0003 and protocol scopes.
#[path = "support/guard.rs"]
mod guard;
use analyte::{Error, installation::config::Program, runtime::service};
use analyte_store::{FreeSpace, Store};
use guard::HANG_GUARD;
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
};

struct Space(u64);
impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(self.0)
    }
}

fn config(temp: &Path, astm: bool) -> Result<std::path::PathBuf, Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let definition = if astm {
        include_str!("../../config/examples/astm.toml").replace("kind = \"serial\"", "kind = \"tcp_server\"").replace("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "").replace("transport = \"serial\"", "transport = \"tcp_server\"")
    } else {
        include_str!("../../config/examples/hl7.toml").to_owned()
    };
    std::fs::write(temp.join("machine.toml"), definition)?;
    std::fs::write(
        temp.join("site.toml"),
        include_str!("../../config/examples/site.toml")
            .replace("127.0.0.1:5600", &address.to_string()),
    )?;
    let path = temp.join("program.toml");
    std::fs::write(
        &path,
        "schema_version = 1\ndata_directory = 'data'\n[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n[store]\nreserve_bytes = 1024\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 100\nbusy_timeout_ms = 1000\n",
    )?;
    Ok(path)
}

#[test]
fn law_tf2b_b6_single_persistent_manager_accepts_client_and_server_without_startup_io()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    std::fs::write(
        temp.path().join("machine.toml"),
        include_str!("../../config/examples/law.toml"),
    )?;
    let loaded = Program::load(&path)?;
    assert_eq!(
        loaded
            .machines()
            .first()
            .ok_or("LAW machine absent")?
            .snapshot()
            .effective()
            .stack()
            .layer_names()[2],
        "law_manager"
    );
    assert!(!temp.path().join("data").exists());
    std::fs::write(
        temp.path().join("machine.toml"),
        include_str!("../../config/examples/law.toml")
            .replace("kind = \"tcp_server\"", "kind = \"tcp_client\""),
    )?;
    let loaded = Program::load(&path)?;
    assert_eq!(
        loaded
            .machines()
            .first()
            .ok_or("LAW machine absent")?
            .snapshot()
            .effective()
            .stack()
            .transport_kind(),
        analyte_config::TransportKind::TcpClient
    );
    assert!(!temp.path().join("data").exists());
    Ok(())
}

async fn connect(program: &Program) -> Result<TcpStream, Error> {
    let endpoint = &program
        .machines()
        .first()
        .ok_or("missing machine")?
        .snapshot()
        .effective()
        .site()
        .endpoint;
    let until = tokio::time::Instant::now()
        .checked_add(HANG_GUARD)
        .unwrap_or(tokio::time::Instant::now());
    loop {
        if let Ok(stream) = TcpStream::connect(endpoint).await {
            return Ok(stream);
        }
        if tokio::time::Instant::now() >= until {
            return Err("service did not start".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn stored(program: &Program) -> Result<Store, Error> {
    Ok(Store::open(
        &program.data_directory().join("analyte.db"),
        analyte_store::Limits {
            reserve_bytes: 1024,
            database_bytes: 100000000,
            record_bytes: 2097152,
            page_rows: 100,
            busy_timeout: Duration::from_millis(20),
        },
        Box::new(Space(u64::MAX)),
    )?)
}

async fn read_ack(peer: &mut TcpStream) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        let mut bytes = Vec::new();
        while bytes.len() < 4096 {
            let byte = peer.read_u8().await?;
            bytes.push(byte);
            if bytes.ends_with(b"\x1c\r") {
                return Ok::<_, Error>(bytes);
            }
        }
        Err("ACK exceeds bound".into())
    })
    .await?
}

const ORU: &[u8] = b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";

#[tokio::test]
async fn hl7_251_2_15_5_2_adr0003_ae_reports_actual_mapping_location() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    let wire = analyte_framing::mllp::encode(ORU, analyte_framing::mllp::Limit::new(1048576)?)?;
    peer.write_all(&wire).await?;
    let ack = read_ack(&mut peer).await?;
    assert!(String::from_utf8_lossy(&ack).contains("MSA|AA|message1"));
    let store = stored(&program)?;
    let receipts = store.receipts("")?;
    assert_eq!(receipts.len(), 1);
    let receipt = receipts.first().ok_or("receipt missing")?;
    assert_eq!(store.receipt(receipt.id)?.ok_or("raw missing")?.raw, wire);
    let interpretation = store
        .interpretation(receipt.id, 1)?
        .ok_or("interpretation missing before AA")?;
    assert!(matches!(interpretation.outcome, analyte_store::Reading::Results(r) if r.len() == 1));
    assert_eq!(
        store
            .health(
                &analyte_model::Identifier::new("demo")?,
                jiff::Timestamp::now().as_second(),
                30
            )?
            .pending_count,
        1
    );
    drop(store);
    let mut bad = String::from_utf8(ORU.to_vec())?.replace("message1", "message2");
    bad.push_str("OBX|2|NM|UNKNOWN^Synthetic^synthetic||12.30|mg/L|||||F\r");
    peer.write_all(&analyte_framing::mllp::encode(
        bad.as_bytes(),
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    let ack = String::from_utf8(read_ack(&mut peer).await?)?;
    assert!(ack.contains("MSA|AE|message2"));
    assert!(ack.contains("ERR||OBX^2^3|207|E|operator-review"), "{ack}");
    assert!(!stored(&program)?.parked(0)?.is_empty());
    assert_eq!(
        stored(&program)?
            .health(
                &analyte_model::Identifier::new("demo")?,
                jiff::Timestamp::now().as_second(),
                30
            )?
            .pending_count,
        1
    );
    let conflict = String::from_utf8(ORU.to_vec())?
        .replace("message1", "message3")
        .replace("12.30", "99.10");
    peer.write_all(&analyte_framing::mllp::encode(
        conflict.as_bytes(),
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    let ack = String::from_utf8(read_ack(&mut peer).await?)?;
    assert!(ack.contains("MSA|AE|message3"));
    assert!(ack.contains("ERR|||207|E|operator-review"), "{ack}");
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

#[tokio::test]
async fn lis01_6_3_adr0003_astm_journal_precedes_ack_and_message_is_interpreted()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    let payload = b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|00012\rR|1|^^^SYN|12.30|mg/L||N||F\rL|1|N\r";
    let frames = analyte_framing::astm::encode(
        payload,
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::new(64000)?,
    )?;
    let mut wire = Vec::new();
    for frame in frames {
        wire.extend_from_slice(&frame);
        peer.write_all(&frame).await?;
        assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
        let store = stored(&program)?;
        let journal = rusqlite::Connection::open_with_flags(
            program.data_directory().join("analyte.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let transfer: String = journal.query_row(
            "SELECT id FROM transfers WHERE state!='complete' ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        let transfer = analyte_model::Identifier::new(&transfer)?;
        assert!(!store.frames(&transfer, 0)?.is_empty());
    }
    peer.write_all(b"\x04").await?;
    let until = tokio::time::Instant::now()
        .checked_add(HANG_GUARD)
        .unwrap_or(tokio::time::Instant::now());
    loop {
        let store = stored(&program)?;
        let receipts = store.receipts("")?;
        if let Some(receipt) = receipts.first()
            && let Some(interpretation) = store.interpretation(receipt.id, 1)?
        {
            assert_eq!(store.receipt(receipt.id)?.ok_or("raw absent")?.raw, wire);
            assert!(
                matches!(interpretation.outcome, analyte_store::Reading::Results(r) if r.len() == 1)
            );
            break;
        }
        if tokio::time::Instant::now() >= until {
            return Err("ASTM interpretation absent".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

#[tokio::test]
async fn adr0003_full_disk_refuses_astm_enq_and_shutdown_is_bounded() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(Program::load(&path)?, Box::new(Space(0)), async {
        stop.await?;
        Ok(())
    }));
    let mut peer = connect(&program).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 21);
    assert!(stored(&program)?.receipts("")?.is_empty());
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

#[test]
fn adr0002_check_config_prints_effective_digest_and_documented_evidence() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    for astm in [true, false] {
        let path = config(temp.path(), astm)?;
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("check-config")
            .arg(path)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout)?;
        let fields: Vec<_> = stdout.split_whitespace().collect();
        assert_eq!(fields.first(), Some(&"synthetic"));
        assert_eq!(fields.get(1).ok_or("digest missing")?.len(), 64);
        assert_eq!(fields.get(2), Some(&"documented"));
    }
    Ok(())
}

#[tokio::test]
async fn adr0003_hl7_full_disk_never_returns_positive_ack() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(Program::load(&path)?, Box::new(Space(0)), async {
        stop.await?;
        Ok(())
    }));
    let mut peer = connect(&program).await?;
    peer.write_all(&analyte_framing::mllp::encode(
        ORU,
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    let ack = read_ack(&mut peer).await?;
    assert!(String::from_utf8_lossy(&ack).contains("MSA|AR|message1"));
    assert!(stored(&program)?.receipts("")?.is_empty());
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

#[tokio::test]
async fn plan5_machine_bind_failure_does_not_stop_another_machine() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    std::fs::write(
        temp.path().join("occupied-site.toml"),
        include_str!("../../config/examples/site.toml")
            .replace("127.0.0.1:5600", &occupied.local_addr()?.to_string()),
    )?;
    let mut file = std::fs::read_to_string(&path)?;
    file.push_str(
        "\n[[machines]]\nid='occupied'\ndefinition='machine.toml'\nsite='occupied-site.toml'\n",
    );
    std::fs::write(&path, file)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    peer.write_all(&analyte_framing::mllp::encode(
        ORU,
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    assert!(String::from_utf8_lossy(&read_ack(&mut peer).await?).contains("MSA|AA|message1"));
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

#[tokio::test]
async fn plan11_operator_list_show_resolve_are_audited_and_backup_is_readable() -> Result<(), Error>
{
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let mut handle = service::open_store(&program, Box::new(Space(u64::MAX))).await?;
    let id = handle
        .request(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(2))
                .ok_or("deadline overflow")?,
            |store| {
                store.park(&analyte_store::Parked {
                    kind: analyte_model::Identifier::new("synthetic")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    reason: analyte_model::Reason::new("synthetic operator test")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    targets: Vec::new(),
                })
            },
        )
        .await?;
    handle
        .shutdown(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(2))
                .ok_or("deadline overflow")?,
        )
        .await?;
    for arguments in [
        vec!["parked".to_owned(), "list".to_owned()],
        vec!["parked".to_owned(), "show".to_owned(), id.to_string()],
        vec![
            "parked".to_owned(),
            "resolve".to_owned(),
            id.to_string(),
            "--reason".to_owned(),
            "reviewed synthetic evidence".to_owned(),
        ],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&path)
            .arg("--actor")
            .arg("synthetic-operator")
            .args(arguments)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        stored(&program)?
            .parked(0)?
            .first()
            .ok_or("parked item absent")?
            .resolution
            .is_some()
    );
    let audits = stored(&program)?.audits(0)?;
    assert_eq!(audits.len(), 3);
    assert!(
        audits
            .iter()
            .all(|(_, audit)| audit.actor.as_str() == "synthetic-operator")
    );
    assert_eq!(
        audits
            .iter()
            .map(|(_, audit)| audit.action.as_str())
            .collect::<Vec<_>>(),
        vec!["parked-list", "parked-show", "parked-resolve"]
    );
    let target = temp.path().join("backup.db");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("--config")
        .arg(&path)
        .arg("backup")
        .arg(&target)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.is_file());
    Ok(())
}

#[tokio::test]
async fn plan11_native_capacity_probe_is_bounded_and_file_pickup_keeps_original()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let incoming = temp.path().join("incoming");
    std::fs::create_dir(&incoming)?;
    let definition = std::fs::read_to_string(temp.path().join("machine.toml"))?
        .replace("tcp_server", "watched_folder")
        .replace("kind = \"mllp\"", "kind = \"none\"")
        .replace("hl7_receiver", "file_pickup")
        .replace("acknowledgment = \"store_and_forward\"\n", "");
    std::fs::write(temp.path().join("machine.toml"), definition)?;
    let site = format!(
        "schema_version=1\nendpoint={}\n[destination]\nname='demo'\nbase_url='https://lab.example.invalid'\napi_key_name='demo-api'\nsigning_key_name='demo-signing'\n",
        toml::Value::String(incoming.to_string_lossy().into_owned())
    );
    std::fs::write(temp.path().join("site.toml"), site)?;
    let program = Program::load(&path)?;
    let mut store = analyte::open_system_store(&program).await?;
    store
        .request(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or("deadline overflow")?,
            |s| s.admission(1),
        )
        .await?;
    store
        .shutdown(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or("deadline overflow")?,
        )
        .await?;
    // The producer renames a complete file into incoming; partial files stay outside.
    std::fs::write(temp.path().join("producer.tmp"), ORU)?;
    std::fs::rename(
        temp.path().join("producer.tmp"),
        incoming.join("result.hl7"),
    )?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let until = tokio::time::Instant::now()
        .checked_add(HANG_GUARD)
        .ok_or("deadline overflow")?;
    loop {
        let store = stored(&program)?;
        let receipts = store.receipts("")?;
        if let Some(receipt) = receipts.first()
            && store.interpretation(receipt.id, 1)?.is_some()
        {
            assert_eq!(
                store
                    .receipt(receipt.id)?
                    .ok_or("file receipt missing")?
                    .raw,
                ORU
            );
            break;
        }
        if tokio::time::Instant::now() >= until {
            return Err("file was not interpreted".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    assert!(!incoming.join("result.hl7").exists());
    Ok(())
}

#[tokio::test]
async fn adr0003_shutdown_parks_incomplete_astm_frame_and_keeps_journal_provenance()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    peer.write_all(b"\x05").await?;
    assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    let first = analyte_framing::astm::encode(
        b"H|\\^&||||||||||P|LIS2-A2|20261115120000\r",
        analyte_framing::astm::FrameNumber::FIRST,
        analyte_framing::astm::Limit::new(64000)?,
    )?;
    for frame in first {
        peer.write_all(&frame).await?;
        assert_eq!(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??, 6);
    }
    let store = stored(&program)?;
    // Inspect the durable journal written by the real receive path before its ACK.
    let journal = rusqlite::Connection::open_with_flags(
        program.data_directory().join("analyte.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let evidence: String = journal.query_row(
        "SELECT evidence FROM frame_provenance WHERE transfer=(SELECT id FROM transfers WHERE state!='complete' ORDER BY id LIMIT 1)",
        [], |row| row.get(0),
    )?;
    let provenance: analyte_store::FrameProvenance = serde_json::from_str(&evidence)?;
    assert_eq!(
        provenance.config,
        program
            .machines()
            .first()
            .ok_or("machine missing")?
            .snapshot()
            .canonical()
            .as_bytes()
    );
    drop(store);
    peer.write_all(b"\x022unfinished").await?;
    // End the socket so its ordered Received event precedes Closed deterministically.
    peer.shutdown().await?;
    let until = tokio::time::Instant::now()
        .checked_add(HANG_GUARD)
        .ok_or("deadline overflow")?;
    loop {
        let store = stored(&program)?;
        if store.parked(0)?.len() >= 2 {
            assert!(store.receipts("")?.iter().any(|receipt| {
                store
                    .raw(receipt.id)
                    .ok()
                    .flatten()
                    .is_some_and(|raw| raw == b"\x022unfinished")
            }));
            break;
        }
        if tokio::time::Instant::now() >= until {
            return Err("partial ASTM frame not parked".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}

async fn serve(
    program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), Error>>,
) -> Result<(), Error> {
    let secrets =
        analyte::installation::secrets::EnvironmentSecrets::load_with(&program, |name| {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
                }
                .into(),
            )
        })?;
    service::serve_loopback_test(program, space, stop, secrets, None).await
}

#[tokio::test]
async fn plan11_status_reports_health_and_last_receipt_without_payloads_or_secrets()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let mut store = service::open_store(&program, Box::new(Space(u64::MAX))).await?;
    let lines = analyte::operator::status::snapshot(&program, &store).await?;
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "order_sending_paused_after_restore=false");
    let destination = lines.get(1).ok_or("missing health")?;
    for field in [
        "pending_count=0",
        "pending_bytes=0",
        "oldest_pending_age_seconds=None",
        "paused=false",
        "last_poll=None",
        "cache_trusted=false",
    ] {
        assert!(destination.contains(field));
    }
    assert_eq!(
        lines.last().ok_or("missing machine")?,
        "machine=synthetic last_receipt=unknown"
    );
    store
        .shutdown(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or("deadline")?,
        )
        .await?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
        .arg("status")
        .arg(&path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("destination=demo"));
    assert!(!text.contains("demo-api"));
    assert!(!text.contains("demo-signing"));
    Ok(())
}

// Plan section 11 and ADR 0003: real CLI commands require actor/reason and reuse
// the live stored-byte interpretation; neither reprocess nor resend touches orders.
#[tokio::test]
async fn plan11_cli_reprocess_receipt_parked_and_bounded_delivery_list_require_actor_reason()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let definition = std::fs::read_to_string(temp.path().join("machine.toml"))?;
    std::fs::write(
        temp.path().join("machine.toml"),
        definition.replace("machine_code=\"SYN\"", "machine_code=\"OTHER\""),
    )?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let owner = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    peer.write_all(&analyte_framing::mllp::encode(
        ORU,
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    assert!(String::from_utf8(read_ack(&mut peer).await?)?.contains("MSA|AE"));
    shutdown.send(()).map_err(|_| "shutdown lost")?;
    tokio::time::timeout(HANG_GUARD, owner).await???;
    let store = stored(&program)?;
    let receipt = store.receipts("")?.first().ok_or("receipt missing")?.id;
    let parked = store.parked(0)?.first().ok_or("parked missing")?.id;
    let raw = store.receipt(receipt)?.ok_or("receipt missing")?.raw;
    drop(store);
    std::fs::write(temp.path().join("machine.toml"), definition)?;
    let receipt = uuid::Uuid::from(receipt).to_string();
    let parked = parked.to_string();
    for args in [
        vec!["parked", "reprocess", &parked],
        vec!["receipts", "reprocess", &receipt],
        vec!["deliveries", "list"],
    ] {
        for suffix in [
            vec!["--reason", "mapping repaired"],
            vec!["--actor", "operator"],
        ] {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
                .arg("--config")
                .arg(&path)
                .args(&args)
                .args(suffix)
                .output()?;
            assert!(!output.status.success());
        }
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&path)
            .args(&args)
            .args(["--actor", "operator", "--reason", "mapping repaired"])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args.first() == Some(&"deliveries") {
            assert!(!output.stdout.is_empty());
        }
    }
    let store = stored(&program)?;
    let receipt = analyte_model::ReceiptId::try_from(uuid::Uuid::parse_str(&receipt)?)?;
    assert_eq!(store.receipt(receipt)?.ok_or("receipt missing")?.raw, raw);
    assert!(
        store
            .parked_item(parked.parse()?)?
            .ok_or("parked missing")?
            .resolution
            .is_some()
    );
    assert_eq!(store.deliveries(0)?.len(), 1);
    assert_eq!(store.audits(0)?.len(), 3);
    assert!(
        store
            .audits(0)?
            .iter()
            .all(|(_, a)| a.actor.as_str() == "operator")
    );
    Ok(())
}

// ADR 0003 / AGENTS.md safety rule 3: provenance belongs to the connection that read bytes.
#[tokio::test]
async fn adr0003_rule3_connections_have_distinct_provenance() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let program = Program::load(&path)?;
    let (shutdown, stop) = oneshot::channel();
    let service = tokio::spawn(serve(
        Program::load(&path)?,
        Box::new(Space(u64::MAX)),
        async {
            stop.await?;
            Ok(())
        },
    ));
    let mut peer = connect(&program).await?;
    for control in ["message1", "message2"] {
        let raw = String::from_utf8(ORU.to_vec())?.replace("message1", control);
        peer.write_all(&analyte_framing::mllp::encode(
            raw.as_bytes(),
            analyte_framing::mllp::Limit::new(1048576)?,
        )?)
        .await?;
        read_ack(&mut peer).await?;
    }
    drop(peer);
    let mut peer = connect(&program).await?;
    let raw = String::from_utf8(ORU.to_vec())?.replace("message1", "message3");
    peer.write_all(&analyte_framing::mllp::encode(
        raw.as_bytes(),
        analyte_framing::mllp::Limit::new(1048576)?,
    )?)
    .await?;
    read_ack(&mut peer).await?;
    let store = stored(&program)?;
    let receipts = store.receipts("")?;
    assert_eq!(receipts.len(), 3);
    let metadata = receipts
        .iter()
        .map(|r| {
            store
                .receipt(r.id)?
                .ok_or("receipt absent")
                .map(|r| r.metadata)
                .map_err(Error::from)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    assert_eq!(metadata[0].connection, metadata[1].connection);
    assert_ne!(metadata[0].connection, metadata[2].connection);
    assert_ne!(metadata[0].connection, metadata[0].machine);
    drop(store);
    shutdown.send(()).map_err(|()| "shutdown receiver gone")?;
    tokio::time::timeout(HANG_GUARD, service).await???;
    Ok(())
}
