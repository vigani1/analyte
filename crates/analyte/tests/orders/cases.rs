//! LIS02-A2 §§8,11,12 and ADR 0004, using synthetic loopback peers only.
use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn configure_orders(root: &Path, send: bool) -> Result<(), Error> {
    let path = root.join("astm.toml");
    let text = std::fs::read_to_string(&path)?;
    let section = include_str!("../../../config/examples/orders.toml")
        .replace("send_ahead = true", &format!("send_ahead = {send}"))
        .replace(
            "namespace=\"demo\",value=\"DEMO\"",
            "namespace=\"synthetic\",value=\"TEST\"",
        );
    std::fs::write(path, format!("{text}\n{section}"))?;
    Ok(())
}

async fn query(peer: &mut TcpStream, sample: &str) -> Result<Vec<u8>, Error> {
    astm(
        peer,
        format!("H|\\^&|||SYN|||||||P|1\rQ|1|^{sample}||ALL||||||||O\rL|1|N\r").as_bytes(),
    )
    .await?;
    receive_orders(peer).await
}

async fn receive_orders(peer: &mut TcpStream) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        assert_eq!(peer.read_u8().await?, 5);
        peer.write_all(b"\x06").await?;
        let mut payload = Vec::new();
        loop {
            let byte = peer.read_u8().await?;
            if byte == 4 {
                return Ok::<_, Error>(payload);
            }
            assert_eq!(byte, 2);
            let mut frame = vec![byte];
            while !frame.ends_with(b"\r\n") {
                if frame.len() > 64000 {
                    return Err("oversized frame".into());
                }
                frame.push(peer.read_u8().await?);
            }
            let end = frame
                .iter()
                .position(|b| matches!(b, 3 | 23))
                .ok_or("missing frame ending")?;
            payload.extend_from_slice(frame.get(2..end).ok_or("invalid frame")?);
            if payload.len() > 1_048_576 {
                return Err("oversized reply".into());
            }
            peer.write_all(b"\x06").await?;
        }
    })
    .await?
}

async fn wait_cache(path: &Path) -> Result<(), Error> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if database(path)?
                .query_row("SELECT count(*) FROM orders", [], |r| r.get::<_, i64>(0))?
                > 0
            {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

#[tokio::test]
async fn lis02_section11_fresh_cache_orders_empty_and_untrusted_are_distinct_on_wire()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), true)?;
    configure_orders(temp.path(), false)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(include_bytes!(
            "../../../../docs/contract/vectors/order.json"
        ))?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api")?;
    let mut peer = connect(&path, "astm").await?;
    wait_cache(&path).await?;
    let reply = query(&mut peer, "000Ab12").await?;
    assert!(
        reply
            .windows(b"O|1|000Ab12||^^^SEND|R".len())
            .any(|w| w == b"O|1|000Ab12||^^^SEND|R")
    );
    assert!(reply.ends_with(b"L|1|N\r"));
    assert!(query(&mut peer, "absent").await?.ends_with(b"L|1|I\r"));
    database(&path)?.execute("UPDATE feed SET trusted=0", [])?;
    assert!(query(&mut peer, "absent").await?.ends_with(b"L|1|Q\r"));
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
