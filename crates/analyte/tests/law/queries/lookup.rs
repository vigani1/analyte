//! Tests LAW order queries and query-triggered work downloads.
use super::*;

// IHE PaLM TF-2b §3.27, §3.28
#[tokio::test]
async fn found_query_reserves_and_finishes_each_awos() -> Result<(), Error> {
    for (codes, expected) in [
        (["OK", "OK"], "Accepted"),
        (["UA", "UA"], "Rejected"),
        (["OK", "UA"], "Uncertain"),
    ] {
        for form in [CONTAINER, ALL] {
            let (temp, path, reference, running, mut peer) = setup(
                vec![order("order-1", "000Ab12", Some("law"), 2)?],
                &definition(),
                false,
            )
            .await?;
            let request = query(form, "found");
            send(&mut peer, request.as_bytes()).await?;
            let rsp = packet(&mut peer).await?;
            assert_eq!(field(&rsp, b"MSH", 9)?, b"RSP^K11^RSP_K11");
            assert_eq!(field(&rsp, b"MSA", 1)?, b"AA", "{}", database(&path)?.query_row("SELECT coalesce((SELECT json_extract(evidence,'$.outcome.Failed') FROM interpretations ORDER BY rowid DESC LIMIT 1),'no interpretation')", [], |row| row.get::<_, String>(0))?);
            assert_eq!(field(&rsp, b"MSA", 2)?, b"found");
            assert_eq!(field(&rsp, b"QAK", 2)?, b"OK");
            let raw: Vec<u8> = database(&path)?.query_row(
                "SELECT raw FROM receipts ORDER BY rowid LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(
                raw,
                analyte_framing::hl7::encode(
                    request.as_bytes(),
                    analyte_framing::hl7::Limit::new(1048576)?
                )?
            );
            let work = packet(&mut peer).await?;
            assert_eq!(field(&work, b"MSH", 9)?, b"OML^O33^OML_O33");
            assert_ne!(field(&work, b"MSH", 10)?, field(&rsp, b"MSH", 10)?);
            let ids = work
                .segments()
                .filter(|s| s.id() == b"OBR")
                .map(|s| s.field(2).map(|f| f.raw().to_vec()).ok_or("AWOS"))
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(ids, [b"1-1".to_vec(), b"1-2".to_vec()]);
            assert_eq!(field(&work, b"SAC", 3)?, b"000Ab12");
            assert_eq!(count(&path, "machine_order_items")?, 2);
            vendor::state(&path, "Sending").await?;
            assert_eq!(count(&path, "outbox")?, 0);
            let raw_orl = orl(&mut peer, &work, &codes, false).await?;
            vendor::state(&path, expected).await?;
            let states = database(&path)?
                .prepare(
                    "SELECT state,placer_namespace FROM machine_order_items ORDER BY item_index",
                )?
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                states,
                codes
                    .iter()
                    .map(|code| (
                        if *code == "OK" {
                            "Accepted"
                        } else {
                            "Rejected"
                        }
                        .to_owned(),
                        "LAB".to_owned()
                    ))
                    .collect::<Vec<_>>()
            );
            wait_state(&path, "done", 1).await?;
            let stored_orl: Vec<u8> = database(&path)?.query_row(
                "SELECT raw FROM receipts ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(stored_orl, raw_orl);
            assert_eq!(receiver_count(&temp.path().join("receiver.db"))?, 1);
            // A repeated query cannot create another reservation or resend the AWOS.
            send(&mut peer, query(form, "repeat").as_bytes()).await?;
            assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
            let negative = packet(&mut peer).await?;
            assert!(!negative.segments().any(|s| s.id() == b"OBR"));
            orl(&mut peer, &negative, &[], false).await?;
            wait_audit(&path).await?;
            assert_eq!(count(&path, "machine_orders")?, 1);
            assert_eq!(count(&path, "outbox")?, 1);
            quiet(&mut peer).await?;
            running.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}

// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn trusted_empty_sends_negative_work_without_items() -> Result<(), Error> {
    for form in [CONTAINER, ALL] {
        let (_temp, path, reference, running, mut peer) =
            setup(vec![], &definition(), false).await?;
        send(&mut peer, query(form, "empty").as_bytes()).await?;
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
        let negative = packet(&mut peer).await?;
        assert_eq!(
            negative
                .segments()
                .map(analyte_hl7::Segment::id)
                .collect::<Vec<_>>(),
            [b"MSH".as_slice(), b"SPM", b"SAC", b"ORC"]
        );
        assert_eq!(field(&negative, b"ORC", 1)?, b"DC");
        assert_eq!(
            field(&negative, b"SAC", 3)?,
            if form == ALL {
                b"\"\"".as_slice()
            } else {
                b"000Ab12".as_slice()
            }
        );
        let raw_orl = orl(&mut peer, &negative, &[], false).await?;
        wait_audit(&path).await?;
        let (orl_id, stored): (String, Vec<u8>) = database(&path)?.query_row(
            "SELECT id,raw FROM receipts WHERE raw=?1",
            [&raw_orl],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(stored, raw_orl);
        let query_id: String =
            database(&path)?.query_row("SELECT id FROM receipts WHERE id<>?1", [&orl_id], |r| {
                r.get(0)
            })?;
        let audit: String = database(&path)?.query_row("SELECT evidence FROM audit WHERE json_extract(evidence,'$.action')='LAW-negative-query-ORL'", [], |r| r.get(0))?;
        let audit: serde_json::Value = serde_json::from_str(&audit)?;
        assert_eq!(audit["actor"], "law");
        assert_eq!(
            audit["targets"],
            serde_json::json!([
                serde_json::from_str::<String>(&orl_id)?,
                serde_json::from_str::<String>(&query_id)?
            ])
        );
        assert_eq!(
            audit["reason"],
            "negative followup ORL correlated=true acceptance=Aa"
        );
        assert_eq!(count(&path, "receipts")?, 2);
        assert_eq!(count(&path, "machine_orders")?, 0);
        assert_eq!(count(&path, "outbox")?, 0);
        quiet(&mut peer).await?;
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn all_pending_pages_filter_other_analyzers() -> Result<(), Error> {
    let mut orders = (0..100)
        .map(|index| order(&format!("a-{index:03}"), "000other", Some("other"), 1))
        .collect::<Result<Vec<_>, _>>()?;
    orders.push(order("b-law", "000Ab12", Some("law"), 1)?);
    orders.push(order("c-any", "000cd34", None, 1)?);
    let settings = definition();
    let (_temp, path, reference, running, mut peer) = setup(orders, &settings, false).await?;
    send(&mut peer, query(ALL, "all").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    let work = packet(&mut peer).await?;
    let samples = work
        .segments()
        .filter(|s| s.id() == b"SAC")
        .map(|s| s.field(3).map(|f| f.raw().to_vec()).ok_or("container"))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(samples, [b"000Ab12".to_vec(), b"000cd34".to_vec()]);
    assert_eq!(count(&path, "machine_orders")?, 2);
    orl(&mut peer, &work, &["OK", "UA"], false).await?;
    wait_state(&path, "done", 2).await?;
    assert_eq!(count(&path, "outbox")?, 2);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// IHE PaLM TF-2b §3.27, §B.7
#[tokio::test]
async fn unsupported_query_forms_get_ar() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", None, 1)?],
        &definition(),
        false,
    )
    .await?;
    for form in [
        "WOS_BY_RACK^Work Order Step by Rack^IHELAW|tag||rack|01",
        "WOS_BY_TRAY^Work Order Step by Tray^IHELAW|tag||||tray|1^2",
        "WOS_BY_RACK^Work Order Step by Rack^IHELAW|tag||rack|01|||room^Bench^99LAB",
        "WOS_BY_TRAY^Work Order Step by Tray^IHELAW|tag||||tray|1^2|room^Bench^99LAB",
        "WOS_BY_ISOLATE^Work Order Step by Isolate^IHELAW|tag|child||||||parent",
    ] {
        // TF-2b 3.27.4.1.2.3: reach host option refusal only after profile validation.
        let bytes = query(form, "unsupported");
        let request = analyte_hl7::law::parse_message(bytes.as_bytes(), options())?;
        analyte_hl7::law::Query::new(&request)?;
        send(&mut peer, bytes.as_bytes()).await?;
        let rsp = packet(&mut peer).await?;
        assert_eq!(field(&rsp, b"QAK", 2)?, b"AR");
        assert!(
            rsp.raw()
                .windows(b"LAW-unsupported-query".len())
                .any(|value| value == b"LAW-unsupported-query")
        );
        quiet(&mut peer).await?;
    }
    assert_eq!(count(&path, "machine_orders")?, 0);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn untrusted_or_paused_lookup_is_an_error() -> Result<(), Error> {
    for paused in [false, true] {
        for form in [CONTAINER, ALL] {
            let (_temp, path, reference, running, mut peer) = setup(
                vec![order("order-1", "000Ab12", None, 1)?],
                &definition(),
                paused,
            )
            .await?;
            if !paused {
                database(&path)?
                    .execute_batch("ALTER TABLE orders RENAME TO synthetic_unreachable_orders")?;
            }
            send(&mut peer, query(form, "untrusted").as_bytes()).await?;
            let rsp = packet(&mut peer).await?;
            assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
            quiet(&mut peer).await?;
            assert_eq!(count(&path, "machine_orders")?, 0);
            if !paused {
                database(&path)?
                    .execute_batch("ALTER TABLE synthetic_unreachable_orders RENAME TO orders")?;
            }
            running.stop().await?;
            reference.close().await?;
        }
    }
    Ok(())
}

// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn query_budget_counts_first_byte_and_storage() -> Result<(), Error> {
    for form in [CONTAINER, ALL] {
        let settings = definition().rewrite("query_deadline_ms = 30000", "query_deadline_ms = 50");
        let (hook, control) = analyte::runtime::test_hooks::Committed::new();
        let (_temp, path, reference, running, mut peer) = setup_observed(
            vec![order("order-1", "000Ab12", None, 1)?],
            &settings,
            false,
            30,
            Some(hook),
        )
        .await?;
        let bytes = analyte_framing::hl7::encode(
            query(form, "late").as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?,
        )?;
        peer.write_all(bytes.get(..12).ok_or("prefix")?).await?;
        control.reached().await?;
        // Deliberately exceed the configured 50 ms first-byte query budget.
        tokio::time::sleep(Duration::from_millis(100)).await;
        control.resume();
        peer.write_all(bytes.get(12..).ok_or("suffix")?).await?;
        let rsp = packet(&mut peer).await?;
        assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
        assert_eq!(count(&path, "receipts")?, 1);
        assert_eq!(count(&path, "machine_orders")?, 0);
        quiet(&mut peer).await?;
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// IHE PaLM TF-2b §3.28
#[tokio::test]
async fn failed_query_followup_is_uncertain() -> Result<(), Error> {
    for action in ["timeout", "disconnect", "wrong-correlation"] {
        eprintln!("LAW followup action={action}");
        let settings = if action == "timeout" {
            definition().rewrite("response_ms = 30000", "response_ms = 200")
        } else {
            definition()
        };
        let (temp, path, reference, running, mut peer) = step(
            "setup",
            setup(
                vec![order("order-1", "000Ab12", None, 2)?],
                &settings,
                false,
            ),
        )
        .await?;
        step(
            "send",
            send(&mut peer, query(CONTAINER, "followup").as_bytes()),
        )
        .await?;
        step("packet", packet(&mut peer)).await?;
        let work = step("packet", packet(&mut peer)).await?;
        if action == "wrong-correlation" {
            step("orl", orl(&mut peer, &work, &["OK", "OK"], true)).await?;
        }
        if action == "disconnect" {
            drop(peer);
        } else {
            step("quiet", quiet(&mut peer)).await?;
        }
        step("vendor::state", vendor::state(&path, "Uncertain")).await?;
        step("wait_state", wait_state(&path, "done", 1)).await?;
        assert_eq!(count(&path, "machine_order_items")?, 2);
        assert_eq!(count(&path, "outbox")?, 1);
        step("running.stop", running.stop()).await?;
        if action == "timeout" {
            // The 200 ms budget exists only to force the first timeout. The restarted
            // service expects a timely negative ORL, so it gets the standard budget;
            // keeping 200 ms made the audit depend on CI scheduling (run 44148059968).
            std::fs::write(temp.path().join("hl7.toml"), definition())?;
        }
        let running = step(
            "Running::start",
            Running::start(&path, &reference.url, "synthetic-api"),
        )
        .await?;
        let mut peer = step("connect", connect(&path, "law")).await?;
        step(
            "send",
            send(&mut peer, query(CONTAINER, "after-failure").as_bytes()),
        )
        .await?;
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
        let negative = step("packet", packet(&mut peer)).await?;
        assert!(!negative.segments().any(|s| s.id() == b"OBR"));
        // Deliberately answer later than the failure-part 200 ms budget, so a
        // restart that kept that budget fails here every time, not under load only.
        tokio::time::sleep(Duration::from_millis(400)).await;
        step("orl", orl(&mut peer, &negative, &[], false)).await?;
        step("wait_audit", wait_audit(&path)).await?;
        assert_eq!(count(&path, "machine_order_items")?, 2);
        assert_eq!(count(&path, "machine_orders")?, 1);
        step("quiet", quiet(&mut peer)).await?;
        step("running.stop", running.stop()).await?;
        step("reference.close", reference.close()).await?;
    }
    Ok(())
}

// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn unreachable_app_never_answers_no_work() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = vendor_config(temp.path(), "law", &definition())?;
    let receiver_path = temp.path().join("receiver.db");
    let reference = Reference::start(
        &receiver_path,
        TcpListener::bind("127.0.0.1:0").await?,
        false,
    )
    .await?;
    let url = reference.url.clone();
    reference.close().await?;
    let running = Running::start(&path, &url, "synthetic-api").await?;
    let mut peer = connect(&path, "law").await?;
    for form in [CONTAINER, ALL] {
        send(&mut peer, query(form, "unreachable").as_bytes()).await?;
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"AE");
        quiet(&mut peer).await?;
    }
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    running.stop().await?;
    assert_eq!(receiver_count(&receiver_path)?, 0);
    Ok(())
}

// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn stale_or_incomplete_cache_is_not_empty() -> Result<(), Error> {
    for change in [
        "UPDATE feed SET polled=0",
        "UPDATE feed SET trusted=0,complete=0",
        "DELETE FROM orders; DELETE FROM feed",
        "INSERT INTO audit(evidence) VALUES ('{\"action\":\"store-restore\"}')",
    ] {
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", None, 1)?],
            &definition(),
            false,
        )
        .await?;
        reference.close().await?;
        database(&path)?.execute_batch(change)?;
        for form in [CONTAINER, ALL] {
            send(&mut peer, query(form, "stale").as_bytes()).await?;
            assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"AE");
            quiet(&mut peer).await?;
        }
        assert_eq!(
            count(&path, "orders")?,
            if change.starts_with("DELETE") { 0 } else { 1 }
        );
        assert_eq!(count(&path, "machine_orders")?, 0);
        running.stop().await?;
    }
    Ok(())
}

// IHE PaLM TF-2b §3.27, §3.28
#[tokio::test]
async fn preparation_failure_sends_no_positive_answer() -> Result<(), Error> {
    for failure in ["later-unmapped-test", "later-reservation-storage"] {
        let settings = definition();
        let mut pending: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&order("order-1", "000Ab12", None, 2)?)?)?;
        if failure == "later-unmapped-test" {
            pending["tests"][1]["value"] = serde_json::json!("UNMAPPED");
        }
        let (_temp, path, reference, running, mut peer) = setup(
            vec![analyte_contract::decode(&serde_json::to_vec(&pending)?)?],
            &settings,
            false,
        )
        .await?;
        if failure == "later-reservation-storage" {
            database(&path)?.execute_batch("CREATE TRIGGER synthetic_reservation_failure BEFORE INSERT ON machine_order_items WHEN NEW.item_index=1 BEGIN SELECT RAISE(FAIL,'synthetic-reservation-failure'); END;")?;
        }
        send(&mut peer, query(CONTAINER, "failed").as_bytes()).await?;
        let rsp = packet(&mut peer).await?;
        assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
        assert_eq!(field(&rsp, b"MSA", 1)?, b"AE");
        quiet(&mut peer).await?;
        assert_eq!(count(&path, "machine_orders")?, 0);
        assert_eq!(count(&path, "machine_order_items")?, 0);
        assert_eq!(count(&path, "outbox")?, 0);
        assert!(count(&path, "parked")? > 0);
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// IHE PaLM TF-2b §2.2.4
#[tokio::test]
async fn query_work_limits_content_to_peer_support() -> Result<(), Error> {
    for selected in [false, true] {
        let settings = definition()
            .rewrite("specimen = false", &format!("specimen = {selected}"))
            .rewrite("priority = false", &format!("priority = {selected}"))
            .rewrite(
                "manager_repeat = false",
                &format!("manager_repeat = {selected}"),
            )
            .rewrite(
                "repeat_control = false",
                &format!("repeat_control = {selected}"),
            );
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", None, 1)?],
            &settings,
            false,
        )
        .await?;
        send(&mut peer, query(CONTAINER, "options").as_bytes()).await?;
        assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
        let work = packet(&mut peer).await?;
        assert_eq!(
            field(&work, b"SPM", 2)?,
            if selected {
                b"000Ab12&LAB".as_slice()
            } else {
                b"".as_slice()
            }
        );
        assert_eq!(
            work.segments().any(|segment| segment.id() == b"TQ1"),
            selected
        );
        if selected {
            assert_eq!(field(&work, b"TQ1", 9)?, b"R^^HL70485");
        }
        // PaLM TF-2b 2.2.4.8/9: peer support is not app authorization.
        for tcd in work.segments().filter(|s| s.id() == b"TCD") {
            for number in [6, 7, 8] {
                assert_eq!(
                    tcd.field(number)
                        .map_or(b"".as_slice(), analyte_hl7::Field::raw),
                    b"",
                    "unsolicited TCD-{number}"
                );
            }
        }
        for orc in work.segments().filter(|s| s.id() == b"ORC") {
            assert_eq!(
                orc.field(8).map_or(b"".as_slice(), analyte_hl7::Field::raw),
                b"",
                "invented parent order"
            );
        }
        orl(&mut peer, &work, &["OK"], false).await?;
        wait_state(&path, "done", 1).await?;
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

// IHE PaLM TF-2b §3.27, §3.28
#[tokio::test]
async fn bad_negative_followup_orl_changes_no_orders() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(vec![], &definition(), false).await?;
    send(&mut peer, query(ALL, "negative").as_bytes()).await?;
    packet(&mut peer).await?;
    let negative = packet(&mut peer).await?;
    let raw = orl(&mut peer, &negative, &[], true).await?;
    wait_audit(&path).await?;
    let reason: String = database(&path)?.query_row("SELECT json_extract(evidence,'$.reason') FROM audit WHERE json_extract(evidence,'$.action')='LAW-negative-query-ORL'", [], |row| row.get(0))?;
    assert_eq!(
        reason,
        "negative followup ORL correlated=false acceptance=invalid"
    );
    let (orl_id, stored): (String, Vec<u8>) =
        database(&path)?.query_row("SELECT id,raw FROM receipts WHERE raw=?1", [&raw], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    assert_eq!(stored, raw);
    let query_id: String =
        database(&path)?.query_row("SELECT id FROM receipts WHERE id<>?1", [&orl_id], |r| {
            r.get(0)
        })?;
    let audit: String = database(&path)?.query_row("SELECT evidence FROM audit WHERE json_extract(evidence,'$.action')='LAW-negative-query-ORL'", [], |r| r.get(0))?;
    let audit: serde_json::Value = serde_json::from_str(&audit)?;
    assert_eq!(audit["actor"], "law");
    assert_eq!(
        audit["targets"],
        serde_json::json!([
            serde_json::from_str::<String>(&orl_id)?,
            serde_json::from_str::<String>(&query_id)?
        ])
    );
    assert_eq!(count(&path, "receipts")?, 2);
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    assert!(count(&path, "parked")? > 0);
    quiet(&mut peer).await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// IHE PaLM TF-2b §3.28
#[tokio::test]
async fn late_negative_followup_orl_is_parked() -> Result<(), Error> {
    let settings = definition().rewrite("response_ms = 30000", "response_ms = 200");
    let (_temp, path, reference, running, mut peer) = setup(vec![], &settings, false).await?;
    send(&mut peer, query(ALL, "late-negative").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    let negative = packet(&mut peer).await?;
    assert!(!negative.segments().any(|s| s.id() == b"OBR"));
    // The 200 ms budget starts when the OML is issued, before this side reads it;
    // waiting a full second puts the ORL past it on any scheduler.
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let raw = orl(&mut peer, &negative, &[], false).await?;
    wait_database_count(
        &path,
        "SELECT count(*) FROM parked p JOIN receipts r ON EXISTS(SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=json_extract(r.id,'$')) WHERE r.raw=?1",
        [&raw],
        1,
    )
    .await?;
    // The session tick may close the expired exchange first; either way the late
    // ORL is kept and shown to an operator, never recorded as a correlated reply.
    let reason: String = database(&path)?.query_row("SELECT json_extract(p.evidence,'$.reason') FROM parked p JOIN receipts r ON EXISTS(SELECT 1 FROM json_each(p.evidence,'$.targets') t WHERE t.value=json_extract(r.id,'$')) WHERE r.raw=?1", [&raw], |r| r.get(0))?;
    assert!(
        [
            "LAW ORL arrived after response deadline",
            "LAW ORL has no pending followup"
        ]
        .contains(&reason.as_str()),
        "{reason}"
    );
    assert_eq!(count(&path, "receipts")?, 2);
    assert_eq!(
        database(&path)?.query_row(
            "SELECT count(*) FROM audit WHERE json_extract(evidence,'$.action')='LAW-negative-query-ORL'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    quiet(&mut peer).await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// IHE PaLM TF-2b §3.27.4.1.2.3
#[tokio::test]
async fn malformed_query_tuple_gets_located_error() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", None, 1)?],
        &definition(),
        false,
    )
    .await?;
    // Informational rack is refused under the documented 60-character field ceiling;
    // shortening its fixed description cannot turn it into a supported query tuple.
    for (form, position) in [
        ("WOS_BY_RACK^Work Order Step by rack^IHELAW|tag||rack|01", 1),
        ("WOS_INFORMATIONAL_RACK^Query^IHELAW|tag|000Ab12|rack|2", 1),
        (
            "WOS_INFORMATIONAL_RACK^Query by WOS and Informational Rack^IHELAW|tag|000Ab12|rack|2",
            1,
        ),
        (
            "WOS_BY_RACK^Work Order Step by Rack^IHELAW|tag||rack|01|||room^Bench",
            8,
        ),
    ] {
        let bytes = query(form, "malformed");
        let request = analyte_hl7::law::parse_message(bytes.as_bytes(), options())?;
        assert!(
            matches!(analyte_hl7::law::Query::new(&request), Err(analyte_hl7::law::Error::Content {segment:"QPD",field}) if field == position)
        );
        send(&mut peer, bytes.as_bytes()).await?;
        let ack = packet(&mut peer).await?;
        assert_eq!(field(&ack, b"MSA", 1)?, b"AE");
        assert_eq!(
            field(&ack, b"ERR", 2)?,
            format!("QPD^1^{position}").as_bytes()
        );
        assert!(
            !ack.segments()
                .any(|s| matches!(s.id(), b"QAK" | b"SPM" | b"SAC" | b"ORC"))
        );
        quiet(&mut peer).await?;
    }
    assert_eq!(count(&path, "machine_orders")?, 0);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}
