//! Tests that startup recovery leaves answered LAW queries and work responses alone.
use super::*;

/// Interpretation versions retained for one receipt, with the failure reason of each.
fn interpretations(path: &Path, receipt: &str) -> Result<Vec<Option<String>>, Error> {
    Ok(database(path)?
        .prepare(
            "SELECT json_extract(evidence,'$.outcome.Failed') FROM interpretations WHERE receipt=?1 ORDER BY version",
        )?
        .query_map([receipt], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

fn receipts(path: &Path) -> Result<Vec<String>, Error> {
    Ok(database(path)?
        .prepare("SELECT id FROM receipts ORDER BY rowid")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

// TF-2b 3.27/3.28; ADR 0003 rules 2 and 3; ADR 0016 recovery: an answered
// LAB-27 query and an answered LAB-28 ORL (positive and negative followup)
// are handled receipts. Restart must not re-read them as unprocessed results
// and park them, while a query stored but never answered is still recovered
// and parked with its specific MSH-21 reason.
// IHE PaLM TF-2b §3.27, §3.28
#[tokio::test]
async fn restart_never_reparks_answered_query_or_orl() -> Result<(), Error> {
    let (_temp, path, reference, running, mut peer) = setup(
        vec![order("order-1", "000Ab12", None, 2)?],
        &definition(),
        false,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "found").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    let work = packet(&mut peer).await?;
    assert!(work.segments().any(|s| s.id() == b"OBR"));
    orl(&mut peer, &work, &["OK", "OK"], false).await?;
    vendor::state(&path, "Accepted").await?;
    // The order is reserved, so the repeated query is answered with negative work.
    send(&mut peer, query(CONTAINER, "negative").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    let negative = packet(&mut peer).await?;
    assert!(!negative.segments().any(|s| s.id() == b"OBR"));
    orl(&mut peer, &negative, &[], false).await?;
    wait_audit(&path).await?;
    // The session handles one message at a time: this RSP proves the negative ORL
    // was fully handled. Its own negative followup is deliberately left unanswered.
    send(&mut peer, query(CONTAINER, "probe").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    packet(&mut peer).await?;
    quiet(&mut peer).await?;
    running.stop().await?;
    drop(peer);
    let answered = receipts(&path)?;
    assert_eq!(answered.len(), 5);
    assert_eq!(count(&path, "parked")?, 0);
    // Crash after raw custody but before the answer: the query stays pending.
    let unanswered = work::seed_pending_raw(
        &path,
        &analyte_framing::hl7::encode(
            query(CONTAINER, "unanswered").as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?,
        )?,
    )?;
    let unanswered = serde_json::to_string(&unanswered)?;
    let running = Running::start(&path, &reference.url, "synthetic-api").await?;
    // Recovery finishes before the listener opens.
    let peer = connect(&path, "law").await?;
    for receipt in &answered {
        assert_eq!(
            interpretations(&path, receipt)?,
            [None],
            "answered receipt {receipt} must keep one handled interpretation"
        );
    }
    let recovered = interpretations(&path, &unanswered)?;
    assert_eq!(recovered.len(), 1);
    let reason = recovered
        .first()
        .cloned()
        .flatten()
        .ok_or("unanswered query must be parked")?;
    assert!(reason.contains("MSH-21"), "{reason}");
    let parked: Vec<String> = database(&path)?
        .prepare("SELECT json_extract(evidence,'$.targets[0]') FROM parked")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(parked, [serde_json::from_str::<String>(&unanswered)?]);
    assert_eq!(count(&path, "machine_orders")?, 1);
    drop(peer);
    running.stop().await?;
    reference.close().await?;
    Ok(())
}
