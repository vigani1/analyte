//! Synthetic localhost receive-path tests, tied to ADR 0003 and protocol scopes.
#[path = "../support/guard.rs"]
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
        include_str!("../../../core/config/examples/astm.toml").replace("kind = \"serial\"", "kind = \"tcp_server\"").replace("[transport.serial]\nbaud = 9600\ndata_bits = 8\nparity = \"none\"\nstop_bits = 1\nflow_control = \"none\"\n", "").replace("transport = \"serial\"", "transport = \"tcp_server\"")
    } else {
        include_str!("../../../core/config/examples/hl7.toml").to_owned()
    };
    std::fs::write(temp.join("machine.toml"), definition)?;
    std::fs::write(
        temp.join("site.toml"),
        include_str!("../../../core/config/examples/site.toml")
            .replace("127.0.0.1:5600", &address.to_string()),
    )?;
    let path = temp.join("program.toml");
    std::fs::write(
        &path,
        "schema_version = 1\ndata_directory = 'data'\n[[machines]]\nid = 'synthetic'\ndefinition = 'machine.toml'\nsite = 'site.toml'\n[store]\nreserve_bytes = 1024\ndatabase_bytes = 100000000\nrecord_bytes = 2097152\npage_rows = 100\nbusy_timeout_ms = 1000\n",
    )?;
    Ok(path)
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

async fn serve(
    program: Program,
    space: Box<dyn FreeSpace>,
    stop: impl Future<Output = Result<(), Error>>,
) -> Result<(), Error> {
    let secrets = analyte::installation::secrets::Environment::load_with(&program, |name| {
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

mod scenarios_1;

mod scenarios_2;
