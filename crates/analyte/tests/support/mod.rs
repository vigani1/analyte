//! Tests bounded synthetic peers and local test fixtures.
use analyte_fixtures::ConfigEdit as _;
pub mod deadline;
pub mod peer;
use analyte_fixtures as guard;
pub use deadline::step;
pub use guard::HANG_GUARD;

use analyte::{Error, installation::config::Program, runtime::service};
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

/// Synthetic wire data or credentials used only by local test peers.
pub use analyte_fixtures::SECRET;
/// Synthetic wire data or credentials used only by local test peers.
pub const ORU: &[u8] = b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012a|SYN^Synthetic^synthetic|||||||||||||||||||||F\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";
/// Synthetic wire data or credentials used only by local test peers.
pub const ASTM: &[u8] = b"H|\\^&||||||||||P|LIS2-A2|20261115120000\rP|1\rO|1|00012A\rR|1|^^^SYN|12.30|mg/L||N||F\rL|1|N\r";

pub use analyte_fixtures::UnlimitedSpace as Space;

/// Owns resources for a synthetic service test.
pub struct Fixture {
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

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub fn config(root: &Path, astm: bool) -> Result<Fixture, Error> {
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
            analyte_fixtures::astm_tcp()
        } else {
            include_str!("../../../core/config/examples/hl7.toml").to_owned()
        };
        std::fs::write(root.join(format!("{name}.toml")), definition)?;
        std::fs::write(
            root.join(format!("{name}-site.toml")),
            include_str!("../../../core/config/examples/site.toml")
                .rewrite("127.0.0.1:5600", &address.to_string()),
        )?;
        machines.push_str(&format!(
            "[[machines]]\nid = '{name}'\ndefinition = '{name}.toml'\nsite = '{name}-site.toml'\n"
        ));
    }
    let path = root.join("program.toml");
    std::fs::write(&path, analyte_fixtures::program(&machines))?;
    Ok(Fixture { path, listeners })
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub fn program(path: &Fixture, url: &str) -> Result<Program, Error> {
    let mut program = Program::load(path)?;
    for (id, listener) in &path.listeners {
        program.set_loopback_listener_for_test(id, listener.try_clone()?)?;
    }
    program.set_loopback_destination_for_test("demo", url.into())?;
    Ok(program)
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub fn secrets(
    program: &Program,
    api: &str,
) -> Result<analyte::installation::secrets::Environment, Error> {
    Ok(analyte::installation::secrets::Environment::load_with(
        program,
        |name| Some(if name.ends_with("API") { api } else { SECRET }.into()),
    )?)
}

/// Owns resources for a synthetic service test.
pub struct Running {
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<(), Error>>,
}

impl Running {
    /// Runs a synthetic peer or inspects its durable test outcome.
    /// # Errors
    /// Propagates configuration, transport, storage or deadline failures.
    /// # Panics
    /// Fails when a recorded byte or durable outcome differs from the expected fixture.
    pub async fn start(path: &Fixture, url: &str, api: &str) -> Result<Self, Error> {
        let program = program(path, url)?;
        Self::start_program(program, api).await
    }

    /// Runs a synthetic peer or inspects its durable test outcome.
    /// # Errors
    /// Propagates configuration, transport, storage or deadline failures.
    /// # Panics
    /// Fails when a recorded byte or durable outcome differs from the expected fixture.
    pub async fn start_program(program: Program, api: &str) -> Result<Self, Error> {
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

    /// Runs a synthetic peer or inspects its durable test outcome.
    /// # Errors
    /// Propagates configuration, transport, storage or deadline failures.
    /// # Panics
    /// Fails when a recorded byte or durable outcome differs from the expected fixture.
    pub async fn stop(self) -> Result<(), Error> {
        self.stop.send(()).map_err(|()| "service stopped early")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        Ok(())
    }
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn connect(path: &Path, name: &str) -> Result<TcpStream, Error> {
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

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn hl7(peer: &mut TcpStream, message: &[u8]) -> Result<String, Error> {
    let wire = analyte_framing::hl7::encode(message, analyte_framing::hl7::Limit::new(1048576)?)?;
    peer.write_all(&wire).await?;
    tokio::time::timeout(HANG_GUARD, async {
        let mut ack = Vec::new();
        while !ack.ends_with(b"\x1c\r") {
            if ack.len() >= 65536 {
                return Err("oversized ACK".into());
            }
            ack.push(peer::byte(peer).await?);
        }
        Ok::<_, Error>(String::from_utf8(ack)?)
    })
    .await?
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn astm(peer: &mut TcpStream, message: &[u8]) -> Result<(), Error> {
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

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub fn database(path: &Path) -> Result<rusqlite::Connection, Error> {
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

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn wait_state(path: &Path, state: &str, count: i64) -> Result<(), Error> {
    wait_database_count(
        path,
        "SELECT count(*) FROM outbox WHERE state=?1",
        [state],
        count,
    )
    .await
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn wait_database_count(
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

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub fn vendor_config(root: &Path, name: &str, definition: &str) -> Result<Fixture, Error> {
    let mut fixture = config(root, false)?;
    fixture.listeners.first_mut().ok_or("listener missing")?.0 = name.into();
    let text =
        std::fs::read_to_string(&fixture.path)?.rewrite("id = 'hl7'", &format!("id = '{name}'"));
    std::fs::write(&fixture.path, text)?;
    std::fs::write(root.join("hl7.toml"), definition)?;
    Ok(fixture)
}

impl Running {
    /// Runs a synthetic peer or inspects its durable test outcome.
    /// # Errors
    /// Propagates configuration, transport, storage or deadline failures.
    /// # Panics
    /// Fails when a recorded byte or durable outcome differs from the expected fixture.
    pub async fn crash(self) -> Result<(), Error> {
        self.task.abort();
        assert!(self.task.await.is_err_and(|error| error.is_cancelled()));
        Ok(())
    }
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn wait_feed(path: &Path) -> Result<(), Error> {
    wait_database_count(
        path,
        "SELECT count(*) FROM feed WHERE complete=1 AND trusted=1",
        [],
        1,
    )
    .await
}

/// Runs a synthetic peer or inspects its durable test outcome.
/// # Errors
/// Propagates configuration, transport, storage or deadline failures.
/// # Panics
/// Fails when a recorded byte or durable outcome differs from the expected fixture.
pub async fn read_mllp_packet(
    peer: &mut TcpStream,
    oversized: &'static str,
) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        assert_eq!(peer::byte(peer).await?, 11);
        let mut bytes = Vec::new();
        loop {
            let byte = peer::byte(peer).await?;
            if byte == 28 {
                assert_eq!(peer::byte(peer).await?, 13);
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
