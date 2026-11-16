//! Tests bounded synthetic peers and local test fixtures.
mod deadline;
mod guard;
pub(crate) use deadline::step;
pub(super) use guard::HANG_GUARD;

use analyte::{Error, installation::config::Program, runtime::service};
use analyte_store::FreeSpace;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    task::JoinHandle,
};

pub(super) const SECRET: &str = "whsec_AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
pub(super) const ORU: &[u8] = b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012a|SYN^Synthetic^synthetic|||||||||||||||||||||F\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";
pub(super) const ASTM: &[u8] = b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|00012A\rR|1|^^^SYN|12.30|mg/L||N||F\rL|1|N\r";

pub(super) struct Space;
impl FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

pub(super) struct Fixture {
    path: PathBuf,
    listeners: Vec<(String, std::net::TcpListener)>,
}

impl AsRef<Path> for Fixture {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl std::ops::Deref for Fixture {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

pub(super) fn config(root: &Path, astm: bool) -> Result<Fixture, Error> {
    let mut listeners = Vec::new();
    let mut machines = String::new();
    for (name, is_astm) in [("hl7", false), ("astm", true)] {
        if is_astm && !astm {
            continue;
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        listeners.push((name.to_owned(), listener));
        let definition = if is_astm {
            include_str!("../../../core/config/examples/astm.toml").replace("kind = \"serial\"", "kind = \"tcp_server\"").replace("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "").replace("transport = \"serial\"", "transport = \"tcp_server\"")
        } else {
            include_str!("../../../core/config/examples/hl7.toml").to_owned()
        };
        std::fs::write(root.join(format!("{name}.toml")), definition)?;
        std::fs::write(
            root.join(format!("{name}-site.toml")),
            include_str!("../../../core/config/examples/site.toml")
                .replace("127.0.0.1:5600", &address.to_string()),
        )?;
        machines.push_str(&format!(
            "[[machines]]\nid = '{name}'\ndefinition = '{name}.toml'\nsite = '{name}-site.toml'\n"
        ));
    }
    let path = root.join("program.toml");
    std::fs::write(
        &path,
        format!(
            "schema_version = 1\ndata_directory = 'data'\n{machines}[store]\nreserve_bytes = 1024\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 100\nbusy_timeout_ms = 1000\n"
        ),
    )?;
    Ok(Fixture { path, listeners })
}

pub(super) fn program(path: &Fixture, url: &str) -> Result<Program, Error> {
    let mut program = Program::load(path)?;
    for (id, listener) in &path.listeners {
        program.set_loopback_listener_for_test(id, listener.try_clone()?)?;
    }
    program.set_loopback_destination_for_test("demo", url.into())?;
    Ok(program)
}

pub(super) fn secrets(
    program: &Program,
    api: &str,
) -> Result<analyte::installation::secrets::Environment, Error> {
    Ok(analyte::installation::secrets::Environment::load_with(
        program,
        |name| Some(if name.ends_with("API") { api } else { SECRET }.into()),
    )?)
}

pub(super) struct Running {
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<(), Error>>,
}

impl Running {
    pub(super) async fn start(path: &Fixture, url: &str, api: &str) -> Result<Self, Error> {
        let program = program(path, url)?;
        Self::start_program(program, api).await
    }

    pub(super) async fn start_program(program: Program, api: &str) -> Result<Self, Error> {
        let secrets = secrets(&program, api)?;
        let (stop, stopped) = oneshot::channel();
        let (ready, started) = oneshot::channel();
        let mut task = tokio::spawn(service::serve_loopback_test(
            program,
            Box::new(Space),
            async {
                stopped.await?;
                Ok(())
            },
            secrets,
            Some(ready),
        ));
        let startup = tokio::time::timeout(HANG_GUARD, async {
            tokio::select! { biased;
                result = &mut task => match result? {
                    Err(error) => Err(error),
                    Ok(()) => Err("test service ended before readiness".into()),
                },
                result = started => match result {
                    Ok(()) => Ok(()),
                    Err(_) => match (&mut task).await? {
                        Err(error) => Err(error),
                        Ok(()) => Err("test service ended without readiness".into()),
                    },
                },
            }
        })
        .await;
        match startup {
            Ok(result) => result?,
            Err(error) => {
                task.abort();
                let _ = task.await;
                return Err(error.into());
            }
        }
        Ok(Self { stop, task })
    }

    pub(super) async fn stop(self) -> Result<(), Error> {
        self.stop.send(()).map_err(|()| "service stopped early")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        Ok(())
    }
}

pub(super) async fn connect(path: &Path, name: &str) -> Result<TcpStream, Error> {
    let program = Program::load(path)?;
    let address = &program
        .machines()
        .iter()
        .find(|m| m.id().as_str() == name)
        .ok_or("machine absent")?
        .snapshot()
        .effective()
        .site()
        .endpoint;
    let stream = tokio::time::timeout(HANG_GUARD, async {
        loop {
            if let Ok(stream) = TcpStream::connect(address).await {
                return Ok::<_, Error>(stream);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    // Listeners are bound before the service starts, so an accepted connection
    // does not prove the store exists; wait until the latest schema is present.
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            // The file appears before migration completes; wait for the version.
            let ready = database(path).is_ok_and(|db| {
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .is_ok_and(|version| version == analyte_store::SCHEMA_VERSION)
            });
            if ready {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(stream)
}

pub(super) async fn hl7(peer: &mut TcpStream, message: &[u8]) -> Result<String, Error> {
    let wire = analyte_framing::hl7::encode(message, analyte_framing::hl7::Limit::new(1048576)?)?;
    peer.write_all(&wire).await?;
    tokio::time::timeout(HANG_GUARD, async {
        let mut ack = Vec::new();
        while !ack.ends_with(b"\x1c\r") {
            if ack.len() >= 65536 {
                return Err("oversized ACK".into());
            }
            ack.push(peer.read_u8().await?);
        }
        Ok::<_, Error>(String::from_utf8(ack)?)
    })
    .await?
}

pub(super) async fn astm(peer: &mut TcpStream, message: &[u8]) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        peer.write_all(b"\x05").await?;
        assert_eq!(peer.read_u8().await?, 6);
        let mut number = analyte_framing::astm::FrameNumber::FIRST;
        for record in message.split_inclusive(|b| *b == b'\r') {
            for frame in analyte_framing::astm::encode(
                record,
                number,
                analyte_framing::astm::Limit::new(64000)?,
            )? {
                number = number.next();
                peer.write_all(&frame).await?;
                assert_eq!(peer.read_u8().await?, 6);
            }
        }
        peer.write_all(b"\x04").await?;
        Ok::<_, Error>(())
    })
    .await?
}

pub(super) fn database(path: &Path) -> Result<rusqlite::Connection, Error> {
    // Never create the file: only the service may create and migrate its store.
    let connection = rusqlite::Connection::open_with_flags(
        path.parent()
            .ok_or("parent missing")?
            .join("data/analyte.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_secs(1))?;
    Ok(connection)
}

pub(super) async fn wait_state(path: &Path, state: &str, count: i64) -> Result<(), Error> {
    wait_database_count(
        path,
        "SELECT count(*) FROM outbox WHERE state=?1",
        [state],
        count,
    )
    .await
}

pub(crate) async fn wait_database_count(
    path: &Path,
    query: &'static str,
    params: impl rusqlite::Params + Clone,
    count: i64,
) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let found: i64 = database(path)?.query_row(query, params.clone(), |r| r.get(0))?;
            if found == count {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

pub(super) fn vendor_config(root: &Path, name: &str, definition: &str) -> Result<Fixture, Error> {
    let mut fixture = config(root, false)?;
    fixture.listeners.first_mut().ok_or("listener missing")?.0 = name.into();
    let text =
        std::fs::read_to_string(&fixture.path)?.replace("id = 'hl7'", &format!("id = '{name}'"));
    std::fs::write(&fixture.path, text)?;
    std::fs::write(root.join("hl7.toml"), definition)?;
    Ok(fixture)
}

impl Running {
    pub(super) async fn crash(self) -> Result<(), Error> {
        self.task.abort();
        assert!(self.task.await.is_err_and(|error| error.is_cancelled()));
        Ok(())
    }
}

pub(super) async fn wait_feed(path: &Path) -> Result<(), Error> {
    wait_database_count(
        path,
        "SELECT count(*) FROM feed WHERE complete=1 AND trusted=1",
        [],
        1,
    )
    .await
}

#[tokio::test]
async fn startup_failure_reaches_caller_before_any_peer_or_cache_wait() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    // A file where the protected directory belongs forces a real startup I/O error.
    std::fs::write(temp.path().join("data"), b"synthetic blocked directory")?;
    let error = Running::start(&path, "http://127.0.0.1:1", "synthetic-api")
        .await
        .err()
        .ok_or("failed service startup was reported as ready")?;
    assert!(error.downcast_ref::<std::io::Error>().is_some(), "{error}");
    Ok(())
}

pub(crate) async fn read_mllp_packet(
    peer: &mut TcpStream,
    oversized: &'static str,
) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        assert_eq!(peer.read_u8().await?, 11);
        let mut bytes = Vec::new();
        loop {
            let byte = peer.read_u8().await?;
            if byte == 28 {
                assert_eq!(peer.read_u8().await?, 13);
                return Ok::<_, Error>(bytes);
            }
            if bytes.len() >= 1048576 {
                return Err(oversized.into());
            }
            bytes.push(byte);
        }
    })
    .await?
}
