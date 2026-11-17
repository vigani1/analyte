//! Tests vendor receipt custody.
//! Kept whole: vendor custody conversations share the same program fixture.
use super::*;

#[tokio::test]
async fn vidas_literal_notes3_adr0003_stored_before_ack_unresolved_parked_no_delivery()
-> Result<(), Error> {
    let mut run = Running::start(include_str!(
        "../../../core/config/examples/vidas_literal.toml"
    ))
    .await?;
    run.peer.write_all(&[5]).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    let packet = vidas_config()?
        .encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn<0.1|y3mg/L|qd1:10|")?;
    run.peer.write_all(&packet).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    run.stored(&packet, "analyte-vidas")?;
    run.parked("VIDAS Literal lacks trusted result/run identity")
        .await?;
    run.result_value("<0.1")?;
    run.dilution_original(Some("1:10"))?;
    // Literal's final controls may share one read, after custody of the packet.
    run.peer.write_all(&[3, 4]).await?;
    run.close().await
}

#[tokio::test]
async fn poct01_b3_2_adr0003_stored_before_ack_unresolved_parked_no_delivery() -> Result<(), Error>
{
    let mut run = Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
    let hello = poct_peer::hello("hello");
    run.peer.write_all(hello.as_bytes()).await?;
    assert!(String::from_utf8(xml(&mut run.peer).await?)?.contains("ACK.type_cd V=\"AA\""));
    run.stored(hello.as_bytes(), "analyte-poct")?;
    run.peer
        .write_all(poct_peer::status("status", "1").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    xml(&mut run.peer).await?;
    let observation =
        poct_peer::observation("observation", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>");
    // Split XML deliberately, independent of transport read boundaries.
    let middle = observation.len() / 2;
    run.peer
        .write_all(observation.get(..middle).ok_or("split")?.as_bytes())
        .await?;
    run.peer
        .write_all(observation.get(middle..).ok_or("split")?.as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    run.stored(observation.as_bytes(), "analyte-poct")?;
    run.parked("POCT service sequence may wrap").await?;
    run.result_value("12.30")?;
    run.dilution_original(None)?;
    run.close().await
}

#[tokio::test]
async fn autolyser_rev18_p175_179_adr0003_corrupt_stored_blocks_r_until_valid_l()
-> Result<(), Error> {
    let mut run =
        Running::start(include_str!("../../../core/config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    let good = report()?;
    let mut corrupt = good.clone();
    *corrupt.get_mut(0).ok_or("fixture")? = b'9';
    run.peer.write_all(&corrupt).await?;
    run.parked("checksum mismatch").await?;
    run.stored(&corrupt, "analyte-autolyser")?;
    poll(&mut run.peer, b'L').await?;
    run.peer.write_all(&[21]).await?;
    // An empty L must keep recovery active; it never permits the next R.
    poll(&mut run.peer, b'L').await?;
    run.peer.write_all(&good).await?;
    run.parked("Autolyser Rev18 has no immutable run identity")
        .await?;
    run.stored(&good, "analyte-autolyser")?;
    run.result_value("<0.1   ")?;
    run.dilution_original(None)?;
    poll(&mut run.peer, b'R').await?;
    run.peer.write_all(&[21]).await?;
    run.close().await
}

#[tokio::test]
async fn adr0003_vendor_storage_failure_never_acknowledges_data() -> Result<(), Error> {
    for definition in [
        include_str!("../../../core/config/examples/vidas_literal.toml"),
        include_str!("../../../core/config/examples/poct.toml"),
        include_str!("../../../core/config/examples/autolyser.toml"),
    ] {
        let mut run = Running::start(definition).await?;
        let raw = if definition.contains("[vidas]") {
            run.peer.write_all(&[5]).await?;
            assert_eq!(byte(&mut run.peer).await?, 6);
            vidas_config()?.encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn12.3|")?
        } else if definition.contains("[autolyser]") {
            poll(&mut run.peer, b'R').await?;
            report()?
        } else {
            // Verify service startup before withdrawing storage capacity.
            let hello = poct_peer::hello("hello");
            run.peer.write_all(hello.as_bytes()).await?;
            xml(&mut run.peer).await?;
            poct_peer::status("status", "1").into_bytes()
        };
        run.space.store(0, Ordering::Release);
        run.peer.write_all(&raw).await?;
        let mut reply = [0u8; 256];
        let received = tokio::time::timeout(HANG_GUARD, run.peer.read(&mut reply)).await?;
        match received {
            Ok(count) => assert_eq!(count, 0, "storage failure must close without acceptance"),
            Err(error) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            )),
        }
        run.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn autolyser_rev18_p175_adr0003_uncertain_r_blocks_restart_until_operator_reconciliation()
-> Result<(), Error> {
    let mut run =
        Running::start(include_str!("../../../core/config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    // No response: neither timeout nor disconnect proves that R did not advance.
    let mut run = run.restart().await?;
    let mut bytes = [0u8; 1];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, run.peer.read(&mut bytes)).await??,
        0
    );
    run.parked("Autolyser R may advance its cursor").await?;
    run.close().await
}

#[tokio::test]
async fn vidas_literal_rule2_unknown_test_is_unmapped_and_operator_visible() -> Result<(), Error> {
    let mut run = Running::start(include_str!(
        "../../../core/config/examples/vidas_literal.toml"
    ))
    .await?;
    run.peer.write_all(&[5]).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    let packet = vidas_config()?.encode(b"mtrsl|pi|si|ci00012a|rtMISS|tt|td|qlpositive|qn<0.1|")?;
    run.peer.write_all(&packet).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    run.parked("UnknownTest").await?;
    run.stored(&packet, "analyte-vidas")?;
    run.no_results()?;
    run.peer.write_all(&[3, 4]).await?;
    run.close().await
}

#[tokio::test]
async fn autolyser_rev18_rule2_unknown_test_is_unmapped_and_operator_visible() -> Result<(), Error>
{
    use analyte_autolyser::syntax::{Field, Report};
    let mut run =
        Running::start(include_str!("../../../core/config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    let mut report = Report::parse(analyte_framing::autolyser::verified(&report()?)?, false)?;
    report.values.first_mut().ok_or("fixture value")?.code = Field::new(b"MISS")?;
    let raw = Report::new(
        &report.patient,
        report.list,
        report.sample,
        None,
        &report.values,
    )?
    .raw;
    let raw = analyte_framing::autolyser::finish(raw);
    run.peer.write_all(&raw).await?;
    run.parked("UnknownTest").await?;
    run.stored(&raw, "analyte-autolyser")?;
    run.no_results()?;
    run.close().await
}

#[tokio::test]
async fn poct01_b6_rule2_unknown_test_is_unmapped_and_operator_visible() -> Result<(), Error> {
    let mut run = Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
    run.peer
        .write_all(poct_peer::hello("hello").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    run.peer
        .write_all(poct_peer::status("status", "1").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    xml(&mut run.peer).await?;
    let raw = poct_peer::observation("unknown", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>")
        .replace("synthetic-analyte", "unknown-analyte");
    run.peer.write_all(raw.as_bytes()).await?;
    xml(&mut run.peer).await?;
    run.parked("UnknownTest").await?;
    run.stored(raw.as_bytes(), "analyte-poct")?;
    run.no_results()?;
    run.close().await
}

// AGENTS.md safety rule 2; VIDAS BCI-A chapter 3: interrupted transfers require reconciliation.
#[tokio::test]
async fn adr0003_rule2_vidas_disconnect_executes_park_action() -> Result<(), Error> {
    let mut run = Running::start(include_str!(
        "../../../core/config/examples/vidas_literal.toml"
    ))
    .await?;
    run.peer.write_all(&[5]).await?;
    assert_eq!(byte(&mut run.peer).await?, 6);
    run.peer.shutdown().await?;
    run.parked(
        "VIDAS transfer incomplete or refused; retained transport journal requires reconciliation",
    )
    .await?;
    run.close().await
}

// AGENTS.md safety rule 2; POCT01-A2 dialog abnormal closure is a retained operator outcome.
#[tokio::test]
async fn adr0003_rule2_poct_disconnect_executes_close_action() -> Result<(), Error> {
    let mut run = Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
    run.peer
        .write_all(poct_peer::hello("hello").as_bytes())
        .await?;
    xml(&mut run.peer).await?;
    run.peer.shutdown().await?;
    run.parked("POCT dialog closure: normal=false; unresolved custody tickets: []")
        .await?;
    run.close().await
}

// AGENTS.md safety rules 1/2; Autolyser Rev18 storage failure must park, never acknowledge.
#[tokio::test]
async fn adr0003_rule2_autolyser_storage_failure_executes_park_action() -> Result<(), Error> {
    let mut run =
        Running::start(include_str!("../../../core/config/examples/autolyser.toml")).await?;
    poll(&mut run.peer, b'R').await?;
    let raw = report()?;
    let encoded = raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
    // Refuse the assembled report only; transport fragments and the parking write remain writable.
    run.database()?.execute_batch(&format!(
        "CREATE TRIGGER refuse_report BEFORE INSERT ON receipts WHEN NEW.raw=X'{encoded}' BEGIN SELECT RAISE(ABORT, 'synthetic report storage failure'); END;"
    ))?;
    let middle = raw.len() / 2;
    let first = raw.get(..middle).ok_or("report split")?;
    let baseline: i64 = run.database()?.query_row(
        "SELECT coalesce(sum(length(raw)),0) FROM receipts",
        [],
        |r| r.get(0),
    )?;
    run.peer.write_all(first).await?;
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let bytes: i64 = run.database()?.query_row(
                "SELECT coalesce(sum(length(raw)),0) FROM receipts",
                [],
                |r| r.get(0),
            )?;
            if bytes >= baseline + i64::try_from(first.len())? {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Ok::<_, Error>(())
    })
    .await??;
    run.peer
        .write_all(raw.get(middle..).ok_or("report split")?)
        .await?;
    run.parked("Autolyser: durable storage unavailable").await?;
    let mut reply = [0u8; 8];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, run.peer.read(&mut reply)).await??,
        0
    );
    run.close().await
}

// CLSI POCT01-A2 Appendix B section 5.7, p.96: the advertised capacity bounds host replies.
#[tokio::test]
async fn poct01_b5_7_host_never_sends_beyond_device_capacity() -> Result<(), Error> {
    let mut run = Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
    let hello = poct_peer::hello("hello").replace(
        "<DSC.max_message_sz NULL=\"PINF\"/>",
        "<DSC.max_message_sz V=\"1\"/>",
    );
    run.peer.write_all(hello.as_bytes()).await?;
    let mut response = [0u8; 1];
    let received = tokio::time::timeout(HANG_GUARD, run.peer.read(&mut response)).await??;
    assert_eq!(
        received, 0,
        "an unfit reply must fail without sending or truncating it"
    );
    run.stored(hello.as_bytes(), "analyte-poct")?;
    run.close().await
}

#[tokio::test]
async fn poct01_b3_2_program_lost_ack_reconnect_restart_and_correction() -> Result<(), Error> {
    let definition = include_str!("../../../core/config/examples/poct.toml").replace(
        "[poct]",
        "[poct]\nidentity = { sequence_number = true, processing = \"Production\" }",
    );
    let mut run = Running::start(&definition).await?;
    let hello = poct_peer::hello("hello");
    poct_start(&mut run, &hello).await?;
    let original = poct_peer::observation("first", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>")
        .replace("</PT>", "</PT><SPC><SPC.specimen_id V=\"000xyz\"/><SPC.specimen_dttm NULL=\"UNK\"/><SPC.source_cd NULL=\"UNK\"/><SPC.type_cd NULL=\"UNK\"/></SPC>");
    run.peer.write_all(original.as_bytes()).await?;
    // The peer deliberately does not consume the first ACK before resending.
    poct_deliveries(&run, 1).await?;
    let resent = original.replace("first", "resent").replace(
        "<SVC.sequence_nbr",
        "<SVC.reason_cd V=\"RES\"/><SVC.sequence_nbr",
    );
    run.peer.write_all(resent.as_bytes()).await?;
    xml(&mut run.peer).await?;
    xml(&mut run.peer).await?;
    for restart in [false, true] {
        if restart {
            run = run.restart().await?;
        } else {
            run.peer = TcpStream::connect(run.listener.local_addr()?).await?;
        }
        poct_start(&mut run, &hello).await?;
        run.peer.write_all(resent.as_bytes()).await?;
        xml(&mut run.peer).await?;
    }
    assert_eq!(
        run.database()?
            .query_row("SELECT count(*) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        run.database()?.query_row(
            "SELECT count(*) FROM parked WHERE evidence LIKE '%conflict%'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0,
        "an unchanged resend must be a retransmission, not a parked conflict"
    );
    let edited = resent
        .replace("12.30", "13.30")
        .replace("RES", "EDT")
        .replace("resent", "edited");
    run.peer.write_all(edited.as_bytes()).await?;
    xml(&mut run.peer).await?;
    poct_deliveries(&run, 2).await?;
    assert_eq!(
        run.database()?
            .query_row("SELECT max(revision) FROM observation_revisions", [], |r| r
                .get::<_, i64>(0))?,
        1
    );
    // POCT01-A2 B 3.2 and 5.21; clinical rule 4: compare delivered meanings.
    let receiver = rusqlite::Connection::open(run.directory.path().join("reference.db"))?;
    let mut statement = receiver.prepare("SELECT body FROM deliveries ORDER BY rowid")?;
    let bodies = statement
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(bodies.len(), 2);
    let first: serde_json::Value =
        serde_json::from_slice(bodies.first().ok_or("missing original")?)?;
    let corrected: serde_json::Value =
        serde_json::from_slice(bodies.get(1).ok_or("missing correction")?)?;
    assert_ne!(first["delivery_id"], corrected["delivery_id"]);
    assert_ne!(first["receipt"]["id"], corrected["receipt"]["id"]);
    for (body, revision, value) in [(&first, 0, "12.30"), (&corrected, 1, "13.30")] {
        assert_eq!(body["results"].as_array().ok_or("results array")?.len(), 1);
        assert_eq!(
            body["results"][0]["identity"]["resolved"]["revision"],
            revision
        );
        assert_eq!(
            body["results"][0]["parts"][0]["values"][0]["original"],
            value
        );
    }
    let identity = &first["results"][0]["identity"]["resolved"]["observation"];
    assert!(identity.is_object());
    assert_eq!(
        identity,
        &corrected["results"][0]["identity"]["resolved"]["observation"]
    );
    assert_missing_identity_and_conflict_retained(&mut run, &resent, &original).await?;
    assert_missing_device_retained(&mut run, &hello).await?;
    run.close_with_deliveries(2).await
}

#[tokio::test]
async fn poct01_b4_1_2_program_rejected_hello_ack_then_terminate_keeps_raw() -> Result<(), Error> {
    let base = poct_peer::hello("bad-hello");
    for (raw, code, reason) in [
        (
            base.replace(
                "DSC.connection_profile_cd V=\"SA\"",
                "DSC.connection_profile_cd V=\"invalid\"",
            ),
            "103",
            "POCT syntax",
        ),
        (
            base.replace("<DEV.device_id V=\"A1-B2-C3-D4-E5-F6-07-08\"/>", ""),
            "101",
            "POCT syntax",
        ),
        (base.replace("</DEV>", "</BROKEN>"), "102", "POCT syntax"),
        (
            base.replace(
                "<DSC.max_message_sz NULL=\"PINF\"/>",
                "<DSC.max_message_sz V=\"0\"/>",
            ),
            "200",
            "POCT Hello capabilities",
        ),
        (
            base.replace("HDR.version_id V=\"POCT01\"", "HDR.version_id V=\"POCT02\""),
            "201",
            "POCT syntax",
        ),
    ] {
        let mut run =
            Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
        run.peer.write_all(raw.as_bytes()).await?;
        let ack = xml(&mut run.peer).await?;
        let ack = analyte_poct::Message::parse(&ack, poct_peer::limits())?;
        assert!(
            matches!(ack.body(), analyte_poct::Body::Acknowledgement { kind: analyte_poct::AckKind::Error, reference } if reference.as_str() == "bad-hello")
        );
        assert!(
            String::from_utf8(ack.document().raw().to_vec())?
                .contains(&format!("ACK.error_detail_cd V=\"{code}\""))
        );
        // Both retention and parking have completed before the error response.
        run.stored(raw.as_bytes(), "analyte-poct")?;
        run.parked(reason).await?;
        let end = xml(&mut run.peer).await?;
        let end = analyte_poct::Message::parse(&end, poct_peer::limits())?;
        assert!(
            matches!(end.body(), analyte_poct::Body::Terminate(code) if matches!(code, analyte_poct::Nullable::Value(code) if code.value == "ABN"))
        );
        let final_ack = poct_peer::message(
            "ACK.R01",
            "end-ack",
            &format!(
                "<ACK><ACK.type_cd V=\"AA\"/>{}</ACK>",
                poct_peer::field("ACK.ack_control_id", end.header().control_id.as_str())
            ),
        );
        run.peer.write_all(final_ack.as_bytes()).await?;
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(HANG_GUARD, run.peer.read(&mut byte)).await??,
            0
        );
        run.close().await?;
    }
    Ok(())
}

// POCT01-A2 B 4.1.2 p.78; AGENTS.md rule 4: never guess a reply reference.
#[tokio::test]
async fn poct01_b4_1_2_program_ambiguous_hello_header_is_parked_without_guessed_ack()
-> Result<(), Error> {
    let mut run = Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
    let raw = poct_peer::hello("one").replace(
        "<HDR.control_id V=\"one\"/>",
        "<HDR.control_id V=\"one\"/><HDR.control_id V=\"two\"/>",
    );
    run.peer.write_all(raw.as_bytes()).await?;
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, run.peer.read(&mut byte)).await??,
        0
    );
    run.stored(raw.as_bytes(), "analyte-poct")?;
    run.parked("POCT syntax").await?;
    run.close().await
}

// POCT01-A2 B 5.13 p.99 and 3.4.2 p.74: reused IDs are protocol faults, not new results.
#[tokio::test]
async fn poct01_b5_13_program_reused_control_id_parks_evidence_and_escapes() -> Result<(), Error> {
    for reused in ["observation", "status"] {
        let mut run =
            Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
        poct_start(&mut run, &poct_peer::hello("hello")).await?;
        let first =
            poct_peer::observation("observation", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>");
        run.peer.write_all(first.as_bytes()).await?;
        xml(&mut run.peer).await?;
        let repeated = poct_peer::observation(reused, true, "<OBS.value V=\"99.30\" U=\"mg/L\"/>");
        run.peer.write_all(repeated.as_bytes()).await?;
        let escape = xml(&mut run.peer).await?;
        let escape = analyte_poct::Message::parse(&escape, poct_peer::limits())?;
        assert!(
            matches!(escape.body(), analyte_poct::Body::Escape { reference, .. } if reference.as_str() == reused)
        );
        run.stored(repeated.as_bytes(), "analyte-poct")?;
        run.parked("POCT reused incoming control ID").await?;
        run.result_value("12.30")?;
        let end = xml(&mut run.peer).await?;
        let end = analyte_poct::Message::parse(&end, poct_peer::limits())?;
        assert!(matches!(end.body(), analyte_poct::Body::Terminate(_)));
        let ack = poct_peer::message(
            "ACK.R01",
            "unique-end-ack",
            &format!(
                "<ACK><ACK.type_cd V=\"AA\"/>{}</ACK>",
                poct_peer::field("ACK.ack_control_id", end.header().control_id.as_str())
            ),
        );
        run.peer.write_all(ack.as_bytes()).await?;
        run.close().await?;
    }
    Ok(())
}

// POCT01-A2 Appendix B 3.5, 5.7, 5.12 and 6.9; synthetic peer bytes.
#[tokio::test]
async fn poct01_b_3_5_unsupported_keep_alive_gets_correlated_escape() -> Result<(), Error> {
    for (host, device) in [(2048, 1024), (1024, 2048)] {
        let definition = include_str!("../../../core/config/examples/poct.toml")
            .replace("bytes = 1048576", &format!("bytes = {host}"));
        let mut run = Running::start(&definition).await?;
        let hello = poct_peer::hello("hello").replace(
            "<DSC.max_message_sz NULL=\"PINF\"/>",
            &format!("<DSC.max_message_sz V=\"{device}\"/>"),
        );
        run.peer.write_all(hello.as_bytes()).await?;
        let ack = xml(&mut run.peer).await?;
        assert!(ack.len() <= host.min(device));
        assert_accept(&ack, "hello")?;
        run.peer
            .write_all(poct_peer::status("status", "1").as_bytes())
            .await?;
        let ack = xml(&mut run.peer).await?;
        assert!(ack.len() <= host.min(device));
        assert_accept(&ack, "status")?;
        let request = xml(&mut run.peer).await?;
        assert!(request.len() <= host.min(device));
        assert!(
            matches!(analyte_poct::Message::parse(&request, poct_peer::limits())?.body(), analyte_poct::Body::Request(analyte_poct::Nullable::Value(code)) if code.value == "ROBS")
        );
        let observation =
            poct_peer::observation("obs", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>");
        run.peer.write_all(observation.as_bytes()).await?;
        let ack = xml(&mut run.peer).await?;
        assert!(ack.len() <= host.min(device));
        assert_accept(&ack, "obs")?;
        // A syntactically valid null analyte cannot be mapped; it gets a classified error ACK.
        let invalid = observation.replace("V=\"obs\"", "V=\"bad-map\"").replace(
            "<OBS.observation_id V=\"synthetic-analyte\" SN=\"synthetic-system\"><TRANSLTN V=\"alternate\" SN=\"alternate-system\"/></OBS.observation_id>",
            "<OBS.observation_id NULL=\"UNK\"/>",
        );
        run.peer.write_all(invalid.as_bytes()).await?;
        let error_ack = xml(&mut run.peer).await?;
        assert!(error_ack.len() <= host.min(device));
        assert!(
            matches!(analyte_poct::Message::parse(&error_ack, poct_peer::limits())?.body(), analyte_poct::Body::Acknowledgement { kind: analyte_poct::AckKind::Error, reference } if reference.as_str() == "bad-map")
        );
        let keep_alive = poct_peer::message("KPA.R01", "keep-alive", "");
        run.peer.write_all(keep_alive.as_bytes()).await?;
        let escape = xml(&mut run.peer).await?;
        assert!(escape.len() <= host.min(device));
        assert!(
            matches!(analyte_poct::Message::parse(&escape, poct_peer::limits())?.body(), analyte_poct::Body::Escape { reference, reason } if reference.as_str() == "keep-alive" && reason == "OTH")
        );
        run.stored(keep_alive.as_bytes(), "analyte-poct")?;
        let end = xml(&mut run.peer).await?;
        assert!(end.len() <= host.min(device));
        let parsed = analyte_poct::Message::parse(&end, poct_peer::limits())?;
        assert!(
            matches!(parsed.body(), analyte_poct::Body::Terminate(analyte_poct::Nullable::Value(code)) if code.value == "ABN")
        );
        let reply = poct_peer::message(
            "ACK.R01",
            "end-ack",
            &format!(
                "<ACK><ACK.type_cd V=\"AA\"/><ACK.ack_control_id V=\"{}\"/></ACK>",
                parsed.header().control_id.as_str()
            ),
        );
        run.peer.write_all(reply.as_bytes()).await?;
        assert_eq!(
            tokio::time::timeout(HANG_GUARD, run.peer.read(&mut [0])).await??,
            0
        );
        run.close().await?;
    }
    Ok(())
}

fn assert_accept(raw: &[u8], expected: &str) -> Result<(), Error> {
    assert!(
        matches!(analyte_poct::Message::parse(raw, poct_peer::limits())?.body(), analyte_poct::Body::Acknowledgement { kind: analyte_poct::AckKind::Accept, reference } if reference.as_str() == expected)
    );
    Ok(())
}

// POCT01-A2 Appendix B 5.7: a later error reply must also be refused whole.
#[tokio::test]
async fn poct01_b_5_7_later_error_ack_cannot_exceed_device_capacity() -> Result<(), Error> {
    let mut run = Running::start(include_str!("../../../core/config/examples/poct.toml")).await?;
    let hello = poct_peer::hello("hello").replace(
        "<DSC.max_message_sz NULL=\"PINF\"/>",
        "<DSC.max_message_sz V=\"300\"/>",
    );
    poct_start(&mut run, &hello).await?;
    let raw = poct_peer::observation(&"x".repeat(256), true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>").replace(
        "<OBS.observation_id V=\"synthetic-analyte\" SN=\"synthetic-system\"><TRANSLTN V=\"alternate\" SN=\"alternate-system\"/></OBS.observation_id>",
        "<OBS.observation_id NULL=\"UNK\"/>");
    run.peer.write_all(raw.as_bytes()).await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, run.peer.read(&mut [0])).await??,
        0
    );
    run.stored(raw.as_bytes(), "analyte-poct")?;
    run.close().await
}
