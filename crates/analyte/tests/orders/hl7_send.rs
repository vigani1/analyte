//! Tests HL7 unsolicited orders and cancellations.
use super::vendor_support::state;
use super::*;
use crate::support::HANG_GUARD;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
async fn setup(
    edition: &str,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    setup_with_id(edition, "order-1", false, 30000).await
}
async fn setup_with_id(
    edition: &str,
    id: &str,
    paused: bool,
    response_ms: u32,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    setup_with_patient_requirement(edition, id, paused, response_ms, false).await
}

async fn setup_with_patient_requirement(
    edition: &str,
    id: &str,
    paused: bool,
    response_ms: u32,
    require_patient: bool,
) -> Result<(tempfile::TempDir, Fixture, Reference, Running, TcpStream), Error> {
    let temp = tempfile::tempdir()?;
    let path = config(temp.path(), false)?;
    let definition = std::fs::read_to_string(temp.path().join("hl7.toml"))?
        .replace(r#"edition = "v251""#, &format!(r#"edition = "{edition}""#));
    std::fs::write(
        temp.path().join("hl7.toml"),
        format!(
            "{definition}\n{}",
            include_str!("../../../core/config/examples/hl7_send.toml")
                .replace(
                    "require_patient_identification = false",
                    &format!("require_patient_identification = {require_patient}")
                )
                .replace(
                    "response_ms = 1500",
                    &format!("response_ms = {response_ms}")
                )
        ),
    )?;
    if matches!(edition, "v26" | "v282") {
        let file = temp.path().join("hl7.toml");
        let configured = std::fs::read_to_string(&file)?
            .replace(r#"["SYN","","L"]"#, r#"["SYN","","L","","","","test-r1"]"#);
        std::fs::write(
            file,
            format!("{configured}\npriority_version = \"priority1\"\n"),
        )?;
    }
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
    if id == "known-patient" {
        value.as_object_mut().ok_or("order")?.insert(
            "patient".into(),
            serde_json::json!({"id":{"namespace":"patients","value":"P1"}}),
        );
    }
    *value.get_mut("id").ok_or("id")? = serde_json::json!(id);
    *value.get_mut("tests").ok_or("tests")? = serde_json::json!([{"namespace":"synthetic","value":"TEST"},{"namespace":"synthetic","value":"TEST"}]);
    value.as_object_mut().ok_or("order")?.insert(
        "specimen_type".into(),
        serde_json::json!({"namespace":"types","value":"SERUM"}),
    );
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &value,
        )?)?])
        .await?;
    if paused {
        let program = program(&path, &reference.url)?;
        let mut store = analyte::open_system_store(&program).await?;
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
    }
    let mut program = program(&path, &reference.url)?;
    // Send-ahead cases check lifecycle, rather than the feed polling interval.
    program.set_loopback_poll_interval_for_test("demo", Duration::from_millis(100))?;
    let service = Running::start_program(program, "synthetic-api").await?;
    let peer = connect(&path, "hl7").await?;
    Ok((temp, path, reference, service, peer))
}

async fn reply(
    peer: &mut TcpStream,
    sent: &[u8],
    codes: &[&str],
    edition: &str,
) -> Result<(), Error> {
    let message = analyte_hl7::Message::parse(sent, analyte_hl7::Options::default())?;
    let header = analyte_hl7::Msh::new(&message)?;
    let family = if edition == "2.3.1" {
        "ORR^O02^ORR_O02"
    } else {
        "ORL^O22^ORL_O22"
    };
    let mut response = format!(
        "MSH|^~\\&|SYN|LAB|HOST|LAB|202611151200||{family}|reply|T|{edition}\rMSA|AA|{}\r",
        std::str::from_utf8(header.control_id())?
    );
    if edition != "2.3.1" {
        response.push_str(if edition == "2.8.2" {
            "PID|1||P1^^^LAB^MR||Synthetic\r"
        } else {
            "PID|1\r"
        });
    }
    for (orc, code) in message
        .segments()
        .filter(|segment| segment.id() == b"ORC")
        .zip(codes)
    {
        response.push_str(&format!(
            "ORC|{code}|{}",
            std::str::from_utf8(orc.field(2).ok_or("placer")?.raw())?
        ));
        if matches!(*code, "UA" | "UC") {
            response.push_str(if matches!(edition, "2.6" | "2.8.2") {
                "||||||||||||||SYN^Machine reason^L^^^^reason-r1"
            } else {
                "||||||||||||||SYN^Machine reason^L"
            });
        }
        response.push('\r');
    }
    peer.write_all(&analyte_framing::hl7::encode(
        response.as_bytes(),
        analyte_framing::hl7::Limit::new(1048576)?,
    )?)
    .await?;
    Ok(())
}
async fn outcome(path: &Path, expected: &str) -> Result<(), Error> {
    state(path, expected).await?;
    wait_state(path, "done", 1).await
}
#[tokio::test]
async fn hl7_231_251_26_282_s4_4_7_send_ahead_accepted_rejected_and_mixed_items()
-> Result<(), Error> {
    for (edition, version) in [
        ("v231", "2.3.1"),
        ("v251", "2.5.1"),
        ("v26", "2.6"),
        ("v282", "2.8.2"),
    ] {
        for (codes, expected) in [
            (["OK", "OK"], "Accepted"),
            (["UA", "UA"], "Rejected"),
            (["OK", "UA"], "Uncertain"),
        ] {
            let (_temp, path, reference, service, mut peer) = setup(edition).await?;
            let bytes = read_mllp_packet(&mut peer, "oversized packet").await?;
            let message = analyte_hl7::Message::parse(&bytes, analyte_hl7::Options::default())?;
            assert_eq!(
                analyte_hl7::Msh::new(&message)?.message_type().next(),
                Some(if edition == "v231" {
                    b"ORM".as_slice()
                } else {
                    b"OML".as_slice()
                })
            );
            let ids = message
                .segments()
                .filter(|segment| segment.id() == b"ORC")
                .map(|segment| {
                    Ok(std::str::from_utf8(segment.field(2).ok_or("identity")?.raw())?.to_owned())
                })
                .collect::<Result<Vec<_>, Error>>()?;
            assert_eq!(ids, vec!["1-1^LAB", "1-2^LAB"]);
            for segment in message.segments().filter(|segment| segment.id() == b"ORC") {
                // HL7 2.8.2 section 4.5.1.4 changes ORC-4 from EI to EIP.
                assert_eq!(
                    segment.field(4).ok_or("group")?.raw(),
                    if edition == "v282" {
                        b"order-1&LAB"
                    } else {
                        b"order-1^LAB"
                    }
                );
            }
            state(&path, "Sending").await?;
            assert_eq!(
                database(&path)?
                    .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                0
            );
            reply(&mut peer, &bytes, &codes, version).await?;
            outcome(&path, expected).await?;
            assert_eq!(
                database(&path)?.query_row(
                    "SELECT count(*) FROM machine_order_items",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                2
            );
            assert_eq!(
                database(&path)?
                    .query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))?,
                1
            );
            service.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}
#[tokio::test]
async fn hl7_251_26_282_rule8_no_reply_disconnect_and_restart_never_resend() -> Result<(), Error> {
    for (edition, disconnect) in [
        ("v251", false),
        ("v251", true),
        ("v26", false),
        ("v26", true),
        ("v282", false),
        ("v282", true),
    ] {
        let (_temp, path, reference, service, mut peer) = setup_with_id(
            edition,
            "order-1",
            false,
            if disconnect { 30000 } else { 250 },
        )
        .await?;
        read_mllp_packet(&mut peer, "oversized packet").await?;
        if disconnect {
            drop(peer);
        }
        outcome(&path, "Uncertain").await?;
        service.stop().await?;
        let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "hl7").await?;
        assert!(
            // Deliberate quiet window: no reply may arrive.
            tokio::time::timeout(Duration::from_millis(250), peer.read_u8())
                .await
                .is_err()
        );
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
async fn cancel_cache(path: &Path, reference: &Reference) -> Result<(), Error> {
    let db = database(path)?;
    let evidence: String =
        db.query_row("SELECT evidence FROM orders WHERE id='order-1'", [], |r| {
            r.get(0)
        })?;
    let cached: analyte_store::CachedOrder = serde_json::from_str(&evidence)?;
    let mut document: serde_json::Value = serde_json::from_str(&cached.document)?;
    *document.get_mut("state").ok_or("state")? = serde_json::json!("cancel_requested");
    *document.get_mut("version").ok_or("version")? = serde_json::json!(2);
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &document,
        )?)?])
        .await?;
    Ok(())
}
#[tokio::test]
async fn hl7_231_251_cancel_accepted_or_refused_restores_prior_state_once() -> Result<(), Error> {
    for (edition, version) in [
        ("v231", "2.3.1"),
        ("v251", "2.5.1"),
        ("v26", "2.6"),
        ("v282", "2.8.2"),
    ] {
        for refused in [false, true] {
            let (_temp, path, reference, service, mut peer) = setup(edition).await?;
            let bytes = read_mllp_packet(&mut peer, "oversized packet").await?;
            reply(&mut peer, &bytes, &["OK", "OK"], version).await?;
            outcome(&path, "Accepted").await?;
            cancel_cache(&path, &reference).await?;
            let cancel = read_mllp_packet(&mut peer, "oversized packet").await?;
            let parsed = analyte_hl7::Message::parse(&cancel, analyte_hl7::Options::default())?;
            assert!(
                parsed
                    .segments()
                    .filter(|row| row.id() == b"ORC")
                    .all(|row| row.field(1).is_some_and(|field| field.raw() == b"CA"))
            );
            assert!(!parsed.segments().any(|row| row.id() == b"OBR"));
            reply(
                &mut peer,
                &cancel,
                if refused {
                    &["UC", "UC"]
                } else {
                    &["CR", "CR"]
                },
                version,
            )
            .await?;
            state(
                &path,
                if refused {
                    "Accepted"
                } else {
                    "CancelConfirmed"
                },
            )
            .await?;
            wait_state(&path, "done", 3).await?;
            assert_eq!(receiver_count(&_temp.path().join("receiver.db"))?, 3);
            assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.source_code') LIKE 'app-cancel:%'",[],|r|r.get::<_,i64>(0))?,1);
            if refused {
                let code: String = database(&path)?.query_row(
                    "SELECT source_code FROM machine_orders",
                    [],
                    |r| r.get(0),
                )?;
                assert!(code.contains("CancelRefused"));
            }
            service.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn hl7_231_orc4_overlong_group_is_rejected_before_any_reservation() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) =
        setup_with_id("v231", "app-order-identifier-too-long", false, 30000).await?;
    outcome(&path, "Rejected").await?;
    let code: String =
        database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |row| {
            row.get(0)
        })?;
    assert!(code.contains("HL7-app-order-id-too-long-for-ORC-4"));
    assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='Sending'",[],|row|row.get::<_,i64>(0))?,0);
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(200), peer.read_u8())
            .await
            .is_err()
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn adr0003_rule8_hl7_251_26_282_restart_during_sending_recovers_items_without_resend()
-> Result<(), Error> {
    for edition in ["v251", "v26", "v282"] {
        let (_temp, path, reference, service, mut peer) = setup(edition).await?;
        read_mllp_packet(&mut peer, "oversized packet").await?;
        state(&path, "Sending").await?;
        service.crash().await?;
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
        state(&path, "Sending").await?;
        let restarted = Running::start(&path, &reference.url, "synthetic-api").await?;
        let mut peer = connect(&path, "hl7").await?;
        outcome(&path, "Uncertain").await?;
        let code: String =
            database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |row| {
                row.get(0)
            })?;
        assert_eq!(code, "host-restart");
        assert!(
            // Deliberate quiet window: no reply may arrive.
            tokio::time::timeout(Duration::from_millis(250), peer.read_u8())
                .await
                .is_err()
        );
        restarted.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn hl7_ei_actual_store_id_overflow_rejects_before_sending_reservation() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) = setup("v251").await?;
    let bytes = read_mllp_packet(&mut peer, "oversized packet").await?;
    reply(&mut peer, &bytes, &["OK", "OK"], "2.5.1").await?;
    outcome(&path, "Accepted").await?;
    database(&path)?.execute(
        "UPDATE sqlite_sequence SET seq=9000000000000000000 WHERE name='machine_orders'",
        [],
    )?;
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/contract/vectors/order.json"
    ))?;
    *value.get_mut("id").ok_or("id")? = serde_json::json!("order-2");
    value.as_object_mut().ok_or("object")?.insert(
        "specimen_type".into(),
        serde_json::json!({"namespace":"types","value":"SERUM"}),
    );
    reference
        .receiver
        .seed(vec![analyte_contract::decode(&serde_json::to_vec(
            &value,
        )?)?])
        .await?;
    tokio::time::timeout(HANG_GUARD, state(&path, "Rejected")).await??;
    let code: String = database(&path)?.query_row(
        "SELECT source_code FROM machine_orders WHERE order_id='order-2'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(code, "HL7-placer-identity-too-long");
    assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.key.order')='order-2' AND json_extract(evidence,'$.state')='Sending'",[],|row|row.get::<_,i64>(0))?,0);
    assert!(
        // Deliberate quiet window: no reply may arrive.
        tokio::time::timeout(Duration::from_millis(200), peer.read_u8())
            .await
            .is_err()
    );
    service.stop().await?;
    reference.close().await?;
    Ok(())
}

#[tokio::test]
async fn plan11_hl7_send_ahead_reserves_nothing_while_restore_paused() -> Result<(), Error> {
    let (_temp, path, reference, service, mut peer) =
        setup_with_id("v251", "order-1", true, 30000).await?;
    // A completed feed proves orders are available; shutdown joins the machine
    // before inspecting its final durable reservations and buffered wire output.
    wait_feed(&path).await?;
    service.stop().await?;
    let mut wire = Vec::new();
    tokio::time::timeout(HANG_GUARD, peer.read_to_end(&mut wire)).await??;
    assert!(wire.is_empty(), "paused machine emitted work");
    for table in [
        "machine_orders",
        "machine_order_items",
        "machine_order_history",
        "outbox",
    ] {
        assert_eq!(
            database(&path)?
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))?,
            0
        );
    }

    reference.close().await?;
    Ok(())
}

// HL7 2.3.1 section 3.3.2 p.3-36: no name is available in contract v1.
#[tokio::test]
async fn hl7_231_3_3_2_required_pid_rejects_before_reservation_without_inventing_name()
-> Result<(), Error> {
    for (edition, required) in [("v231", true), ("v251", true), ("v231", false)] {
        let (_temp, path, reference, service, mut peer) =
            setup_with_patient_requirement(edition, "known-patient", false, 30000, required)
                .await?;
        outcome(&path, "Rejected").await?;
        let reason: String =
            database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |row| {
                row.get(0)
            })?;
        assert!(
            reason.contains("HL7-patient-name-required-but-not-carried-by-contract"),
            "{reason}"
        );
        assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='Sending'", [], |row| row.get::<_, i64>(0))?, 0);
        assert!(
            // Deliberate quiet window: no reply may arrive.
            tokio::time::timeout(Duration::from_millis(200), peer.read_u8())
                .await
                .is_err()
        );
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// HL7 2.3.1 section 4.3.1.4 p.4-17; 2.5.1 section 4.5.1.4 p.4-39.
// The scalar fits 22 bytes, but the EI including its namespace does not.
#[tokio::test]
async fn hl7_231_4_3_1_4_251_4_5_1_4_full_group_ei_overflow_precedes_reservation()
-> Result<(), Error> {
    for edition in ["v231", "v251"] {
        let (_temp, path, reference, service, mut peer) =
            setup_with_id(edition, &"g".repeat(19), false, 30000).await?;
        outcome(&path, "Rejected").await?;
        let code: String =
            database(&path)?.query_row("SELECT source_code FROM machine_orders", [], |row| {
                row.get(0)
            })?;
        assert!(code.contains("HL7-app-order-id-too-long-for-ORC-4"));
        assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='Sending'", [], |row| row.get::<_, i64>(0))?, 0);
        assert!(
            // Deliberate quiet window: no reply may arrive.
            tokio::time::timeout(Duration::from_millis(200), peer.read_u8())
                .await
                .is_err()
        );
        service.stop().await?;
        reference.close().await?;
    }
    Ok(())
}
