//! Tests machine receipt storage and acknowledgment ordering.
use analyte::{Error, installation::config::Program, runtime::service};
use analyte_fixtures as guard;
use analyte_store::{FreeSpace, Store};
use guard::HANG_GUARD;
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
};

use analyte_fixtures::Space;
mod connections;
mod custody;
mod hl7_safety;

fn config(temp: &Path, astm: bool) -> Result<std::path::PathBuf, Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let definition = if astm {
        analyte_fixtures::astm_tcp()
    } else {
        include_str!("../../../core/config/examples/hl7.toml").to_owned()
    };
    let path = analyte_fixtures::write_machine(temp, &definition, &address.to_string())?;
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
        {
            let mut limits = analyte_fixtures::store_limits();
            limits.record_bytes = 2097152;
            limits.busy_timeout = Duration::from_millis(20);
            limits
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

const ORU: &[u8] = b"MSH|^~\\&|SYN|LAB|HOST|LAB|20260101000000||ORU^R01^ORU_R01|message1|P|2.5.1\rPID|1\rOBR|1||00012|SYN^Synthetic^synthetic|||||||||||||||||||||F\rOBX|1|NM|SYN^Synthetic^synthetic||12.30|mg/L|||||F\r";

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
                analyte_fixtures::SECRET
            }
            .into(),
        )
    })?;
    service::serve_loopback_test(program, space, stop, secrets, None).await
}
