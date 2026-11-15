//! Synthetic localhost fixtures; no environment mutation or external peers.
use analyte::{Error, config::Program, service};
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
pub(super) const ORU: &[u8] = b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01|message1|P|2.5.1\rPID|1\rOBR|1||00012a\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";
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
            include_str!("../../../config/examples/astm.toml").replace("kind = \"serial\"", "kind = \"tcp_server\"").replace("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "").replace("transport = \"serial\"", "transport = \"tcp_server\"")
        } else {
            include_str!("../../../config/examples/hl7.toml").to_owned()
        };
        std::fs::write(root.join(format!("{name}.toml")), definition)?;
        std::fs::write(
            root.join(format!("{name}-site.toml")),
            include_str!("../../../config/examples/site.toml")
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
) -> Result<analyte::secrets::EnvironmentSecrets, Error> {
    Ok(analyte::secrets::EnvironmentSecrets::load_with(
        program,
        |name| Some(if name.ends_with("API") { api } else { SECRET }.into()),
    )?)
}

pub(super) struct Running {
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<(), Error>>,
}

impl Running {
    pub(super) fn start(path: &Fixture, url: &str, api: &str) -> Result<Self, Error> {
        let program = program(path, url)?;
        let secrets = secrets(&program, api)?;
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(service::serve_loopback_test(
            program,
            Box::new(Space),
            async {
                stopped.await?;
                Ok(())
            },
            secrets,
        ));
        Ok(Self { stop, task })
    }

    pub(super) async fn stop(self) -> Result<(), Error> {
        self.stop.send(()).map_err(|()| "service stopped early")?;
        tokio::time::timeout(Duration::from_secs(7), self.task).await???;
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
    let stream = tokio::time::timeout(Duration::from_secs(5), async {
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
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let ready = database(path).is_ok_and(|db| {
                db.query_row("SELECT count(*) FROM machine_orders", [], |r| {
                    r.get::<_, i64>(0)
                })
                .is_ok()
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
    let wire = analyte_framing::mllp::encode(message, analyte_framing::mllp::Limit::new(1048576)?)?;
    peer.write_all(&wire).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
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
    tokio::time::timeout(Duration::from_secs(5), async {
        peer.write_all(b"\x05").await?;
        assert_eq!(peer.read_u8().await?, 6);
        for frame in analyte_framing::astm::encode(
            message,
            analyte_framing::astm::FrameNumber::FIRST,
            analyte_framing::astm::Limit::new(64000)?,
        )? {
            peer.write_all(&frame).await?;
            assert_eq!(peer.read_u8().await?, 6);
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
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let found: i64 = database(path)?.query_row(
                "SELECT count(*) FROM outbox WHERE state=?1",
                [state],
                |r| r.get(0),
            )?;
            if found == count {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}
