//! Synthetic program/reference-receiver evidence, TF-2b 3.28.4/C.8 and ADR 0003 rule 8.
use super::*;
use crate::support::HANG_GUARD;

async fn state(path: &Path, expected: &str) -> Result<(), Error> {
    if let Err(error) = vendor_support::state(path, expected).await {
        let current: String = database(path)?.query_row(
            "SELECT state || ':' || source_code FROM machine_orders LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        return Err(format!("expected {expected}, retained {current}: {error}").into());
    }
    Ok(())
}

fn send_ahead() -> String {
    definition().replace("send_ahead = false", "send_ahead = true")
}

#[tokio::test]
async fn law_tf2b_3_28_send_ahead_accepted_rejected_and_mixed_awos() -> Result<(), Error> {
    for (codes, expected) in [
        (["OK", "OK"], "Accepted"),
        (["UA", "UA"], "Rejected"),
        (["OK", "UA"], "Uncertain"),
    ] {
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", Some("law"), 2)?],
            &send_ahead(),
            false,
        )
        .await?;
        let sent = packet(&mut peer).await?;
        assert_eq!(field(&sent, b"MSH", 9)?, b"OML^O33^OML_O33");
        state(&path, "Sending").await?;
        assert_eq!(count(&path, "machine_order_items")?, 2);
        assert_eq!(count(&path, "outbox")?, 0);
        orl(&mut peer, &sent, &codes, false).await?;
        state(&path, expected).await?;
        wait_state(&path, "done", 1).await?;
        let states = database(&path)?
            .prepare("SELECT state FROM machine_order_items ORDER BY item_index")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for (state, code) in states.iter().zip(codes) {
            assert_eq!(
                state.as_str(),
                if code == "OK" { "Accepted" } else { "Rejected" }
            );
        }
        no_work(&mut peer).await?;
        running.stop().await?;
        drop(peer);
        let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "law").await?;
        no_work(&mut peer).await?;
        assert_eq!(count(&path, "machine_orders")?, 1);
        assert_eq!(count(&path, "machine_order_items")?, 2);
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

async fn no_work(peer: &mut TcpStream) -> Result<(), Error> {
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(350), peer.read_u8())
            .await
            .is_err(),
        "command must not be resent"
    );
    Ok(())
}
pub(super) async fn cancel_cache(path: &Path, reference: &Reference) -> Result<(), Error> {
    let evidence: String =
        database(path)?.query_row("SELECT evidence FROM orders WHERE id='order-1'", [], |r| {
            r.get(0)
        })?;
    let cached: analyte_store::CachedOrder = serde_json::from_str(&evidence)?;
    let mut document: serde_json::Value = serde_json::from_str(&cached.document)?;
    *document.get_mut("state").ok_or("state")? = "cancel_requested".into();
    *document.get_mut("version").ok_or("version")? = 2.into();
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &document,
        )?)?])
        .await?;
    wait_cancel_cache(path).await?;

    Ok(())
}
async fn wait_cancel_cache(path: &Path) -> Result<(), Error> {
    // The app feed polls every five seconds; wait for admission input, not a LAW deadline.
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if database(path)?.query_row("SELECT count(*) FROM orders WHERE id='order-1' AND json_extract(json_extract(evidence,'$.document'),'$.version')=2",[],|r|r.get::<_,i64>(0))? == 1 { return Ok::<_,Error>(()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;

    Ok(())
}
fn identities(sent: &analyte_hl7::Message) -> Result<Vec<Vec<u8>>, Error> {
    sent.segments()
        .filter(|s| s.id() == b"OBR")
        .map(|s| Ok(s.field(2).ok_or("AWOS")?.raw().to_vec()))
        .collect()
}
#[tokio::test]
async fn law_tf2b_3_28_c8_cancel_confirmed_refused_and_mixed_keep_each_awos() -> Result<(), Error> {
    for (codes, expected) in [
        (["CR", "CR"], "CancelConfirmed"),
        (["UC", "UC"], "Accepted"),
        (["CR", "UC"], "Accepted"),
    ] {
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", Some("law"), 2)?],
            &send_ahead(),
            false,
        )
        .await?;
        let sent = packet(&mut peer).await?;
        orl(&mut peer, &sent, &["OK", "OK"], false).await?;
        state(&path, "Accepted").await?;
        cancel_cache(&path, &reference).await?;
        let cancel = packet(&mut peer)
            .await
            .map_err(|e| format!("cancel packet for {codes:?}: {e}"))?;
        assert_eq!(identities(&sent)?, identities(&cancel)?);
        assert_eq!(cancel.segments().filter(|s| s.id() == b"ORC").count(), 2);
        assert!(
            cancel
                .segments()
                .filter(|s| s.id() == b"ORC")
                .all(|s| s.field(1).is_some_and(|v| v.raw() == b"CA"))
        );
        state(&path, "CancelRequested").await?;
        orl(&mut peer, &cancel, &codes, false).await?;
        state(&path, expected).await?;
        wait_state(&path, "done", 3).await?;
        let states = database(&path)?
            .prepare("SELECT state FROM machine_order_items ORDER BY item_index")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for (state, code) in states.iter().zip(codes) {
            assert_eq!(
                state.as_str(),
                if code == "CR" {
                    "CancelConfirmed"
                } else {
                    "Accepted"
                }
            );
        }
        assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.source_code') LIKE 'app-cancel:2:%'",[],|r|r.get::<_,i64>(0))?,1);
        no_work(&mut peer).await?;
        running.stop().await?;
        drop(peer);
        let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "law").await?;
        no_work(&mut peer).await?;
        assert_eq!(count(&path, "machine_orders")?, 1);
        assert_eq!(count(&path, "machine_order_items")?, 2);
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
#[tokio::test]
async fn law_tf2b_3_28_adr0003_no_reply_and_disconnect_are_uncertain_never_replayed()
-> Result<(), Error> {
    for cancelling in [false, true] {
        for disconnect in [false, true] {
            // Cancellation first needs a real acceptance round trip. Only the
            // no-reply-only case can use a short timer without racing that reply.
            let response_ms = if cancelling || disconnect { 10000 } else { 250 };
            let settings = send_ahead().replace(
                "response_ms = 30000",
                &format!("response_ms = {response_ms}"),
            );
            let (_temp, path, reference, running, mut peer) = setup(
                vec![order("order-1", "000Ab12", Some("law"), 2)?],
                &settings,
                false,
            )
            .await?;
            let sent = packet(&mut peer).await?;
            if cancelling {
                orl(&mut peer, &sent, &["OK", "OK"], false).await?;
                state(&path, "Accepted").await?;
                cancel_cache(&path, &reference).await?;
                packet(&mut peer).await?;
                state(&path, "CancelRequested").await?;
            }
            if disconnect {
                drop(peer);
            } else {
                // Keep the peer connected and withhold ORL until timeout is durable.
                // Dropping it after a fixed wait could instead exercise disconnect.
                state(&path, "Uncertain").await?;
                let source: String = database(&path)?.query_row(
                    "SELECT source_code FROM machine_orders",
                    [],
                    |row| row.get(0),
                )?;
                assert!(source.contains("LAW-ORL-timeout"), "{source}");
                drop(peer);
            }
            state(&path, "Uncertain").await?;
            running.stop().await?;
            let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
            let mut peer = connect(&path, "law").await?;
            no_work(&mut peer).await?;
            assert_eq!(count(&path, "machine_orders")?, 1);
            assert_eq!(count(&path, "machine_order_items")?, 2);
            restarted.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}
#[tokio::test]
async fn law_tf2b_3_28_adr0003_restart_during_sending_or_cancelling_is_uncertain_without_resend()
-> Result<(), Error> {
    for cancelling in [false, true] {
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", Some("law"), 2)?],
            &send_ahead(),
            false,
        )
        .await?;
        let sent = packet(&mut peer).await?;
        if cancelling {
            orl(&mut peer, &sent, &["OK", "OK"], false).await?;
            state(&path, "Accepted").await?;
            cancel_cache(&path, &reference).await?;
            packet(&mut peer).await?;
        }
        let before = if cancelling {
            "CancelRequested"
        } else {
            "Sending"
        };
        state(&path, before).await?;
        running.crash().await?;
        tokio::time::timeout(HANG_GUARD, async {
            let mut byte = [0];
            loop {
                match peer.read(&mut byte).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        })
        .await?;
        state(&path, before).await?;
        drop(peer);
        let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "law").await?;
        state(&path, "Uncertain").await?;
        no_work(&mut peer).await?;
        assert_eq!(
            database(&path)?.query_row(
                "SELECT count(*) FROM machine_order_items WHERE state='Uncertain'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            2
        );
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_28_adr0003_restore_pause_blocks_send_ahead_and_cancellation()
-> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 2)?],
        &send_ahead(),
        true,
    )
    .await?;
    no_work(&mut peer).await?;
    assert_eq!(count(&path, "machine_orders")?, 0);
    running.stop().await?;
    reference.close().await?;

    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 2)?],
        &send_ahead(),
        false,
    )
    .await?;
    let sent = packet(&mut peer).await?;
    orl(&mut peer, &sent, &["OK", "OK"], false).await?;
    state(&path, "Accepted").await?;
    running.stop().await?;
    drop(peer);
    pause(&path, &reference.url).await?;
    let app = order("order-1", "000Ab12", Some("law"), 2)?;
    let mut value: serde_json::Value = serde_json::to_value(&app)?;
    *value.get_mut("version").ok_or("version")? = 2.into();
    *value.get_mut("state").ok_or("state")? = "cancel_requested".into();
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &value,
        )?)?])
        .await?;
    let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    wait_feed(&path).await?;
    wait_cancel_cache(&path).await?;
    no_work(&mut peer).await?;
    state(&path, "Accepted").await?;
    assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='CancelRequested'",[],|r|r.get::<_,i64>(0))?,0);
    restarted.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_28_adr0003_unmapped_send_is_rejected_before_reservation_or_bytes()
-> Result<(), Error> {
    let mut value = serde_json::to_value(order("order-1", "000Ab12", Some("law"), 2)?)?;
    *value.get_mut("tests").ok_or("tests")? =
        serde_json::json!([{"namespace":"unknown","value":"unmapped"}]);
    let (_temp, path, reference, running, mut peer) = setup(
        vec![analyte_contract::decode(&serde_json::to_vec(&value)?)?],
        &send_ahead(),
        false,
    )
    .await?;
    state(&path, "Rejected").await?;
    wait_state(&path, "done", 1).await?;
    assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='Sending'",[],|r|r.get::<_,i64>(0))?,0);
    no_work(&mut peer).await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_27_3_28_queried_work_uses_the_same_cancel_exchange_without_send_ahead()
-> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 2)?],
        &definition(),
        false,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "query-cancel").as_bytes()).await?;
    packet(&mut peer).await?;
    let sent = packet(&mut peer).await?;
    orl(&mut peer, &sent, &["OK", "OK"], false).await?;
    state(&path, "Accepted").await?;
    cancel_cache(&path, &reference).await?;
    let cancel = packet(&mut peer).await?;
    assert_eq!(identities(&sent)?, identities(&cancel)?);
    orl(&mut peer, &cancel, &["CR", "CR"], false).await?;
    state(&path, "CancelConfirmed").await?;
    wait_state(&path, "done", 3).await?;
    no_work(&mut peer).await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}
