//! Tests LAW order queries and query-triggered work downloads.
use super::*;

#[tokio::test]
async fn law_tf2b_3_27_3_28_found_query_reserves_per_awos_and_finishes_each_orl_outcome()
-> Result<(), Error> {
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
            vendor_support::state(&path, "Sending").await?;
            assert_eq!(count(&path, "outbox")?, 0);
            let raw_orl = orl(&mut peer, &work, &codes, false).await?;
            vendor_support::state(&path, expected).await?;
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

#[tokio::test]
async fn law_tf2b_3_27_4_trusted_empty_uses_restricted_negative_and_correlated_orl_without_items()
-> Result<(), Error> {
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
        orl(&mut peer, &negative, &[], false).await?;
        wait_audit(&path).await?;
        assert_eq!(count(&path, "receipts")?, 2);
        assert_eq!(count(&path, "machine_orders")?, 0);
        assert_eq!(count(&path, "outbox")?, 0);
        quiet(&mut peer).await?;
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_27_4_all_pending_pages_filter_other_analyzers_and_keep_every_awos()
-> Result<(), Error> {
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

#[tokio::test]
async fn law_tf2b_3_27_b7_unsupported_forms_are_ar_never_negative_work() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", None, 1)?],
        &definition(),
        false,
    )
    .await?;
    for form in [
        "WOS_BY_RACK^Work Order Step by rack^IHELAW|tag||rack|01",
        "WOS_BY_TRAY^Work Order Step by tray^IHELAW|tag||||tray|1^2",
        "WOS_BY_ISOLATE^Work Order Step by isolate^IHELAW|tag|child||||||parent",
        "WOS_INFORMATIONAL_RACK^Query^IHELAW|tag|000Ab12|rack|2",
    ] {
        send(&mut peer, query(form, "unsupported").as_bytes()).await?;
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

#[tokio::test]
async fn law_tf2b_3_27_4_untrusted_read_failure_and_restore_pause_are_errors_not_no_work()
-> Result<(), Error> {
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

#[tokio::test]
async fn law_tf2b_3_27_4_late_query_budget_includes_first_byte_and_raw_storage() -> Result<(), Error>
{
    for form in [CONTAINER, ALL] {
        let settings = definition().replace("query_deadline_ms = 30000", "query_deadline_ms = 50");
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", None, 1)?],
            &settings,
            false,
        )
        .await?;
        let bytes = analyte_framing::hl7::encode(
            query(form, "late").as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?,
        )?;
        peer.write_all(bytes.get(..12).ok_or("prefix")?).await?;
        // Deliberately exceed the configured 50 ms first-byte query budget.
        tokio::time::sleep(Duration::from_millis(100)).await;
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

#[tokio::test]
async fn law_tf2b_3_28_query_followup_timeout_disconnect_and_bad_correlation_are_uncertain()
-> Result<(), Error> {
    for action in ["timeout", "disconnect", "wrong-correlation"] {
        let settings = if action == "timeout" {
            definition().replace("response_ms = 30000", "response_ms = 200")
        } else {
            definition()
        };
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", None, 2)?],
            &settings,
            false,
        )
        .await?;
        send(&mut peer, query(CONTAINER, "followup").as_bytes()).await?;
        packet(&mut peer).await?;
        let work = packet(&mut peer).await?;
        if action == "wrong-correlation" {
            orl(&mut peer, &work, &["OK", "OK"], true).await?;
        }
        if action == "disconnect" {
            drop(peer);
        } else {
            quiet(&mut peer).await?;
        }
        vendor_support::state(&path, "Uncertain").await?;
        wait_state(&path, "done", 1).await?;
        assert_eq!(count(&path, "machine_order_items")?, 2);
        assert_eq!(count(&path, "outbox")?, 1);
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_27_4_unreachable_app_without_snapshot_never_answers_no_work()
-> Result<(), Error> {
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

#[tokio::test]
async fn law_tf2b_3_27_4_stale_and_incomplete_cache_with_retained_orders_are_not_empty()
-> Result<(), Error> {
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

#[tokio::test]
async fn law_tf2b_3_27_3_28_preflight_and_reservation_failure_cannot_emit_positive_rsp_or_work()
-> Result<(), Error> {
    for failure in ["unmapped-test", "reservation-storage"] {
        let settings = if failure == "unmapped-test" {
            definition().replace("value=\"TEST\"", "value=\"OTHER\"")
        } else {
            definition()
        };
        let (_temp, path, reference, running, mut peer) = setup(
            vec![order("order-1", "000Ab12", None, 1)?],
            &settings,
            false,
        )
        .await?;
        if failure == "reservation-storage" {
            database(&path)?.execute_batch("CREATE TRIGGER synthetic_reservation_failure BEFORE INSERT ON machine_order_items BEGIN SELECT RAISE(FAIL,'synthetic-reservation-failure'); END;")?;
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

#[tokio::test]
async fn law_tf2b_2_2_4_query_work_intersects_supplied_specimen_priority_with_peer_support()
-> Result<(), Error> {
    for selected in [false, true] {
        let settings = definition()
            .replace("specimen = false", &format!("specimen = {selected}"))
            .replace("priority = false", &format!("priority = {selected}"));
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
        orl(&mut peer, &work, &["OK"], false).await?;
        wait_state(&path, "done", 1).await?;
        running.stop().await?;
        reference.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn law_tf2b_3_27_3_28_negative_followup_bad_orl_is_retained_without_order_effects()
-> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(vec![], &definition(), false).await?;
    send(&mut peer, query(ALL, "negative").as_bytes()).await?;
    packet(&mut peer).await?;
    let negative = packet(&mut peer).await?;
    orl(&mut peer, &negative, &[], true).await?;
    wait_audit(&path).await?;
    let reason: String = database(&path)?.query_row("SELECT json_extract(evidence,'$.reason') FROM audit WHERE json_extract(evidence,'$.action')='LAW-negative-query-ORL'", [], |row| row.get(0))?;
    assert!(reason.contains("correlated=false"));
    assert_eq!(count(&path, "receipts")?, 2);
    assert_eq!(count(&path, "machine_orders")?, 0);
    assert_eq!(count(&path, "outbox")?, 0);
    assert!(count(&path, "parked")? > 0);
    quiet(&mut peer).await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

#[cfg(feature = "test-hooks")]
#[tokio::test]
async fn law_tf2b_3_27_4_adr0003_raw_commit_precedes_query_response_and_reservation()
-> Result<(), Error> {
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
#[tokio::test]
async fn rule9_law_query_freshness_follows_destination_config() -> Result<(), Error> {
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
