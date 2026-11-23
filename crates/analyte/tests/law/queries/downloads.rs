//! Tests LAW order queries and query-triggered work downloads.
use super::*;

// IHE PaLM TF-2b table C.6-2, usage X
#[tokio::test]
async fn ignored_awos_components_do_not_change_work_match() -> Result<(), Error> {
    let (_temp, path, mut reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 1)?],
        &definition().rewrite("specimen = false", "specimen = true"),
        false,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "validate-work").as_bytes()).await?;
    packet(&mut peer).await?;
    let work = packet(&mut peer).await?;
    orl(&mut peer, &work, &["OK"], false).await?;
    vendor::state(&path, "Accepted").await?;
    wait_state(&path, "done", 1).await?;
    let awos = std::str::from_utf8(field(&work, b"OBR", 2)?)?;
    let valid = law_results::result("valid", &format!("{awos}^LAB"), "", "12.30", "F")
        .rewrite(
            "sample-01&99LAB",
            std::str::from_utf8(field(&work, b"SPM", 2)?)?,
        )
        .rewrite(
            "00012a^LAB",
            &format!("{}^LAB", std::str::from_utf8(field(&work, b"SAC", 3)?)?),
        )
        .rewrite(
            "SYN^Synthetic^99LAB",
            std::str::from_utf8(field(&work, b"OBR", 4)?)?,
        );
    running.stop().await?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let baseline = count(&path, "outbox")?;
    for (id, bad, location) in [
        (
            "unknown",
            valid.rewrite(&format!("{awos}^LAB"), "never-downloaded^LAB"),
            "OBR^1^2",
        ),
        (
            "specimen",
            valid.rewrite("000Ab12&", "different&"),
            "SPM^1^2",
        ),
        (
            "specimen-type",
            valid.rewrite("SER^Serum^HL70487", "PLAS^Plasma^HL70487"),
            "SPM^1^4",
        ),
        (
            "container",
            valid.rewrite("000Ab12^LAB", "different^LAB"),
            "SAC^1^3",
        ),
        (
            "test",
            valid.replacen(
                std::str::from_utf8(field(&work, b"OBR", 4)?)?,
                "OTHER^Other^99LAB",
                1,
            ),
            "OBR^1^4",
        ),
    ] {
        let bad = bad.rewrite("|valid|", &format!("|{id}|"));
        let unsolicited =
            law_results::result("unused", "\"\"", "unsolicited-run^ANALYZER", "1.0", "F");
        let (_, body) = unsolicited
            .split_once('\r')
            .ok_or("missing synthetic header")?;
        let mixed = format!("{bad}{}", body.replacen("SPM|1|", "SPM|2|", 1));
        send(&mut peer, mixed.as_bytes()).await?;
        let ack = packet(&mut peer).await?;
        assert_eq!(field(&ack, b"MSA", 1)?, b"AR", "{id}");
        assert_eq!(field(&ack, b"ERR", 2)?, location.as_bytes(), "{id}");
        assert_eq!(count(&path, "outbox")?, baseline);
        assert_eq!(count(&path, "observation_revisions")?, 0);
    }
    for (index, suffix) in ["", "^OTHER", "^OTHER^arbitrary^ignored"]
        .into_iter()
        .enumerate()
    {
        let accepted = valid
            .rewrite(&format!("{awos}^LAB"), &format!("{awos}{suffix}"))
            .rewrite("|valid|", &format!("|valid-{index}|"));
        send(&mut peer, accepted.as_bytes()).await?;
        let ack = packet(&mut peer).await?;
        assert_eq!(
            field(&ack, b"MSA", 1)?,
            b"AA",
            "variant {index}: {}",
            String::from_utf8_lossy(ack.raw())
        );
    }
    send(&mut peer, valid.as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"MSA", 1)?, b"AA");
    reference.event().await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b Rev 11 B.7.1-B.7.2 pp.80-81; C.1 pp.84-85.
// IHE PaLM TF-2b §B.7.1, §B.7.2
#[tokio::test]
async fn host_correlates_errors_without_ack_loops() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(vec![], &definition(), false).await?;
    let query = query(CONTAINER, "correlated");
    let result = law_results::result("correlated", "\"\"", "run^ANALYZER", "1.0", "F");
    for (message, code) in [
        (query.rewrite("QPD|", "QPD|BROKEN"), b"AE".as_slice()),
        (query.rewrite("QBP^Q11^QBP_Q11", "QBP^Q99^QBP_Q11"), b"AR"),
        (query.rewrite("|2.5.1|", "|2.4|"), b"AR"),
        (query.rewrite("LAB-27^IHE", "OTHER^IHE"), b"AR"),
        (result.rewrite("OUL^R22^OUL_R22", "ORU^R01^ORU_R01"), b"AR"),
        (format!("{query}@@@|bad\r"), b"AE"),
    ] {
        send(&mut peer, message.as_bytes()).await?;
        let ack = packet(&mut peer).await?;
        assert_eq!(field(&ack, b"MSA", 1)?, code);
        assert_eq!(field(&ack, b"MSA", 2)?, b"correlated");
        assert!(ack.segments().any(|s| s.id() == b"ERR"));
    }
    for kind in ["ACK^R22^ACK", "ORL^O34^ORL_O42", "RSP^K11^RSP_K11"] {
        let ack = query
            .rewrite("QBP^Q11^QBP_Q11", kind)
            .rewrite("|2.5.1|", "|2.4|");
        send(&mut peer, ack.as_bytes()).await?;
        quiet(&mut peer).await?;
    }
    assert_eq!(wait_count(&path, "receipts", 9).await?, 9);
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b 3.27.4 and ADR 0003: bound exhaustion cannot issue a subset.
// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn pending_bound_never_returns_partial_work() -> Result<(), Error> {
    let orders = (0..17)
        .map(|i| order(&format!("order-{i:02}"), &format!("tube-{i:02}"), None, 1))
        .collect::<Result<Vec<_>, _>>()?;
    let settings = definition().rewrite("records = 4096", "records = 16");
    let (_temp, path, reference, running, mut peer) = setup(orders, &settings, false).await?;
    send(&mut peer, query(ALL, "oversized").as_bytes()).await?;
    let rsp = packet(&mut peer).await?;
    assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "machine_order_items")?, 0);
    assert!(count(&path, "parked")? > 0);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b 3.28.4 / ADR 0003: failed raw custody cannot accept any item.
// IHE PaLM TF-2b §3.28.4; ADR 0003
#[tokio::test]
async fn failed_orl_write_never_finishes_items() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", None, 2)?],
        &definition(),
        false,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "orl-storage").as_bytes()).await?;
    packet(&mut peer).await?;
    let work = packet(&mut peer).await?;
    database(&path)?.execute_batch("CREATE TRIGGER fail_orl BEFORE INSERT ON receipts BEGIN SELECT RAISE(FAIL,'synthetic-ORL-custody-failure'); END;")?;
    orl(&mut peer, &work, &["OK", "OK"], false).await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut [0])).await??,
        0
    );
    vendor::state(&path, "Uncertain").await?;
    let states = database(&path)?
        .prepare("SELECT state FROM machine_order_items ORDER BY item_index")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(states, ["Uncertain", "Uncertain"]);
    assert_eq!(count(&path, "receipts")?, 1);
    database(&path)?.execute_batch("DROP TRIGGER fail_orl")?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b 3.28.4 and ADR 0003: raw custody precedes every AWOS decision.
// IHE PaLM TF-2b §3.28.4; ADR 0003
#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn orl_commit_precedes_each_item_outcome() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    reference
        .receiver
        .seed(vec![order("order-1", "000Ab12", None, 2)?])
        .await?;
    let mut loaded = program(&path, &reference.url)?;
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    loaded.pause_law_precommit_for_test("law", hook)?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    wait_feed(&path).await?;
    send(&mut peer, query(CONTAINER, "custody").as_bytes()).await?;
    control.reached().await?;
    assert_eq!(count(&path, "receipts")?, 0);
    assert_eq!(count(&path, "machine_order_items")?, 0);
    quiet(&mut peer).await?;
    control.resume();
    packet(&mut peer).await?;
    let work = packet(&mut peer).await?;
    control.arm_next();
    let raw = orl(&mut peer, &work, &["OK", "UA"], false).await?;
    control.reached().await?;
    assert_eq!(count(&path, "receipts")?, 1);
    let states = database(&path)?
        .prepare("SELECT state FROM machine_order_items ORDER BY item_index")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(states, ["Sending", "Sending"]);
    assert_eq!(count(&path, "outbox")?, 0);
    control.resume();
    vendor::state(&path, "Uncertain").await?;
    let stored: Vec<u8> =
        database(&path)?.query_row("SELECT raw FROM receipts WHERE raw=?1", [&raw], |r| {
            r.get(0)
        })?;
    assert_eq!(stored, raw);
    let states = database(&path)?
        .prepare("SELECT state FROM machine_order_items ORDER BY item_index")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(states, ["Accepted", "Rejected"]);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// IHE PaLM TF-2b §3.27.4; ADR 0003
#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn raw_commit_precedes_query_response() -> Result<(), Error> {
    for form in [CONTAINER, ALL] {
        let temp = tempfile::tempdir()?;
        let path = vendor_config(temp.path(), "law", &definition())?;
        let reference = Reference::start(
            &temp.path().join("receiver.db"),
            TcpListener::bind("127.0.0.1:0").await?,
            false,
        )
        .await?;
        reference
            .receiver
            .seed(vec![order("order-1", "000Ab12", None, 1)?])
            .await?;
        let mut loaded = program(&path, &reference.url)?;
        let (hook, control) = analyte::runtime::test_hooks::Committed::new();
        loaded.pause_law_committed_for_test("law", hook)?;
        let running = Running::start_program(loaded, "synthetic-api").await?;
        let mut peer = connect(&path, "law").await?;
        wait_feed(&path).await?;
        let request = query(form, "durable");
        send(&mut peer, request.as_bytes()).await?;
        control.reached().await?;
        let raw: Vec<u8> =
            database(&path)?.query_row("SELECT raw FROM receipts", [], |row| row.get(0))?;
        assert_eq!(
            raw,
            analyte_framing::hl7::encode(
                request.as_bytes(),
                analyte_framing::hl7::Limit::new(1048576)?
            )?
        );
        assert_eq!(count(&path, "machine_orders")?, 0);
        quiet(&mut peer).await?;
        control.resume();
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
        let work = packet(&mut peer).await?;
        assert_eq!(count(&path, "machine_order_items")?, 1);
        orl(&mut peer, &work, &["OK"], false).await?;
        wait_state(&path, "done", 1).await?;
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// ADR 0004 / rule 9 / LAW TF-2b 3.27.4: only configured fresh emptiness is a negative work answer.
// Safety rule 9 (ADR 0003)
#[tokio::test]
async fn law_query_freshness_follows_destination_config() -> Result<(), Error> {
    for freshness in [10, 120] {
        let (_temp, path, reference, running, mut peer) =
            setup_with_freshness(Vec::new(), &definition(), false, freshness).await?;
        reference.close().await?;
        database(&path)?.execute(
            "UPDATE feed SET polled=?1",
            [jiff::Timestamp::now().as_second() - 50],
        )?;
        send(&mut peer, query(ALL, "configured-age").as_bytes()).await?;
        let response = packet(&mut peer).await?;
        if freshness == 10 {
            assert_eq!(field(&response, b"QAK", 2)?, b"AE");
        } else {
            assert_eq!(field(&response, b"QAK", 2)?, b"OK");
            let negative = packet(&mut peer).await?;
            assert_eq!(field(&negative, b"ORC", 1)?, b"DC");
            orl(&mut peer, &negative, &[], false).await?;
        }
        running.stop().await?;
    }
    Ok(())
}

// IHE PaLM TF-2b Rev 11 B.3.1 pp.63-64 and B.7.6 p.82.

// PaLM TF-2b B.6 / ADR 0003: wire expiry cannot cancel raw custody.
// IHE PaLM TF-2b §B.6; ADR 0003
#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn wire_deadline_does_not_cancel_raw_custody() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let settings = definition().rewrite("receive_timeout_ms = 30000", "receive_timeout_ms = 50");
    let path = vendor_config(temp.path(), "law", &settings)?;
    let reference = Reference::start(
        &temp.path().join("receiver.db"),
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let mut loaded = program(&path, &reference.url)?;
    let (hook, control) = analyte::runtime::test_hooks::Committed::new();
    loaded.pause_law_precommit_for_test("law", hook)?;
    let running = Running::start_program(loaded, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    let request = query(CONTAINER, "custody-expiry");
    send(&mut peer, request.as_bytes()).await?;
    control.reached().await?;
    // The existing no-bytes observation spans the 50 ms wire deadline while
    // custody is gated. Closing here would cancel the uncommitted receipt.
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "receipts")?, 0);
    control.resume();
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, peer.read(&mut [0])).await??,
        0
    );
    let raw: Vec<u8> = database(&path)?.query_row("SELECT raw FROM receipts", [], |r| r.get(0))?;
    assert_eq!(
        raw,
        analyte_framing::hl7::encode(
            request.as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?
        )?
    );
    assert_eq!(count(&path, "machine_order_items")?, 0);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// Contract v1, Order tests.
#[tokio::test]
async fn query_rejects_dilution_without_sending_work() -> Result<(), Error> {
    let mut document = serde_json::to_value(order("order-1", "000Ab12", Some("law"), 1)?)?;
    document
        .get_mut("tests")
        .and_then(|tests| tests.get_mut(0))
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("synthetic test")?
        .insert("dilution_ratio".into(), 5.into());
    let (temp, path, reference, running, mut peer) = setup(
        vec![analyte_contract::decode(&serde_json::to_vec(&document)?)?],
        &definition(),
        false,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "dilution").as_bytes()).await?;
    let reply = packet(&mut peer).await?;
    assert_eq!(field(&reply, b"QAK", 2)?, b"AE");
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "machine_order_items")?, 0);
    wait_state(&path, "done", 1).await?;
    let body: Vec<u8> = rusqlite::Connection::open(temp.path().join("receiver.db"))?.query_row(
        "SELECT body FROM deliveries WHERE kind='order-events'",
        [],
        |row| row.get(0),
    )?;
    let body: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(
        body.get("state").and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    assert!(
        body.get("source_status")
            .ok_or("source status")?
            .as_str()
            .ok_or("source status")?
            .contains("unsupported-dilution")
    );
    running.stop().await?;
    reference.close().await?;
    Ok(())
}
