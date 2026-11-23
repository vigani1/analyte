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
    let path = config(temp.path(), false)?;
    let definition = analyte_fixtures::definition_path(temp.path(), "hl7");
    let wire_settings = std::fs::read_to_string(folder.join("replay-orders.toml"))?;
    let configured = std::fs::read_to_string(&definition)?
        .rewrite("deviations = []", &wire_settings)
        .rewrite(r#"edition = "v251""#, r#"edition = "v231""#);
    std::fs::write(definition, configured)?;

    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let loaded = program(&path, &reference.url)?;
    let live = match cache {
        Cache::Seeded => {
            let order = include_str!("../../../../docs/contract/vectors/order.json");
            let next = order.rewrite("order-1", "order-2").rewrite("TEST", "NEXT");
            reference
                .receiver
                .seed(vec![
                    analyte_contract::decode(order.as_bytes())?,
                    analyte_contract::decode(next.as_bytes())?,
                ])
                .await?;
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
