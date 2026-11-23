//! Tests record-only connections end to end with synthetic analyzer peers (ADR 0034).
//!
//! Each test runs the real service with a record-only machine on a loopback
//! listener in a temporary folder: no real port, no lab app, no real data.
//! It checks what the standard asks for on the wire, that every byte is in
//! the store before each reply, that the capture replays through the
//! recording reader, and that nothing is read, parked, delivered or ordered.
type Error = Box<dyn std::error::Error + Send + Sync>;
use analyte::{installation::config::Program, runtime::service};
use analyte_fixtures::{AdjustableSpace, HANG_GUARD};
use analyte_transport::recording::{self, Direction, Evidence};
use std::sync::{Arc, atomic::AtomicU64};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    task::JoinHandle,
};

mod conversations;
mod packing;

/// The model file of a synthetic record-only machine on a loopback listener.
fn record_file(protocol: &str) -> String {
    format!(
        "recording_schema_version = 1\nprotocol = \"{protocol}\"\n[transport]\nkind = \"tcp_server\"\n"
    )
}

/// One running record-only service and the analyzer peer connected to it.
struct Recorder {
    directory: tempfile::TempDir,
    path: std::path::PathBuf,
    _listener: std::net::TcpListener,
    peer: TcpStream,
    space: Arc<AtomicU64>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<(), analyte::runtime::Error>>,
    /// Everything the peer sent, in order.
    sent: Vec<u8>,
    /// Everything the peer read, in order.
    read: Vec<u8>,
}

impl Recorder {
    /// Starts a record-only machine speaking `protocol`, with `answers` as its
    /// site file's `[recording]` lines.
    async fn start(protocol: &str, answers: &str) -> Result<Self, Error> {
        Self::start_model("test/recorder", &record_file(protocol), answers).await
    }

    /// Starts a record-only machine from the model file `record` placed in
    /// the model folder `model` (`vendor/model`), with `answers` as its site
    /// file's `[recording]` lines.
    async fn start_model(model: &str, record: &str, answers: &str) -> Result<Self, Error> {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_ansi(false)
            .with_max_level(tracing_subscriber::filter::LevelFilter::WARN)
            .try_init();
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        std::fs::create_dir_all(root.join(model))?;
        std::fs::write(root.join(model).join("record.toml"), record)?;
        std::fs::write(
            root.join("site.toml"),
            format!(
                "retention = \"keep\"\nschema_version = 1\nendpoint = \"{address}\"\n[recording]\n{answers}"
            ),
        )?;
        let path = root.join("program.toml");
        std::fs::write(
            &path,
            analyte_fixtures::program(&format!(
                "[[machines]]\nid = 'recorder'\ndefinition = '{model}/record.toml'\nsite = 'site.toml'\n"
            )),
        )?;
        let mut program = Program::load(&path)?;
        program.set_loopback_listener_for_test("recorder", listener.try_clone()?)?;
        program.set_shutdown_io_budget_for_test(analyte_fixtures::STORE_BUDGET)?;
        // No lab app: there is no key to find and none is asked for.
        let secrets = analyte::installation::secrets::Environment::load_with(&program, |_| None)?;
        let space = Arc::new(AtomicU64::new(u64::MAX));
        let (stop, stopped) = oneshot::channel();
        let (ready, started) = oneshot::channel();
        let task = tokio::spawn(service::serve_loopback_test(
            program,
            Box::new(AdjustableSpace(space.clone())),
            async {
                stopped.await?;
                Ok(())
            },
            secrets,
            Some(ready),
        ));
        tokio::time::timeout(HANG_GUARD, started).await??;
        let peer = TcpStream::connect(address).await?;
        Ok(Self {
            directory,
            path,
            _listener: listener,
            peer,
            space,
            stop,
            task,
            sent: Vec::new(),
            read: Vec::new(),
        })
    }

    async fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.peer.write_all(bytes).await?;
        self.sent.extend_from_slice(bytes);
        Ok(())
    }

    async fn byte(&mut self) -> Result<u8, Error> {
        let byte = tokio::time::timeout(HANG_GUARD, self.peer.read_u8()).await??;
        self.read.push(byte);
        Ok(byte)
    }

    /// Reads until `end` closes what was read; bounded.
    async fn until(&mut self, end: &[u8]) -> Result<Vec<u8>, Error> {
        let mut read = Vec::new();
        while !read.ends_with(end) {
            if read.len() >= 65536 {
                return Err("oversized host reply".into());
            }
            read.push(self.byte().await?);
        }
        Ok(read)
    }

    /// Asserts that the host sends nothing for `wait`.
    async fn silent(&mut self, wait: Duration) -> Result<(), Error> {
        let mut byte = [0u8; 1];
        match tokio::time::timeout(wait, self.peer.read(&mut byte)).await {
            Err(_) => Ok(()),
            Ok(Ok(0)) => Err("the host closed instead of staying quiet".into()),
            Ok(Ok(_)) => {
                Err(format!("the host sent {:#04x} instead of staying quiet", byte[0]).into())
            }
            Ok(Err(error)) => Err(error.into()),
        }
    }

    fn database(&self) -> Result<rusqlite::Connection, Error> {
        let db = rusqlite::Connection::open_with_flags(
            self.directory.path().join("data/analyte.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_secs(1))?;
        Ok(db)
    }

    fn count(&self, sql: &str) -> Result<i64, Error> {
        Ok(self.database()?.query_row(sql, [], |row| row.get(0))?)
    }

    /// The bytes of every stored record-only receipt, in order.
    fn kept(&self) -> Result<Vec<u8>, Error> {
        let db = self.database()?;
        let mut statement = db.prepare("SELECT metadata, raw FROM receipts ORDER BY rowid")?;
        let mut kept = Vec::new();
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })? {
            let (metadata, raw) = row?;
            let metadata: analyte_model::ReceiptMetadata = serde_json::from_str(&metadata)?;
            assert_eq!(metadata.machine.as_str(), "recorder");
            assert_eq!(
                metadata.driver.namespace.as_str(),
                analyte_config::recording::RECORD_DRIVER
            );
            kept.extend(raw);
        }
        Ok(kept)
    }

    /// Every byte the peer sent is a stored record-only receipt, in order;
    /// the last bytes may still be on their way into the store.
    async fn kept_everything(&self) -> Result<(), Error> {
        tokio::time::timeout(HANG_GUARD, async {
            while self.kept()? != self.sent {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, Error>(())
        })
        .await
        .map_err(|_| "every received byte is kept, in order")??;
        Ok(())
    }

    /// Nothing was read, delivered or ordered, and only `open_parked`
    /// operator items are open (the Autolyser's uncertain R guard).
    fn nothing_else(&self, open_parked: i64) -> Result<(), Error> {
        for (table, sql) in [
            ("interpretations", "SELECT count(*) FROM interpretations"),
            ("outbox", "SELECT count(*) FROM outbox"),
            ("machine_orders", "SELECT count(*) FROM machine_orders"),
        ] {
            assert_eq!(self.count(sql)?, 0, "{table} must stay empty");
        }
        assert_eq!(
            self.count("SELECT count(*) FROM parked WHERE resolution IS NULL")?,
            open_parked,
            "recording never fills Needs review"
        );
        Ok(())
    }

    /// Stops the service without checking what it kept.
    async fn stop(self) -> Result<(), Error> {
        drop(self.peer);
        self.stop.send(()).map_err(|()| "service ended early")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        Ok(())
    }

    /// Stops the service and reads its capture through the ADR 0009 reader:
    /// it records exactly what crossed the wire.
    async fn finish(self, stack: [&str; 4], open_parked: i64) -> Result<Finished, Error> {
        self.kept_everything().await?;
        self.nothing_else(open_parked)?;
        drop(self.peer);
        self.stop.send(()).map_err(|()| "service ended early")?;
        tokio::time::timeout(HANG_GUARD, self.task).await???;
        let folder = self.directory.path().join("data/captures/recorder");
        let mut files = std::fs::read_dir(&folder)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        files.retain(|file| file.to_string_lossy().ends_with(".rec.jsonl"));
        assert_eq!(files.len(), 1, "one capture per connection");
        let file = files.first().ok_or("capture missing")?;
        let (header, events) =
            recording::read(&std::fs::read(file)?[..], recording::Limits::default())?;
        assert_eq!(header.evidence, Evidence::Machine);
        assert_eq!(header.de_identification, "NOT de-identified");
        assert_eq!(header.stack, stack);
        let bytes = |direction| {
            events
                .iter()
                .filter(|event| event.link.is_none() && event.direction == direction)
                .flat_map(|event| event.bytes.iter().copied())
                .collect::<Vec<_>>()
        };
        assert_eq!(bytes(Direction::MachineToHost), self.sent);
        assert_eq!(bytes(Direction::HostToMachine), self.read);
        // The audited command lists the recording and names its patient data.
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_analyte"))
            .arg("--config")
            .arg(&self.path)
            .args([
                "--actor",
                "it-test",
                "captures",
                "list",
                "--machine",
                "recorder",
            ])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let listed = String::from_utf8(output.stdout)?;
        assert!(listed.contains("contain patient data"), "{listed}");
        let name = file
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or("capture name")?;
        assert!(listed.contains(name), "{listed}");
        Ok(Finished {
            directory: self.directory,
            path: self.path,
        })
    }
}

/// What a finished conversation leaves: its folder and program file.
struct Finished {
    directory: tempfile::TempDir,
    path: std::path::PathBuf,
}
