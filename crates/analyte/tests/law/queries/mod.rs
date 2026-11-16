//! Tests LAW order queries and query-triggered work downloads.
use super::*;
use crate::support::HANG_GUARD;
#[path = "../connections.rs"]
mod connections;
#[path = "../work.rs"]
mod work;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn definition() -> String {
    format!(
        "{}\n{}",
        include_str!("../../../../core/config/examples/law.toml")
            .replace(
                "operation_timeout_ms = 10000",
                "operation_timeout_ms = 60000"
            )
            .replace("response_ms = 1500", "response_ms = 30000")
            .replace("query_deadline_ms = 1500", "query_deadline_ms = 30000"),
        include_str!("../../../../core/config/examples/law_orders.toml")
    )
}
fn order(
    id: &str,
    sample: &str,
    target: Option<&str>,
    tests: usize,
) -> Result<analyte_contract::Order, Error> {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/contract/vectors/order.json"
    ))?;
    let fields = value.as_object_mut().ok_or("synthetic order object")?;
    fields.insert("id".into(), serde_json::json!(id));
    fields.insert("sample_id".into(), serde_json::json!(sample));
    fields.insert(
        "target_analyzers".into(),
        serde_json::json!(target.map(|value| vec![value])),
    );
    fields.insert(
        "tests".into(),
        serde_json::json!(
            (0..tests)
                .map(|_| serde_json::json!({"namespace":"synthetic","value":"TEST"}))
                .collect::<Vec<_>>()
        ),
    );
    fields.insert(
        "specimen_type".into(),
        serde_json::json!({"namespace":"types","value":"SERUM"}),
    );
    Ok(analyte_contract::decode(&serde_json::to_vec(&value)?)?)
}
async fn setup(
    orders: Vec<analyte_contract::Order>,
    settings: &str,
    paused: bool,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    setup_with_freshness(orders, settings, paused, 30).await
}
async fn setup_with_freshness(
    orders: Vec<analyte_contract::Order>,
    settings: &str,
    paused: bool,
    freshness: u64,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", settings)?;
    let site = temp.path().join("hl7-site.toml");
    std::fs::write(
        &site,
        format!(
            "{}\nfreshness_seconds = {freshness}\n",
            std::fs::read_to_string(&site)?
        ),
    )?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    for batch in orders.chunks(100) {
        reference.receiver.seed(batch.to_vec()).await?;
    }
    if paused {
        pause(&path, &reference.url).await?;
    }
    let mut program = program(&path, &reference.url)?;
    if settings.contains("send_ahead = true") {
        // Unsolicited lifecycle cases do not specify the app feed poll interval.
        program.set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))?;
    }
    let running = Running::start_program(program, "synthetic-api").await?;
    let peer = connect(&path, "law").await?;
    wait_feed(&path).await?;
    Ok((temp, path, reference, running, peer))
}
async fn pause(path: &Fixture, url: &str) -> Result<(), Error> {
    let loaded = program(path, url)?;
    let mut store = analyte::open_system_store(&loaded).await?;
    store
        .request(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or("deadline")?,
            |store| {
                store.audit(&analyte_store::Audit {
                    actor: analyte_model::Identifier::new("operator")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    action: analyte_model::Identifier::new("store-restore")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                    targets: Vec::new(),
                    time: jiff::Timestamp::now().as_second(),
                    reason: analyte_model::Reason::new("synthetic restore")
                        .map_err(|_| analyte_store::Error::InvalidInput)?,
                })
            },
        )
        .await?;
    store
        .shutdown(
            std::time::Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or("deadline")?,
        )
        .await?;
    Ok(())
}

fn query(qpd: &str, id: &str) -> String {
    format!(
        "MSH|^~\\&|SYN|LAB|HOST|LAB|20261115120000+0000||QBP^Q11^QBP_Q11|{id}|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-27^IHE\rQPD|{qpd}\rRCP|I||R^Real Time^HL70394\r"
    )
}
const CONTAINER: &str = "WOS^Work Order Step^IHELAW|tag|000Ab12^LAB";
const ALL: &str = "WOS_ALL^Work Order Step All^IHELAW|tag";
fn options() -> analyte_hl7::Options {
    analyte_hl7::Options {
        charset: analyte_hl7::Charset::Utf8,
        ..analyte_hl7::Options::default()
    }
}
async fn send(peer: &mut TcpStream, bytes: &[u8]) -> Result<(), Error> {
    peer.write_all(&analyte_framing::hl7::encode(
        bytes,
        analyte_framing::hl7::Limit::new(1048576)?,
    )?)
    .await?;
    Ok(())
}
async fn packet(peer: &mut TcpStream) -> Result<analyte_hl7::Message, Error> {
    let bytes = read_mllp_packet(peer, "LAW response bound").await?;
    Ok(analyte_hl7::law::parse_message(&bytes, options())?)
}
fn field<'a>(
    message: &'a analyte_hl7::Message,
    segment: &[u8],
    number: usize,
) -> Result<&'a [u8], Error> {
    Ok(message
        .segments()
        .find(|s| s.id() == segment)
        .and_then(|s| s.field(number))
        .ok_or("LAW field missing")?
        .raw())
}
async fn orl(
    peer: &mut TcpStream,
    sent: &analyte_hl7::Message,
    codes: &[&str],
    wrong: bool,
) -> Result<Vec<u8>, Error> {
    let id = if wrong {
        "wrong-id"
    } else {
        std::str::from_utf8(field(sent, b"MSH", 10)?)?
    };
    let mut body = format!(
        "MSH|^~\\&|SYN|LAB|HOST|LAB|20261115120000+0000||ORL^O34^ORL_O42|orl1|P|2.5.1||||||UNICODE UTF-8|||LAB-28^IHE\rMSA|AA|{id}\r"
    );
    let mut index = 0;
    let mut replies: Vec<String> = Vec::new();
    for segment in sent.segments() {
        match segment.id() {
            b"SPM" | b"SAC" if !codes.is_empty() => {
                if segment.id() == b"SPM" {
                    while let Some(reply) = replies.pop() {
                        body.push_str(&reply);
                    }
                }
                body.push_str(std::str::from_utf8(segment.raw())?);
                body.push('\r');
            }
            b"OBR" => {
                let awos = std::str::from_utf8(segment.field(2).ok_or("AWOS")?.raw())?;
                let code = codes.get(index).ok_or("ORL outcome missing")?;
                replies.push(format!(
                    "ORC|{code}|{awos}|||{}\r",
                    match *code {
                        "OK" => "SC",
                        "UC" => "IP",
                        _ => "CA",
                    }
                ));
                index = index.checked_add(1).ok_or("index")?;
            }
            _ => {}
        }
    }
    // Replies deliberately reverse AWOS order within each specimen group.
    while let Some(reply) = replies.pop() {
        body.push_str(&reply);
    }
    send(peer, body.as_bytes()).await?;
    Ok(analyte_framing::hl7::encode(
        body.as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?)
}
fn count(path: &Path, table: &str) -> Result<i64, Error> {
    Ok(database(path)?.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
}
async fn quiet(peer: &mut TcpStream) -> Result<(), Error> {
    // Deliberate quiet window: no reply may arrive.
    let outcome = tokio::time::timeout(Duration::from_millis(60), peer.read_u8()).await;
    let mut received = Vec::new();
    let mut collection = None;
    if let Ok(Ok(first)) = &outcome {
        received.push(*first);
        if *first == 11 {
            // The assertion already failed; retain the complete bounded synthetic
            // MLLP message so work replay can be distinguished from a late reply.
            collection = Some(
                tokio::time::timeout(HANG_GUARD, async {
                    while !received.ends_with(&[28, 13]) {
                        if received.len() >= 1_048_576 {
                            return Err::<(), Error>("oversized unexpected LAW frame".into());
                        }
                        received.push(peer.read_u8().await?);
                    }
                    Ok::<(), Error>(())
                })
                .await,
            );
        }
    }
    assert!(
        outcome.is_err(),
        "unexpected work or ACK loop: {outcome:?}; message: {:?}; collection: {collection:?}",
        String::from_utf8_lossy(&received)
    );
    Ok(())
}
/// Waits until `table` holds at least `expected` rows. Messages the host
/// deliberately leaves unanswered have no wire event that marks their storage.
async fn wait_count(path: &Path, table: &str, expected: i64) -> Result<i64, Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let found = count(path, table)?;
            if found >= expected {
                return Ok::<_, Error>(found);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}
async fn wait_audit(path: &Path) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let found: i64 = database(path)?.query_row(
                "SELECT count(*) FROM audit WHERE json_extract(evidence,'$.action')='LAW-negative-query-ORL'",
                [],
                |r| r.get(0),
            )?;
            if found == 1 {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

#[cfg(feature = "test-hooks")]
mod scenarios_1;

mod scenarios_2;
