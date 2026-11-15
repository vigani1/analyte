//! Synthetic program-to-peer custody tests, ADR 0003 and documented vendor scopes.
#[path = "../../poct/tests/common/mod.rs"]
mod poct_peer;
use analyte::{Error, config::Program, service};
use analyte_store::FreeSpace;
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
            include_str!("../../config/examples/site.toml")
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
        let secrets = analyte::secrets::EnvironmentSecrets::load_with(&program, |name| {
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
        tokio::time::timeout(Duration::from_secs(8), self.task).await???;
        let mut program = Program::load(&self.path)?;
        // The reference receiver remains bound while only the program restarts.
        let url = self.url.clone();
        program.set_loopback_destination_for_test("demo", url)?;
        program.set_loopback_listener_for_test("synthetic", self.listener.try_clone()?)?;
        let secrets = analyte::secrets::EnvironmentSecrets::load_with(&program, |name| {
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
        ));
        let peer = TcpStream::connect(self.listener.local_addr()?).await?;
        let running = Self {
            stop,
            task,
            peer,
            ..self
        };
        tokio::time::timeout(Duration::from_secs(10), async {
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
        assert_eq!(metadata.config_version.as_str(), "1");
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
        tokio::time::timeout(Duration::from_secs(5), async {
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
        self.stop.send(()).map_err(|()| "service ended early")?;
        tokio::time::timeout(Duration::from_secs(8), self.task).await???;
        assert_eq!(self.result_requests.load(Ordering::Acquire), 0);
        let db = rusqlite::Connection::open(self.directory.path().join("reference.db"))?;
        assert_eq!(
            db.query_row("SELECT count(*) FROM deliveries", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        self.receiver_stop
            .send(())
            .map_err(|()| "reference ended early")?;
        tokio::time::timeout(Duration::from_secs(3), self.receiver_task).await???;
        self.receiver
            .shutdown()
            .map_err(|_| "reference worker panicked")?;
        drop(self.listener);
        Ok(())
    }
}

async fn byte(peer: &mut TcpStream) -> Result<u8, Error> {
    Ok(tokio::time::timeout(Duration::from_secs(5), peer.read_u8()).await??)
}

async fn xml(peer: &mut TcpStream) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
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

fn vidas_config() -> Result<analyte_vidas::framing::Config, Error> {
    use analyte_vidas::framing::{Checksum, Config, Mode};
    Ok(Config::new(
        Mode::Literal,
        80,
        1920,
        &[],
        Checksum {
            include_stx: false,
            include_suffix: false,
            uppercase: true,
            accept_either_case: false,
        },
    )?)
}

#[tokio::test]
async fn vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery()
-> Result<(), Error> {
    let mut run = Running::start(include_str!("../../config/examples/vidas_literal.toml")).await?;
    run.peer.write_all(&[5]).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    let packet = vidas_config()?
        .encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn<0.1|y3mg/L|qd1:10|")?;
    run.peer.write_all(&packet).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    run.stored(&packet, "analyte-vidas")?;
    run.parked("VIDAS Literal lacks trusted result/run identity")
        .await?;
    run.result_value("<0.1")?;
    run.dilution_original(Some("1:10"))?;
    // Literal's final controls may share one read, after custody of the packet.
    run.peer.write_all(&[3, 4]).await?;
    run.close().await
}

#[tokio::test]
async fn poct01_b3_2_adr0003_stored_before_ack_unresolved_parked_no_delivery() -> Result<(), Error>
{
    let mut run = Running::start(include_str!("../../config/examples/poct.toml")).await?;
    let hello = poct_peer::hello("hello");
    run.peer.write_all(hello.as_bytes()).await?;
    assert!(String::from_utf8(xml(&mut run.peer).await?)?.contains("ACK.type_cd V=\"AA\""));
    run.stored(hello.as_bytes(), "analyte-poct")?;
    run.peer
        .write_all(poct_peer::status("status", "1").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    xml(&mut run.peer).await?;
    let observation =
        poct_peer::observation("observation", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>");
    // Split XML deliberately, independent of transport read boundaries.
    let middle = observation.len() / 2;
    run.peer
        .write_all(observation.get(..middle).ok_or("split")?.as_bytes())
        .await?;
    run.peer
        .write_all(observation.get(middle..).ok_or("split")?.as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    run.stored(observation.as_bytes(), "analyte-poct")?;
    run.parked("POCT service sequence may wrap").await?;
    run.result_value("12.30")?;
    run.dilution_original(None)?;
    run.close().await
}

fn report() -> Result<Vec<u8>, Error> {
    use analyte_autolyser::records::{Field, List, PatientCode, Report, Sample, Value};
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

#[tokio::test]
async fn autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l()
-> Result<(), Error> {
    let mut run = Running::start(include_str!("../../config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    let good = report()?;
    let mut corrupt = good.clone();
    *corrupt.get_mut(0).ok_or("fixture")? = b'9';
    run.peer.write_all(&corrupt).await?;
    run.parked("checksum mismatch").await?;
    run.stored(&corrupt, "analyte-autolyser")?;
    poll(&mut run.peer, b'L').await?;
    run.peer.write_all(&[21]).await?;
    // An empty L must keep recovery active; it never permits the next R.
    poll(&mut run.peer, b'L').await?;
    run.peer.write_all(&good).await?;
    run.parked("Autolyser Rev18 has no immutable run identity")
        .await?;
    run.stored(&good, "analyte-autolyser")?;
    run.result_value("<0.1   ")?;
    run.dilution_original(None)?;
    poll(&mut run.peer, b'R').await?;
    run.peer.write_all(&[21]).await?;
    run.close().await
}

#[tokio::test]
async fn adr0003_vendor_storage_failure_never_acknowledges_data() -> Result<(), Error> {
    for definition in [
        include_str!("../../config/examples/vidas_literal.toml"),
        include_str!("../../config/examples/poct.toml"),
        include_str!("../../config/examples/autolyser.toml"),
    ] {
        let mut run = Running::start(definition).await?;
        let raw = if definition.contains("[vidas]") {
            run.peer.write_all(&[5]).await?;
            assert_eq!(byte(&mut run.peer).await?, 6);
            vidas_config()?.encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn12.3|")?
        } else if definition.contains("[autolyser]") {
            poll(&mut run.peer, b'R').await?;
            report()?
        } else {
            // Verify service startup before withdrawing storage capacity.
            let hello = poct_peer::hello("hello");
            run.peer.write_all(hello.as_bytes()).await?;
            xml(&mut run.peer).await?;
            poct_peer::status("status", "1").into_bytes()
        };
        run.space.store(0, Ordering::Release);
        run.peer.write_all(&raw).await?;
        let mut reply = [0u8; 256];
        let received =
            tokio::time::timeout(Duration::from_secs(5), run.peer.read(&mut reply)).await?;
        match received {
            Ok(count) => assert_eq!(count, 0, "storage failure must close without acceptance"),
            Err(error) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            )),
        }
        run.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn autolyser_rev18_p175_adr0003_uncertain_r_blocks_restart_until_operator_reconciliation()
-> Result<(), Error> {
    let mut run = Running::start(include_str!("../../config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    // No response: neither timeout nor disconnect proves that R did not advance.
    let mut run = run.restart().await?;
    let mut bytes = [0u8; 1];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), run.peer.read(&mut bytes)).await??,
        0
    );
    run.parked("Autolyser R may advance its cursor").await?;
    run.close().await
}

#[tokio::test]
async fn vidas_literal_rule2_unknown_test_is_unmapped_and_operator_visible() -> Result<(), Error> {
    let mut run = Running::start(include_str!("../../config/examples/vidas_literal.toml")).await?;
    run.peer.write_all(&[5]).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    let packet = vidas_config()?.encode(b"mtrsl|pi|si|ci00012a|rtMISS|tt|td|qlpositive|qn<0.1|")?;
    run.peer.write_all(&packet).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    run.parked("UnknownTest").await?;
    run.stored(&packet, "analyte-vidas")?;
    run.no_results()?;
    run.peer.write_all(&[3, 4]).await?;
    run.close().await
}

#[tokio::test]
async fn autolyser_rev18_rule2_unknown_test_is_unmapped_and_operator_visible() -> Result<(), Error>
{
    use analyte_autolyser::records::{Field, Report};
    let mut run = Running::start(include_str!("../../config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    let mut report = Report::parse(&report()?, false)?;
    report.values.first_mut().ok_or("fixture value")?.code = Field::new(b"MISS")?;
    let raw = Report::build(
        &report.patient,
        report.list,
        report.sample,
        None,
        &report.values,
    )?
    .raw;
    run.peer.write_all(&raw).await?;
    run.parked("UnknownTest").await?;
    run.stored(&raw, "analyte-autolyser")?;
    run.no_results()?;
    run.close().await
}

#[tokio::test]
async fn poct01_b6_rule2_unknown_test_is_unmapped_and_operator_visible() -> Result<(), Error> {
    let mut run = Running::start(include_str!("../../config/examples/poct.toml")).await?;
    run.peer
        .write_all(poct_peer::hello("hello").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    run.peer
        .write_all(poct_peer::status("status", "1").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    xml(&mut run.peer).await?;
    let raw = poct_peer::observation("unknown", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>")
        .replace("synthetic-analyte", "unknown-analyte");
    run.peer.write_all(raw.as_bytes()).await?;
    xml(&mut run.peer).await?;
    run.parked("UnknownTest").await?;
    run.stored(raw.as_bytes(), "analyte-poct")?;
    run.no_results()?;
    run.close().await
}
