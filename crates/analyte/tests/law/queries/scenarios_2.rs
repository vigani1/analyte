//! Tests LAW order queries and query-triggered work downloads.
use super::*;

#[tokio::test]
async fn law_tf2b_c6_2_x_awos_components_do_not_change_retained_work_match() -> Result<(), Error> {
    let (_temp, path, mut reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", Some("law"), 1)?],
        &definition().replace("specimen = false", "specimen = true"),
        false,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "validate-work").as_bytes()).await?;
    packet(&mut peer).await?;
    let work = packet(&mut peer).await?;
    orl(&mut peer, &work, &["OK"], false).await?;
    vendor_support::state(&path, "Accepted").await?;
    wait_state(&path, "done", 1).await?;
    let awos = std::str::from_utf8(field(&work, b"OBR", 2)?)?;
    let valid = law_results::result("valid", &format!("{awos}^LAB"), "", "12.30", "F")
        .replace(
            "sample-01&99LAB",
            std::str::from_utf8(field(&work, b"SPM", 2)?)?,
        )
        .replace(
            "00012a^LAB",
            &format!("{}^LAB", std::str::from_utf8(field(&work, b"SAC", 3)?)?),
        )
        .replace(
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
            valid.replace(&format!("{awos}^LAB"), "never-downloaded^LAB"),
            "OBR^1^2",
        ),
        (
            "specimen",
            valid.replace("000Ab12&", "different&"),
            "SPM^1^2",
        ),
        (
            "specimen-type",
            valid.replace("SER^Serum^HL70487", "PLAS^Plasma^HL70487"),
            "SPM^1^4",
        ),
        (
            "container",
            valid.replace("000Ab12^LAB", "different^LAB"),
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
        let bad = bad.replace("|valid|", &format!("|{id}|"));
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
    for suffix in ["", "^OTHER", "^OTHER^arbitrary^ignored"] {
        let accepted = valid.replace(&format!("{awos}^LAB"), &format!("{awos}{suffix}"));
        send(&mut peer, accepted.as_bytes()).await?;
        assert_eq!(field(&packet(&mut peer).await?, b"MSA", 1)?, b"AA");
    }
    send(&mut peer, valid.as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"MSA", 1)?, b"AA");
    reference.event().await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b Rev 11 B.7.1-B.7.2 pp.80-81; C.1 pp.84-85.
#[tokio::test]
async fn law_tf2b_b7_1_b7_2_host_correlates_errors_without_ack_loops() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(vec![], &definition(), false).await?;
    let query = query(CONTAINER, "correlated");
    let result = law_results::result("correlated", "\"\"", "run^ANALYZER", "1.0", "F");
    for (message, code) in [
        (query.replace("QPD|", "QPD|BROKEN"), b"AE".as_slice()),
        (query.replace("QBP^Q11^QBP_Q11", "QBP^Q99^QBP_Q11"), b"AR"),
        (query.replace("|2.5.1|", "|2.4|"), b"AR"),
        (query.replace("LAB-27^IHE", "OTHER^IHE"), b"AR"),
        (result.replace("OUL^R22^OUL_R22", "ORU^R01^ORU_R01"), b"AR"),
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
            .replace("QBP^Q11^QBP_Q11", kind)
            .replace("|2.5.1|", "|2.4|");
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
