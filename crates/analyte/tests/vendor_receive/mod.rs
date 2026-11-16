//! Tests vendor receipt custody.
#[path = "../support/guard.rs"]
mod guard;
#[path = "../../../protocols/poct/tests/peer_messages.rs"]
mod poct_peer;
use analyte::{Error, installation::config::Program, runtime::service};
use analyte_store::FreeSpace;
use guard::HANG_GUARD;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};

const SECRET: &str = "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

async fn count_results(
    axum::extract::State(count): axum::extract::State<Arc<AtomicU64>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if request.uri().path() == "/results" {
        count.fetch_add(1, Ordering::AcqRel);
    }
    next.run(request).await
}

struct Space(Arc<AtomicU64>);
impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(self.0.load(Ordering::Acquire))
    }
}

struct Running {
    directory: tempfile::TempDir,
    path: std::path::PathBuf,
    // Retaining the bound socket prevents another test from taking this address.
    listener: std::net::TcpListener,
    peer: TcpStream,
    space: Arc<AtomicU64>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<(), Error>>,
    url: String,
    result_requests: Arc<AtomicU64>,
    receiver: analyte_receiver::Receiver,
    receiver_stop: oneshot::Sender<()>,
    receiver_task: JoinHandle<std::io::Result<()>>,
}

impl Running {
    async fn start(definition: &str) -> Result<Self, Error> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        let receiver = analyte_receiver::Receiver::open(
            &root.join("reference.db"),
            "synthetic-api".into(),
            analyte_contract::SigningSecret::parse(SECRET)?,
        )?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let result_requests = Arc::new(AtomicU64::new(0));
        let router = receiver
            .router()
            .layer(axum::middleware::from_fn_with_state(
                result_requests.clone(),
                count_results,
            ));
        let (receiver_stop, receiver_stopped) = oneshot::channel();
        let receiver_task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = receiver_stopped.await;
                })
                .await
        });
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        std::fs::write(root.join("machine.toml"), definition)?;
        std::fs::write(
            root.join("site.toml"),
            include_str!("../../../core/config/examples/site.toml")
                .replace("127.0.0.1:5600", &address.to_string()),
        )?;
        let path = root.join("program.toml");
        std::fs::write(
            &path,
            "schema_version=1\ndata_directory='data'\n[[machines]]\nid='synthetic'\ndefinition='machine.toml'\nsite='site.toml'\n[store]\nreserve_bytes=1024\ndatabase_bytes=100000000\nrecord_bytes=2097152\npage_rows=100\nbusy_timeout_ms=1000\n",
        )?;
        let mut program = Program::load(&path)?;
        program.set_loopback_destination_for_test("demo", url.clone())?;
        program.set_loopback_listener_for_test("synthetic", listener.try_clone()?)?;
        let secrets = analyte::installation::secrets::Environment::load_with(&program, |name| {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    SECRET
                }
                .into(),
            )
        })?;
        let space = Arc::new(AtomicU64::new(u64::MAX));
        let probe = Space(space.clone());
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(service::serve_loopback_test(
            program,
            Box::new(probe),
            async {
                stopped.await?;
                Ok(())
            },
            secrets,
            None,
        ));
        let peer = TcpStream::connect(address).await?;
        Ok(Self {
            directory,
            path,
            listener,
            peer,
            space,
            stop,
            task,
            receiver,
            result_requests,
            url,
            receiver_stop,
            receiver_task,
        })
    }

    async fn restart(self) -> Result<Self, Error> {
        self.stop.send(()).map_err(|()| "service ended early")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        let mut program = Program::load(&self.path)?;
        // The reference receiver remains bound while only the program restarts.
        let url = self.url.clone();
        program.set_loopback_destination_for_test("demo", url)?;
        program.set_loopback_listener_for_test("synthetic", self.listener.try_clone()?)?;
        let secrets = analyte::installation::secrets::Environment::load_with(&program, |name| {
            Some(
                if name.ends_with("API") {
                    "synthetic-api"
                } else {
                    SECRET
                }
                .into(),
            )
        })?;
        let (stop, stopped) = oneshot::channel();
        let probe = Space(self.space.clone());
        let task = tokio::spawn(service::serve_loopback_test(
            program,
            Box::new(probe),
            async {
                stopped.await?;
                Ok(())
            },
            secrets,
            None,
        ));
        let peer = TcpStream::connect(self.listener.local_addr()?).await?;
        let running = Self {
            stop,
            task,
            peer,
            ..self
        };
        tokio::time::timeout(HANG_GUARD, async {
            loop {
                if running.database().is_ok_and(|db| {
                    db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                        .is_ok_and(|version| version == analyte_store::SCHEMA_VERSION)
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        Ok(running)
    }

    fn database(&self) -> Result<rusqlite::Connection, Error> {
        let db = rusqlite::Connection::open_with_flags(
            self.directory.path().join("data/analyte.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_secs(1))?;
        Ok(db)
    }

    fn stored(&self, raw: &[u8], driver: &str) -> Result<(), Error> {
        let db = self.database()?;
        let (metadata, config): (String, Vec<u8>) = db.query_row(
            "SELECT metadata,config FROM receipts WHERE raw=?1 LIMIT 1",
            [raw],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&metadata)?;
        assert_eq!(metadata.driver.namespace.as_str(), driver);
        assert_eq!(metadata.driver.value.as_str(), env!("CARGO_PKG_VERSION"));
        assert_eq!(
            metadata.config_version.as_str(),
            analyte_contract::content_digest(&config)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert_eq!(
            metadata.config_digest.0,
            analyte_contract::content_digest(&config)
        );
        Ok(())
    }

    fn result_value(&self, original: &str) -> Result<(), Error> {
        let db = self.database()?;
        let mut statement = db.prepare("SELECT evidence FROM interpretations")?;
        let evidence = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut found = false;
        for encoded in evidence {
            let interpretation: analyte_store::Interpretation = serde_json::from_str(&encoded)?;
            match interpretation.outcome {
                analyte_store::Reading::Results(results) => {
                    for result in results {
                        assert_eq!(result.processing, analyte_model::ProcessingMode::Unknown);
                        assert!(matches!(
                            result.identity,
                            analyte_model::IdentityStatus::Unresolved(_)
                        ));
                        for part in result.parts.as_slice() {
                            found |= part
                                .values
                                .as_slice()
                                .iter()
                                .any(|v| v.original.as_str() == original);
                        }
                    }
                }
                analyte_store::Reading::Failed(_) => {}
            }
        }
        assert!(
            found,
            "exact original value must survive the program interpretation"
        );
        Ok(())
    }

    fn dilution_original(&self, expected: Option<&str>) -> Result<(), Error> {
        let db = self.database()?;
        let mut statement = db.prepare("SELECT evidence FROM interpretations")?;
        let mut found = false;
        for row in statement.query_map([], |row| row.get::<_, String>(0))? {
            let interpretation: analyte_store::Interpretation = serde_json::from_str(&row?)?;
            if let analyte_store::Reading::Results(results) = interpretation.outcome {
                for result in results {
                    for part in result.parts.as_slice() {
                        found = true;
                        assert_eq!(
                            part.dilution
                                .as_ref()
                                .and_then(|d| d.original.as_ref())
                                .map(analyte_model::SourceText::as_str),
                            expected
                        );
                        if let Some(dilution) = &part.dilution {
                            assert!(dilution.factor.is_none());
                            assert_eq!(dilution.applied, analyte_model::DilutionApplied::Unknown);
                        }
                    }
                }
            }
        }
        assert!(found);
        Ok(())
    }

    fn no_results(&self) -> Result<(), Error> {
        let db = self.database()?;
        let mut statement = db.prepare("SELECT evidence FROM interpretations")?;
        let mut found = false;
        for row in statement.query_map([], |row| row.get::<_, String>(0))? {
            let interpretation: analyte_store::Interpretation = serde_json::from_str(&row?)?;
            match interpretation.outcome {
                analyte_store::Reading::Results(results) => {
                    assert!(results.is_empty());
                    found = true;
                }
                analyte_store::Reading::Failed(_) => {
                    return Err("unknown test must use the mapping-finding path".into());
                }
            }
        }
        assert!(found);
        let alternate: i64 = db.query_row(
            "SELECT count(*) FROM parked WHERE evidence LIKE '%unknown_vendor_test%'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(alternate, 0);
        Ok(())
    }

    async fn parked(&self, reason: &str) -> Result<(), Error> {
        tokio::time::timeout(HANG_GUARD, async {
            loop {
                let db = self.database()?;
                let count: i64 = db.query_row(
                    "SELECT count(*) FROM parked WHERE evidence LIKE ?1",
                    [format!("%{reason}%")],
                    |r| r.get(0),
                )?;
                if count > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, Error>(())
        })
        .await??;
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&self.path)
            .args(["parked", "list"])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8(output.stdout)?.contains(reason));
        assert_eq!(
            self.database()?
                .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }

    async fn close(self) -> Result<(), Error> {
        self.close_with_deliveries(0).await
    }

    async fn close_with_deliveries(mut self, deliveries: u64) -> Result<(), Error> {
        self.stop.send(()).map_err(|()| "service ended early")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        let requests = self.result_requests.load(Ordering::Acquire);
        if deliveries == 0 {
            assert_eq!(requests, 0);
        } else {
            assert!(requests >= deliveries);
        }
        let db = rusqlite::Connection::open(self.directory.path().join("reference.db"))?;
        assert_eq!(
            db.query_row("SELECT count(*) FROM deliveries", [], |r| r
                .get::<_, i64>(0))?,
            i64::try_from(deliveries)?
        );
        self.receiver_stop
            .send(())
            .map_err(|()| "reference ended early")?;
        tokio::time::timeout(HANG_GUARD, self.receiver_task).await???;
        self.receiver
            .shutdown()
            .await
            .map_err(|_| "reference worker panicked")?;
        drop(self.listener);
        Ok(())
    }
}

async fn byte(peer: &mut TcpStream) -> Result<u8, Error> {
    Ok(tokio::time::timeout(HANG_GUARD, peer.read_u8()).await??)
}

async fn xml(peer: &mut TcpStream) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        let mut raw = Vec::new();
        let mut ending = None;
        loop {
            if raw.len() >= 65536 {
                return Err("oversized POCT peer response".into());
            }
            let b = peer.read_u8().await?;
            raw.push(b);
            if ending.is_none() && b == b'>' {
                let root = std::str::from_utf8(
                    raw.get(1..raw.len().saturating_sub(1)).ok_or("XML root")?,
                )?;
                ending = Some(format!("</{root}>").into_bytes());
            }
            if ending.as_ref().is_some_and(|e| raw.ends_with(e)) {
                analyte_poct::Message::parse(&raw, poct_peer::limits())?;
                return Ok::<_, Error>(raw);
            }
        }
    })
    .await?
}

fn vidas_config() -> Result<analyte_framing::vidas::Config, Error> {
    use analyte_framing::vidas::{Checksum, Config, Mode};
    Ok(Config::new(
        Mode::Literal,
        80,
        1920,
        &[],
        Checksum {
            include_stx: false,
            include_suffix: false,
            hex: analyte_framing::vidas::HexOptions {
                uppercase: true,
                accept_either_case: false,
            },
        },
    )?)
}

fn report() -> Result<Vec<u8>, Error> {
    use analyte_autolyser::syntax::{Field, List, PatientCode, Report, Sample, Value};
    Ok(Report::build(
        &PatientCode::from_eight(b"00012aBC")?,
        List::Routine,
        Sample::Serum,
        None,
        &[Value {
            code: Field::new(b"SYN ")?,
            text: Field::new(b"<0.1   ")?,
        }],
    )?
    .raw)
}

async fn poll(peer: &mut TcpStream, expected: u8) -> Result<(), Error> {
    assert_eq!(byte(peer).await?, 2);
    peer.write_all(&[6]).await?;
    assert_eq!(byte(peer).await?, expected);
    assert_eq!(byte(peer).await?, 4);
    Ok(())
}

async fn poct_start(run: &mut Running, hello: &str) -> Result<(), Error> {
    let middle = hello.len() / 2;
    run.peer
        .write_all(hello.get(..middle).ok_or("Hello split")?.as_bytes())
        .await?;
    tokio::time::sleep(Duration::from_millis(20)).await;
    run.peer
        .write_all(hello.get(middle..).ok_or("Hello split")?.as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    run.peer
        .write_all(poct_peer::status("status", "1").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    xml(&mut run.peer).await?;
    Ok(())
}

async fn poct_deliveries(run: &Running, expected: i64) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let db = rusqlite::Connection::open(run.directory.path().join("reference.db"))?;
            let count: i64 = db.query_row("SELECT count(*) FROM deliveries", [], |r| r.get(0))?;
            if count == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, Error>(())
    })
    .await??;
    Ok(())
}

// CLSI POCT01-A2 Appendix B 3.2 pp.71-72, 5.5 p.95, 5.21 Tables 46/49 p.105; ADR 0003.
async fn assert_missing_identity_and_conflict_retained(
    run: &mut Running,
    resent: &str,
    original: &str,
) -> Result<(), Error> {
    let missing_time = resent.replace("resent", "missing-time").replace(
        "<SVC.observation_dttm V=\"2026-03-12T09:35:00+01:00\"/>",
        "<SVC.observation_dttm NULL=\"UNK\"/>",
    );
    run.peer.write_all(missing_time.as_bytes()).await?;
    xml(&mut run.peer).await?;

    run.stored(missing_time.as_bytes(), "analyte-poct")?;
    let missing_sequence = resent
        .replace("resent", "missing-sequence")
        .replace("<SVC.sequence_nbr V=\"0007\"/>", "");
    run.peer.write_all(missing_sequence.as_bytes()).await?;
    xml(&mut run.peer).await?;
    let conflict = original
        .replace("12.30", "99.30")
        .replace("first", "conflict");
    run.peer.write_all(conflict.as_bytes()).await?;
    xml(&mut run.peer).await?;
    let db = run.database()?;
    assert_eq!(
        db.query_row("SELECT count(*) FROM observation_revisions", [], |r| r
            .get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        2
    );
    let mut statement = db.prepare(
        "SELECT i.evidence FROM interpretations i JOIN receipts r ON i.receipt=r.id WHERE r.raw=?1",
    )?;
    let encoded: String = statement
        .query_row([missing_time.as_bytes()], |r| r.get(0))
        .map_err(|e| format!("missing time interpretation: {e}"))?;
    let evidence: analyte_store::Interpretation = serde_json::from_str(&encoded)?;
    assert!(
        matches!(evidence.outcome, analyte_store::Reading::Results(results) if results.iter().all(|r| matches!(r.identity, analyte_model::IdentityStatus::Unresolved(_))))
    );
    assert!(
        db.query_row(
            "SELECT count(*) FROM parked WHERE evidence LIKE '%identity evidence missing%'",
            [],
            |r| r.get::<_, i64>(0)
        )? >= 2
    );
    assert!(
        db.query_row(
            "SELECT count(*) FROM parked WHERE evidence LIKE '%conflict%'",
            [],
            |r| r.get::<_, i64>(0)
        )? >= 1
    );
    drop(statement);
    drop(db);
    Ok(())
}

async fn assert_missing_device_retained(run: &mut Running, hello: &str) -> Result<(), Error> {
    run.peer = TcpStream::connect(run.listener.local_addr()?).await?;
    let missing_device = hello.replace("<DEV.device_id V=\"A1-B2-C3-D4-E5-F6-07-08\"/>", "");
    run.peer.write_all(missing_device.as_bytes()).await?;
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if run.database()?.query_row(
                "SELECT count(*) FROM parked WHERE evidence LIKE '%DEV.device_id%'",
                [],
                |r| r.get::<_, i64>(0),
            )? > 0
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, Error>(())
    })
    .await??;
    run.stored(missing_device.as_bytes(), "analyte-poct")
        .map_err(|e| format!("missing device receipt: {e}"))?;
    assert_eq!(
        run.database()?
            .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
        2
    );
    assert!(
        run.database()?.query_row(
            "SELECT count(*) FROM parked WHERE evidence LIKE '%DEV.device_id%'",
            [],
            |r| r.get::<_, i64>(0)
        )? >= 1
    );
    Ok(())
}

// POCT01-A2 Appendix B 4.1.2 p.78, 3.4.1 p.73: error ACK precedes abnormal END.

mod scenarios_1;

mod edits;
mod identity;
