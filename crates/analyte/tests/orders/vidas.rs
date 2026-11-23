//! Tests VIDAS result uploads and work downloads.
use super::vendor::*;
use super::*;
use crate::support::HANG_GUARD;
use analyte_fixtures::ConfigEdit as _;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn definition() -> String {
    format!(
        "{}\n{}",
        include_str!("../../../core/config/examples/vidas_literal.toml")
            .rewrite("download = false", "download = true"),
        r#"
[orders]
send_ahead = true
patient_namespace = "synthetic"
priorities = [{app="routine",machine="R"}]
"#
    )
}
fn order() -> Result<serde_json::Value, Error> {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order-context.json"
    ))?;
    value
        .as_object_mut()
        .ok_or("order")?
        .remove("specimen_type");
    Ok(value)
}
async fn setup(
    value: serde_json::Value,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    setup_with_definition(value, &definition()).await
}

async fn setup_with_definition(
    value: serde_json::Value,
    definition: &str,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "vidas", definition)?;
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
    let peer = connect(&path, "vidas").await?;
    Ok((temp, path, reference, service, peer))
}
async fn packet(peer: &mut TcpStream) -> Result<(), Error> {
    peer.write_all(b"\x06").await?;
    read_packet(peer).await
}

async fn read_packet(peer: &mut TcpStream) -> Result<(), Error> {
    let expected = framing()?.encode(b"mtmpr|pi000PatA|si|ci000Ab12|rtTEST|")?;
    let mut received = vec![0; expected.len()];
    tokio::time::timeout(HANG_GUARD, peer.read_exact(&mut received)).await??;
    assert_eq!(received, expected);
    Ok(())
}
fn framing() -> Result<analyte_framing::vidas::Config, Error> {
    use analyte_framing::vidas::{Checksum, Config, Mode};
    Ok(Config::new(
        Mode::Literal,
        80,
        1920,
        &[],
        Checksum {
            include_stx: false,
            include_suffix: false,
            hex: analyte_framing::vidas::HexOptions {
                uppercase: true,
                accept_either_case: false,
            },
        },
    )?)
}
async fn confirm(peer: &mut TcpStream) -> Result<(), Error> {
    peer.write_all(b"\x06").await?;
    assert_eq!(byte(peer).await?, 3);
    assert_eq!(byte(peer).await?, 4);
    Ok(())
}
// VIDAS BCI guide C pp. 3-2 and 10-2
#[tokio::test]
async fn all_assays_confirm_once_and_survive_restart() -> Result<(), Error> {
    let mut value = order()?;
    value["tests"]
        .as_array_mut()
        .ok_or("tests")?
        .push(serde_json::json!({"namespace":"synthetic","value":"TEST"}));
    let (temp, path, reference, service, mut peer) = setup(value).await?;
    assert_eq!(byte(&mut peer).await?, 5);
    state(&path, "Sending").await?;
    no_event(&path).await?;
    packet(&mut peer).await?;
    peer.write_all(b"\x06").await?;
    assert_eq!(byte(&mut peer).await?, 3);
    state(&path, "Sending").await?;
    no_event(&path).await?;
    read_packet(&mut peer).await?;
    confirm(&mut peer).await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Sent",
        "VIDAS-transfer-confirmed",
    )
    .await?;
    drop(peer);
    service.stop().await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "vidas").await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Sent",
        "VIDAS-transfer-confirmed",
    )
    .await?;
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// VIDAS BCI Link rev A chapter 3; ADR 0004
#[tokio::test]
async fn ack_timeout_is_uncertain_never_resent() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?).await?;
    assert_eq!(byte(&mut peer).await?, 5);
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "VIDAS-acknowledgment-timeout",
    )
    .await?;
    drop(peer);
    let mut peer = connect(&path, "vidas").await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// VIDAS BCI Link rev A chapter 3; ADR 0004
#[tokio::test]
async fn disconnect_mid_send_is_uncertain() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?).await?;
    assert_eq!(byte(&mut peer).await?, 5);
    packet(&mut peer).await?;
    state(&path, "Sending").await?;
    no_event(&path).await?;
    drop(peer);
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "VIDAS-transfer-disconnected",
    )
    .await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0004; Safety rule 8 (ADR 0003)
#[tokio::test]
async fn restart_during_send_never_replays() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?).await?;
    assert_eq!(byte(&mut peer).await?, 5);
    packet(&mut peer).await?;
    state(&path, "Sending").await?;
    no_event(&path).await?;
    service.crash().await?;
    drop(peer);
    state(&path, "Sending").await?;
    let service = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "vidas").await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "host-restart",
    )
    .await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

async fn rejection(value: serde_json::Value, code: &str) -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(value).await?;
    event(&path, &temp.path().join("receiver.db"), "Rejected", code).await?;
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}
// VIDAS BCI Link rev A chapter 4
#[tokio::test]
async fn foreign_test_is_rejected_before_send() -> Result<(), Error> {
    let mut value = order()?;
    value["tests"]
        .as_array_mut()
        .ok_or("tests")?
        .push(serde_json::json!({"namespace":"lab-app","value":"TEST"}));
    rejection(value,"rejected: test NamespacedId { namespace: Identifier(\"lab-app\"), value: Identifier(\"TEST\") } is not in this machine's code system").await
}
// VIDAS BCI Link rev A chapter 4
#[tokio::test]
async fn specimen_type_is_rejected() -> Result<(), Error> {
    let mut value = order()?;
    value["specimen_type"] = serde_json::json!({"namespace":"synthetic","value":"SERUM"});
    rejection(value, "rejected: unsupported-specimen-type").await
}
// ADR 0004
#[tokio::test]
async fn wrong_patient_namespace_is_rejected() -> Result<(), Error> {
    let mut value = order()?;
    value["patient"]["id"]["namespace"] = "other".into();
    rejection(value, "rejected: patient-namespace-mismatch").await
}
// VIDAS BCI Link rev A chapter 4
#[tokio::test]
async fn missing_patient_is_rejected_before_send() -> Result<(), Error> {
    let mut value = order()?;
    value.as_object_mut().ok_or("order")?.remove("patient");
    rejection(value, "rejected: missing-patient").await
}
// VIDAS PC User Manual F chapter 4.14
#[tokio::test]
async fn overlong_sample_id_is_rejected_not_truncated() -> Result<(), Error> {
    let mut value = order()?;
    value["sample_id"] = "1234567890123".into();
    rejection(value, "rejected: overlong-sample-id").await
}
// VIDAS BCI Link rev A chapter 4
#[tokio::test]
async fn overlong_patient_id_is_rejected_not_truncated() -> Result<(), Error> {
    let mut value = order()?;
    value["patient"]["id"]["value"] = "123456789012345678".into();
    rejection(value, "rejected: overlong-patient-id").await
}
// ADR 0004
#[tokio::test]
async fn cancelled_before_send_sends_nothing() -> Result<(), Error> {
    let mut value = order()?;
    value["state"] = "cancel_requested".into();
    let (_temp, path, reference, service, mut peer) = setup(value).await?;
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let count: i64 =
                database(&path)?.query_row("SELECT count(*) FROM orders", [], |r| r.get(0))?;
            if count == 1 {
                return Ok::<_, Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    // A trusted pass must exist, or "never sent" would hold without the cancel rule.
    wait_feed(&path).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM orders", [], |r| r.get::<_, i64>(0))?,
        1
    );
    no_event(&path).await?;
    assert_eq!(
        database(&path)?.query_row("SELECT count(*) FROM machine_orders", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0004
#[tokio::test]
async fn missing_patient_namespace_prevents_startup() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(
        temp.path(),
        "vidas",
        &definition().rewrite("patient_namespace = \"synthetic\"", ""),
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
        return Err("program admitted a missing patient namespace".into());
    };
    assert!(error.to_string().contains("orders.patient_namespace"));
    assert!(!temp.path().join("data/analyte.db").exists());
    assert_eq!(receiver_count(&receiver_path)?, 0);
    reference.close().await?;
    Ok(())
}

// VIDAS BCI Link rev A chapter 3
#[tokio::test]
async fn park_mid_send_commits_uncertain_then_closes() -> Result<(), Error> {
    let (temp, path, reference, service, mut peer) = setup(order()?).await?;
    assert_eq!(byte(&mut peer).await?, 5);
    packet(&mut peer).await?;
    state(&path, "Sending").await?;
    no_event(&path).await?;
    peer.write_all(b"\x15").await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "VIDAS-transfer-refused-or-unexpected",
    )
    .await?;
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut byte)).await??,
        0
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// VIDAS BCI guide C pp. 3-2 and 10-2; ADR 0004.
#[tokio::test]
async fn second_packet_nak_leaves_entire_order_uncertain() -> Result<(), Error> {
    let mut value = order()?;
    value["tests"]
        .as_array_mut()
        .ok_or("tests")?
        .push(serde_json::json!({"namespace":"synthetic","value":"TEST"}));
    let (temp, path, reference, service, mut peer) = setup(value).await?;
    assert_eq!(byte(&mut peer).await?, 5);
    packet(&mut peer).await?;
    peer.write_all(b"\x06").await?;
    assert_eq!(byte(&mut peer).await?, 3);
    read_packet(&mut peer).await?;
    peer.write_all(b"\x15").await?;
    peer::closed(&mut peer).await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Uncertain",
        "VIDAS-transfer-refused-or-unexpected",
    )
    .await?;
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// VIDAS BCI guide C pp. 4-6 and 10-2.
#[tokio::test]
async fn required_dilution_is_refused_before_enq() -> Result<(), Error> {
    let definition = definition().rewrite(
        "download = true",
        "download = true\ndilution_required_assays = ['TEST']",
    );
    let (temp, path, reference, service, mut peer) =
        setup_with_definition(order()?, &definition).await?;
    event(
        &path,
        &temp.path().join("receiver.db"),
        "Rejected",
        "rejected: required VIDAS dilution is missing",
    )
    .await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), peer.read_u8())
            .await
            .is_err()
    );
    drop(peer);
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

// ADR 0004: without trusted orders, no download session starts.
#[tokio::test]
async fn unavailable_cache_sends_nothing() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "vidas", &definition())?;
    let (_outage, url) = AppOutage::start(&temp.path().join("receiver.db"), true).await?;
    let service = Running::start(&path, &url, "synthetic-api").await?;
    let mut peer = connect(&path, "vidas").await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(1200), peer.read_u8())
            .await
            .is_err()
    );
    no_event(&path).await?;
    drop(peer);
    service.stop().await
}
