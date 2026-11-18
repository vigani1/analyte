//! Tests Autolyser polling, checksums and order exchanges.
use super::vendor::*;
use super::*;
use crate::support::{
    HANG_GUARD,
    peer::{closed, while_servicing},
};
use analyte_fixtures::ConfigEdit as _;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn definition(clone: bool) -> String {
    format!(
        "{}\n[orders]\nsend_ahead=true\npatient_namespace='synthetic'\nclone={clone}\n{}",
        include_str!("../../../core/config/examples/autolyser.toml")
            .rewrite("download = false", "download = true")
            .rewrite("poll_ms = 1000", "poll_ms = 200"),
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
    setup_with_gate(Some(value), definition, None).await
}
async fn setup_with_gate(
    value: Option<serde_json::Value>,
    definition: &str,
    hook: Option<Arc<analyte::runtime::test_hooks::Committed>>,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "autolyser", definition)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    if let Some(value) = value {
        reference
            .receiver
            .seed(vec![analyte_contract::decode(&serde_json::to_vec(
                &value,
            )?)?])
            .await?;
    }
    let mut configured = program(&path, &reference.url)?;
    if let Some(hook) = hook {
        configured.pause_autolyser_quiescent_for_test("autolyser", hook)?;
    }
    let service = Running::start_program(configured, "synthetic-api").await?;
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn confirmed_insertion_sends_clone_and_survives_restart() -> Result<(), Error> {
    for clone in [true, false] {
        let (hook, control) = analyte::runtime::test_hooks::Committed::new();
        let (temp, path, reference, service, mut peer) =
            setup_with_gate(Some(order()?), &definition(clone), Some(hook)).await?;
        empty_poll(&mut peer).await?;
        step("initial empty poll committed", control.reached()).await?;
        control.resume();
        // Reading the insertion start proves the initial pause has finished.
        assert_eq!(byte(&mut peer).await?, 2);
        control.arm_next();
        state(&path, "Sending").await?;
        no_event(&path).await?;
        peer.write_all(b"\x06").await?;
        let expected = if clone {
            b"000PatA        TSY0301SYN 212\x04"
        } else {
            b"000PatA        TSN0301SYN 201\x04"
        };
        let mut bytes = vec![0; expected.len()];
        step("explicit clone insertion", peer.read_exact(&mut bytes)).await?;
        assert_eq!(bytes, expected);
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
        // Hold the owner after confirmed insertion, with no next poll in flight.
        step(
            "confirmed insertion committed before shutdown",
            control.reached(),
        )
        .await?;
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
        step("restart polls", only_polls(&mut peer)).await?;
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn unanswered_poll_never_reserves_or_sends() -> Result<(), Error> {
    // Exercise the acknowledgment_ms deadline with the peer deliberately withholding input.
    let settings =
        definition(false).rewrite("acknowledgment_ms = 5000", "acknowledgment_ms = 1000");
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn unanswered_insertion_is_uncertain() -> Result<(), Error> {
    // Exercise the acknowledgment_ms deadline with the peer deliberately withholding input.
    let settings =
        definition(false).rewrite("acknowledgment_ms = 5000", "acknowledgment_ms = 1000");
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn partial_reply_is_parked_and_uncertain() -> Result<(), Error> {
    // Exercise the interbyte_ms deadline with the peer deliberately withholding input.
    let settings = definition(false).rewrite("interbyte_ms = 5000", "interbyte_ms = 1000");
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn bad_ack_is_parked_and_uncertain() -> Result<(), Error> {
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn disconnect_mid_send_keeps_partial_reply() -> Result<(), Error> {
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
// ADR 0004; Safety rule 8 (ADR 0003)
#[tokio::test]
async fn restart_recovers_only_sending_never_replays() -> Result<(), Error> {
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
    let (temp, path, reference, service, mut peer) =
        setup_with_gate(None, &definition(false), None).await?;
    empty_poll(&mut peer).await?;
    // Force the slow-feed ordering: work arrives only after the initial poll.
    // A peer that waits for rejection without servicing later polls deadlocks.
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &value,
        )?)?])
        .await?;
    let receiver_path = temp.path().join("receiver.db");
    while_servicing(event(&path, &receiver_path, "Rejected", code), async || {
        empty_poll(&mut peer).await
    })
    .await?;
    only_polls(&mut peer).await?;
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn unmapped_test_rejects_whole_order() -> Result<(), Error> {
    let mut value = order()?;
    value["tests"]
        .as_array_mut()
        .ok_or("tests")?
        .push(serde_json::json!({"namespace":"synthetic","value":"UNKNOWN"}));
    rejection(value,"rejected: unmapped test NamespacedId { namespace: Identifier(\"synthetic\"), value: Identifier(\"UNKNOWN\") }").await
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn unmapped_specimen_type_rejects_before_insertion() -> Result<(), Error> {
    let mut value = order()?;
    value["specimen_type"]["namespace"] = "other".into();
    rejection(value, "rejected: unmapped-specimen-type").await
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn missing_specimen_type_has_no_default() -> Result<(), Error> {
    let mut value = order()?;
    value
        .as_object_mut()
        .ok_or("order")?
        .remove("specimen_type");
    rejection(value, "rejected: missing-specimen-type").await
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn missing_cup_position_never_becomes_zero() -> Result<(), Error> {
    let mut value = order()?;
    value.as_object_mut().ok_or("order")?.remove("position");
    rejection(value, "rejected: missing-cup-position").await
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn zero_or_invalid_cup_position_is_rejected() -> Result<(), Error> {
    for slot in ["00", "100", "A1"] {
        let mut value = order()?;
        value["position"]["slot"] = slot.into();
        rejection(value, "rejected: invalid-cup-position").await?;
    }
    Ok(())
}
// ADR 0004
#[tokio::test]
async fn wrong_patient_namespace_rejects_before_insertion() -> Result<(), Error> {
    let mut value = order()?;
    value["patient"]["id"]["namespace"] = "other".into();
    rejection(value, "rejected: patient-namespace-mismatch").await
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn missing_patient_never_uses_sample_id() -> Result<(), Error> {
    let mut value = order()?;
    value.as_object_mut().ok_or("order")?.remove("patient");
    rejection(value, "rejected: missing-patient").await
}
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn overlong_patient_id_is_never_truncated() -> Result<(), Error> {
    let mut value = order()?;
    value["patient"]["id"]["value"] = "1234567890123456".into();
    rejection(value, "rejected: overlong-patient-id").await
}
// ADR 0004
#[tokio::test]
async fn cancelled_before_send_never_inserts() -> Result<(), Error> {
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
// ADR 0004
#[tokio::test]
async fn missing_patient_namespace_blocks_startup() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(
        temp.path(),
        "autolyser",
        &definition(false).rewrite("patient_namespace='synthetic'", ""),
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
// Autolyser manual Rev18 chapter 23
#[tokio::test]
async fn negative_or_unknown_reply_never_claims_sent() -> Result<(), Error> {
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
