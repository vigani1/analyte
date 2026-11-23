//! Replays the normal-mode HL7 record-display query with a synthetic order cache.
use super::support::{HANG_GUARD, config, program, read_mllp_packet};
use super::*;
use analyte_fixtures::ConfigEdit as _;
use analyte_transport::recording::{Direction, Event, Header};
use tokio::{io::AsyncWriteExt as _, net::TcpStream};

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Cache {
    Seeded,
    Unavailable,
}

// YHLO LIS V1.1 sections 2.3.6-2.3.11 and 2.4.2; ADR 0003 rules 8 and 9.
pub(super) async fn replay(
    folder: &Path,
    header: &Header,
    events: &[Event],
    cache: Cache,
) -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = display_config(temp.path(), folder)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let loaded = program(&path, &reference.url)?;
    let live = match cache {
        Cache::Seeded => {
            seed_orders(&reference).await?;
            Some(reference)
        }
        Cache::Unavailable => {
            reference.close().await?;
            None
        }
    };
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let mut peer = connect(&path, "hl7").await?;
    if matches!(cache, Cache::Seeded) {
        wait_count(
            &path,
            "SELECT count(*) FROM feed WHERE trusted=1 AND complete=1",
            1,
        )
        .await?;
    }
    conversation(&path, &mut peer, header, events).await?;
    match cache {
        Cache::Seeded => {
            wait_count(
                &path,
                "SELECT count(*) FROM machine_orders WHERE state='Sent'",
                2,
            )
            .await?;
            assert_eq!(count(&path, "SELECT count(*) FROM machine_orders")?, 2);
        }
        Cache::Unavailable => {
            assert_eq!(count(&path, "SELECT count(*) FROM machine_orders")?, 0);
            assert_quiet(&mut peer).await;
        }
    }
    running.stop().await?;
    if let Some(reference) = live {
        reference.close().await?;
    }
    Ok(())
}

fn display_config(root: &Path, folder: &Path) -> Result<Fixture, Error> {
    let path = config(root, false)?;
    let definition = analyte_fixtures::definition_path(root, "hl7");
    let wire_settings = std::fs::read_to_string(folder.join("replay-orders.toml"))?;
    let configured = std::fs::read_to_string(&definition)?
        .rewrite("deviations = []", &wire_settings)
        .rewrite(r#"edition = "v251""#, r#"edition = "v231""#);
    std::fs::write(definition, configured)?;

    Ok(path)
}

async fn seed_orders(reference: &Reference) -> Result<(), Error> {
    let order = include_str!("../../../../docs/contract/vectors/order.json");
    let next = order.rewrite("order-1", "order-2").rewrite("TEST", "NEXT");
    reference
        .receiver
        .seed(vec![
            analyte_contract::decode(order.as_bytes())?,
            analyte_contract::decode(next.as_bytes())?,
        ])
        .await?;
    Ok(())
}

// YHLO LIS V1.1 sections 2.3.9-2.3.11; AGENTS.md clinical rule 8.
#[tokio::test]
async fn record_display_stops_after_the_first_page_when_the_next_order_is_cancelled()
-> Result<(), Error> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machines/yhlo/iflash-1200");
    let (_, events) = analyte_transport::recording::read(
        std::fs::File::open(folder.join("recordings/barcode-orders.rec.jsonl"))?,
        Default::default(),
    )?;
    let [query, _, _, ack, ..] = events.as_slice() else {
        return Err("missing exchange for the first display page".into());
    };
    let temp = tempfile::tempdir()?;
    let path = display_config(temp.path(), &folder)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    seed_orders(&reference).await?;
    let mut loaded = program(&path, &reference.url)?;
    loaded.set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let result = async {
        wait_feed(&path).await?;
        let mut peer = connect(&path, "hl7").await?;
        peer.write_all(&query.bytes).await?;
        let qck = read_mllp_packet(&mut peer, "oversized QCK").await?;
        assert!(std::str::from_utf8(&qck)?.contains("QCK^Q02"));
        let first = read_mllp_packet(&mut peer, "oversized first DSR").await?;
        assert!(std::str::from_utf8(&first)?.contains("DSP|29||TEST"));
        let message = analyte_hl7::Message::parse(&first, Default::default())?;
        let msh = analyte_hl7::Msh::new(&message)?;
        assert_eq!(msh.message_type().next(), Some(b"DSR".as_slice()));

        let order = include_str!("../../../../docs/contract/vectors/order.json");
        let cancelled = order.rewrite("order-1", "order-2").rewrite("TEST", "NEXT")
            .rewrite("\"version\": 1", "\"version\": 2")
            .rewrite("\"requested\"", "\"cancel_requested\"");
        reference.receiver.seed(vec![analyte_contract::decode(cancelled.as_bytes())?]).await?;
        wait_count(
            &path,
            "SELECT count(*) FROM orders WHERE id='order-2' AND json_extract(json_extract(evidence, '$.document'), '$.state')='cancel_requested'",
            1,
        ).await?;

        let ack = std::str::from_utf8(&ack.bytes)?
            .rewrite("01234567890123456789", std::str::from_utf8(msh.control_id())?);
        peer.write_all(ack.as_bytes()).await?;
        let mut next = [0];
        let read = tokio::time::timeout(HANG_GUARD, peer.read(&mut next)).await?;
        match read {
            Ok(0) => {}
            Ok(_) => return Err(format!("a second display page was sent: {next:02x?}").into()),
            Err(error) if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
            Err(error) => return Err(error.into()),
        }
        assert_eq!(count(&path, "SELECT count(*) FROM machine_orders WHERE state='Sent' AND order_id='order-1'")?, 1);
        assert_eq!(count(&path, "SELECT count(*) FROM machine_orders WHERE state='Sent'")?, 1);
        Ok::<_, Error>(())
    }.await;
    running.stop().await?;
    reference.close().await?;
    result
}

async fn conversation(
    path: &Path,
    peer: &mut TcpStream,
    header: &Header,
    events: &[Event],
) -> Result<(), Error> {
    let mut last_dsr_id = None;
    let mut settled = 0_i64;
    for (index, step) in events.iter().enumerate() {
        assert!(step.link.is_none());
        match step.direction {
            Direction::MachineToHost => {
                let wire = std::str::from_utf8(&step.bytes)?;
                if let Some(id) = last_dsr_id.as_deref() {
                    assert_quiet(peer).await;
                    assert_eq!(
                        count(
                            path,
                            "SELECT count(*) FROM machine_orders WHERE state='Sent'"
                        )?,
                        settled
                    );
                    peer.write_all(wire.rewrite("01234567890123456789", id).as_bytes())
                        .await?;
                    settled = settled.saturating_add(1);
                } else {
                    peer.write_all(step.bytes.as_slice()).await?;
                }
            }
            Direction::HostToMachine => {
                let payload = read_mllp_packet(peer, "oversized record-display response").await?;
                let actual = analyte_framing::hl7::encode(
                    &payload,
                    analyte_framing::hl7::Limit::new(1048576)?,
                )?;
                assert_eq!(
                    actual.len(),
                    step.bytes.len(),
                    "event {index}: actual {:?}, expected {:?}",
                    String::from_utf8_lossy(&actual),
                    String::from_utf8_lossy(&step.bytes)
                );
                for (offset, (actual, expected)) in actual.iter().zip(&step.bytes).enumerate() {
                    if !header
                        .exceptions
                        .iter()
                        .any(|e| e.event == index && e.bytes.contains(&offset))
                    {
                        assert_eq!(actual, expected, "event {index}, byte {offset}");
                    }
                }
                assert!(count(path, "SELECT count(*) FROM receipts")? > settled);
                let message = analyte_hl7::Message::parse(&payload, Default::default())?;
                let msh = analyte_hl7::Msh::new(&message)?;
                assert!(msh.control_id().iter().all(u8::is_ascii_digit));
                if msh.message_type().next() == Some(b"DSR") {
                    let id = std::str::from_utf8(msh.control_id())?.to_owned();
                    assert_ne!(last_dsr_id.as_ref(), Some(&id));
                    last_dsr_id = Some(id);
                }
            }
        }
    }
    Ok(())
}

async fn assert_quiet(peer: &mut TcpStream) {
    assert!(
        tokio::time::timeout(
            Duration::from_millis(40),
            read_mllp_packet(peer, "unexpected DSR")
        )
        .await
        .is_err(),
        "a DSR was issued without a successful lookup or prior ACK"
    );
}

fn count(path: &Path, sql: &str) -> Result<i64, Error> {
    Ok(database(path)?.query_row(sql, [], |r| r.get(0))?)
}

async fn wait_count(path: &Path, sql: &str, expected: i64) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if count(path, sql)? == expected {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}
