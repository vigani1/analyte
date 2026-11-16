//! Rev18 chapter 23 pp.175-179 and ADR 0004/0008, synthetic peers only.
use super::vendor_support::*;
use super::*;
use crate::support::HANG_GUARD;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn definition(clone: bool) -> String {
    format!(
        "{}\n[orders]\nsend_ahead=true\npatient_namespace='synthetic'\nclone={clone}\n{}",
        include_str!("../../../config/examples/autolyser.toml")
            .replace("download = false", "download = true")
            .replace("poll_ms = 1000", "poll_ms = 200"),
        r#"
tests=[{app={namespace="synthetic",value="TEST"},machine_components=["SYN "]}]
priorities=[{app="routine",machine="T"}]
specimen_types=[{app={namespace="synthetic",value="SERUM"},machine="S"}]
"#
    )
}
fn order() -> Result<serde_json::Value, Error> {
    Ok(serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order-context.json"
    ))?)
}
async fn setup(
    value: serde_json::Value,
    clone: bool,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    setup_with_definition(value, &definition(clone)).await
}
async fn setup_with_definition(
    value: serde_json::Value,
    definition: &str,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "autolyser", definition)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &value,
        )?)?])
        .await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let peer = connect(&path, "autolyser").await?;
    Ok((temp, path, reference, service, peer))
}
async fn empty_poll(peer: &mut TcpStream) -> Result<(), Error> {
    assert_eq!(byte(peer).await?, 2);
    peer.write_all(b"\x06").await?;
    assert_eq!(byte(peer).await?, b'R');
    assert_eq!(byte(peer).await?, 4);
    peer.write_all(b"\x15").await?;
    Ok(())
}
async fn insertion(peer: &mut TcpStream, path: &Path, clone: bool) -> Result<(), Error> {
    empty_poll(peer).await?;
    assert_eq!(byte(peer).await?, 2);
    state(path, "Sending").await?;
    no_event(path).await?;
    peer.write_all(b"\x06").await?;
    let expected = if clone {
        b"000PatA        TSY0301SYN 212\x04"
    } else {
        b"000PatA        TSN0301SYN 201\x04"
    };
    let mut bytes = vec![0; expected.len()];
    tokio::time::timeout(HANG_GUARD, peer.read_exact(&mut bytes)).await??;
    assert_eq!(bytes, expected);
    Ok(())
}
async fn only_polls(peer: &mut TcpStream) -> Result<(), Error> {
    // Deliberate quiet window: only empty polls may arrive.
    let until = tokio::time::Instant::now()
        .checked_add(Duration::from_millis(700))
        .ok_or("test deadline overflow")?;
    loop {
        match tokio::time::timeout_at(until, empty_poll(peer)).await {
            Err(_) => return Ok(()),
            Ok(result) => result?,
        }
    }
}
async fn wait_count(path: &Path, sql: &str, expected: i64) -> Result<(), Error> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let found: i64 = database(path)?.query_row(sql, [], |r| r.get(0))?;
            if found == expected {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?
}
async fn closed(peer: &mut TcpStream) -> Result<(), Error> {
    let mut bytes = [0];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut bytes)).await??,
        0
    );
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_confirmed_insertion_carries_explicit_clone_and_stays_sent_on_restart()
-> Result<(), Error> {
    for clone in [true, false] {
        let (temp, path, reference, service, mut peer) = setup(order()?, clone).await?;
        insertion(&mut peer, &path, clone).await?;
        peer.write_all(b"Y").await?;
        state(&path, "Sending").await?;
        no_event(&path).await?;
        peer.write_all(b"\x03").await?;
        event(
            &path,
            &temp.path().join("receiver.db"),
            "Sent",
            "Autolyser-insertion-confirmed-position-03",
        )
        .await?;
        // Clear the next poll's custody guard before shutting down.
        empty_poll(&mut peer).await?;
        wait_count(
            &path,
            "SELECT count(*) FROM poll_intents WHERE state IN ('pending','uncertain')",
            0,
        )
        .await?;
        service.stop().await?;
        drop(peer);
        let service = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "autolyser").await?;
        only_polls(&mut peer).await?;
        event(
            &path,
            &temp.path().join("receiver.db"),
            "Sent",
            "Autolyser-insertion-confirmed-position-03",
        )
        .await?;
        drop(peer);
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_unanswered_initial_poll_never_reserves_or_sends_order()
-> Result<(), Error> {
    // Exercise the acknowledgment_ms deadline with the peer deliberately withholding input.
    let settings =
        definition(false).replace("acknowledgment_ms = 5000", "acknowledgment_ms = 1000");
    let (temp, path, reference, service, mut peer) =
        setup_with_definition(order()?, &settings).await?;
    assert_eq!(byte(&mut peer).await?, 2);
    assert_eq!(
        database(&path)?.query_row(
            "SELECT count(*) FROM poll_intents WHERE state IN ('pending','uncertain')",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    closed(&mut peer).await?;
    wait_count(
        &path,
        "SELECT count(*) FROM poll_intents WHERE state='uncertain'",
        1,
    )
    .await?;
    let mut reconnect = connect(&path, "autolyser").await?;
    closed(&mut reconnect).await?;
    wait_count(&path, "SELECT count(*) FROM poll_intents", 1).await?;
    no_event(&path).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    assert_eq!(receiver_count(&temp.path().join("receiver.db"))?, 0);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_unanswered_insertion_is_uncertain_not_sent() -> Result<(), Error> {
    // Exercise the acknowledgment_ms deadline with the peer deliberately withholding input.
    let settings =
        definition(false).replace("acknowledgment_ms = 5000", "acknowledgment_ms = 1000");
    let (temp, path, reference, service, mut peer) =
        setup_with_definition(order()?, &settings).await?;
    empty_poll(&mut peer).await?;
    assert_eq!(byte(&mut peer).await?, 2);
    state(&path, "Sending").await?;
    no_event(&path).await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "Autolyser-transfer-timeout",
    )
    .await?;
    closed(&mut peer).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_parked_partial_reply_is_uncertain_same_round_with_timeout_code()
-> Result<(), Error> {
    // Exercise the interbyte_ms deadline with the peer deliberately withholding input.
    let settings = definition(false).replace("interbyte_ms = 5000", "interbyte_ms = 1000");
    let (temp, path, reference, service, mut peer) =
        setup_with_definition(order()?, &settings).await?;
    insertion(&mut peer, &path, false).await?;
    peer.write_all(b"Y").await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "Autolyser-transfer-timeout-incomplete",
    )
    .await?;
    closed(&mut peer).await?;
    // No restart is involved in either the transition or its app delivery.
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_parked_bad_ack_is_uncertain_same_round() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?, false).await?;
    empty_poll(&mut peer).await?;
    assert_eq!(byte(&mut peer).await?, 2);
    state(&path, "Sending").await?;
    no_event(&path).await?;
    peer.write_all(b"\x15").await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "Autolyser-parked-ack",
    )
    .await?;
    closed(&mut peer).await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_disconnect_mid_send_retains_partial_reply_and_uncertain_code()
-> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?, false).await?;
    insertion(&mut peer, &path, false).await?;
    peer.write_all(b"Y").await?;
    // Wait for journal custody so the partial response precedes disconnect.
    wait_count(&path, "SELECT count(*) FROM receipts WHERE raw=x'59'", 1).await?;
    drop(peer);
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "Autolyser-transfer-disconnected-incomplete",
    )
    .await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn adr0004_rule8_autolyser_restart_during_send_recovers_only_sending_never_replays()
-> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?, false).await?;
    insertion(&mut peer, &path, false).await?;
    service.crash().await?;
    drop(peer);
    state(&path, "Sending").await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "autolyser").await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "host-restart",
    )
    .await?;
    only_polls(&mut peer).await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "host-restart",
    )
    .await?;
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
async fn rejection(value: serde_json::Value, code: &str) -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(value, false).await?;
    empty_poll(&mut peer).await?;
    event(&path, &temp.path().join("receiver.db"), "Rejected", code).await?;
    only_polls(&mut peer).await?;
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_unmapped_test_rejects_whole_order_before_insertion()
-> Result<(), Error> {
    let mut value = order()?;
    value["tests"]
        .as_array_mut()
        .ok_or("tests")?
        .push(serde_json::json!({"namespace":"synthetic","value":"UNKNOWN"}));
    rejection(value,"rejected: unmapped test NamespacedId { namespace: Identifier(\"synthetic\"), value: Identifier(\"UNKNOWN\") }").await
}
#[tokio::test]
async fn autolyser_rev18_ch23_unmapped_specimen_type_rejects_before_insertion() -> Result<(), Error>
{
    let mut value = order()?;
    value["specimen_type"]["namespace"] = "other".into();
    rejection(value, "rejected: unmapped-specimen-type").await
}
#[tokio::test]
async fn autolyser_rev18_ch23_missing_required_specimen_type_has_no_default() -> Result<(), Error> {
    let mut value = order()?;
    value
        .as_object_mut()
        .ok_or("order")?
        .remove("specimen_type");
    rejection(value, "rejected: missing-specimen-type").await
}
#[tokio::test]
async fn autolyser_rev18_ch23_missing_cup_position_never_autoassigns_zero() -> Result<(), Error> {
    let mut value = order()?;
    value.as_object_mut().ok_or("order")?.remove("position");
    rejection(value, "rejected: missing-cup-position").await
}
#[tokio::test]
async fn autolyser_rev18_ch23_zero_or_invalid_cup_position_is_rejected() -> Result<(), Error> {
    for slot in ["00", "100", "A1"] {
        let mut value = order()?;
        value["position"]["slot"] = slot.into();
        rejection(value, "rejected: invalid-cup-position").await?;
    }
    Ok(())
}
#[tokio::test]
async fn adr0004_autolyser_wrong_patient_namespace_rejects_before_insertion() -> Result<(), Error> {
    let mut value = order()?;
    value["patient"]["id"]["namespace"] = "other".into();
    rejection(value, "rejected: patient-namespace-mismatch").await
}
#[tokio::test]
async fn autolyser_rev18_ch23_missing_patient_never_uses_sample_id() -> Result<(), Error> {
    let mut value = order()?;
    value.as_object_mut().ok_or("order")?.remove("patient");
    rejection(value, "rejected: missing-patient").await
}
#[tokio::test]
async fn autolyser_rev18_ch23_overlong_patient_id_never_truncated() -> Result<(), Error> {
    let mut value = order()?;
    value["patient"]["id"]["value"] = "1234567890123456".into();
    rejection(value, "rejected: overlong-patient-id").await
}
#[tokio::test]
async fn adr0004_autolyser_cancelled_before_send_has_no_insertion_or_send_event()
-> Result<(), Error> {
    let mut value = order()?;
    value["state"] = "cancel_requested".into();
    let (temp, path, reference, service, mut peer) = setup(value, false).await?;
    // The cancelled order must be in the cache before "never sent" means anything;
    // wait for the poll to commit it instead of racing the first poll.
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let cached = database(&path)?
                .query_row("SELECT count(*) FROM orders", [], |r| r.get::<_, i64>(0))?;
            if cached == 1 {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    // A trusted pass must exist, or "never sent" would hold without the cancel rule.
    wait_feed(&path).await?;
    only_polls(&mut peer).await?;
    no_event(&path).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    assert_eq!(receiver_count(&temp.path().join("receiver.db"))?, 0);
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn adr0004_autolyser_missing_patient_namespace_prevents_program_start() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(
        temp.path(),
        "autolyser",
        &definition(false).replace("patient_namespace='synthetic'", ""),
    )?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &order()?,
        )?)?])
        .await?;
    let Err(error) = Running::start(&path, &reference.url, "synthetic-api").await else {
        return Err("missing patient namespace admitted".into());
    };
    assert!(error.to_string().contains("orders.patient_namespace"));
    assert_eq!(receiver_count(&receiver_path)?, 0);
    reference.close().await?;
    Ok(())
}
#[tokio::test]
async fn autolyser_rev18_ch23_negative_or_unknown_insertion_reply_never_claims_sent()
-> Result<(), Error> {
    for (reply, code) in [
        (b"N\x06", "Autolyser-insertion-rejected-Position"),
        (b"?\x03", "Autolyser-insertion-unknown-3F-03"),
    ] {
        let (temp, path, reference, service, mut peer) = setup(order()?, false).await?;
        insertion(&mut peer, &path, false).await?;
        peer.write_all(reply).await?;
        event(&path, &temp.path().join("receiver.db"), "Uncertain", code).await?;
        closed(&mut peer).await?;
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
