//! Verifies LAW query custody, reservation and cancellation admission ordering.
use super::*;

// PaLM TF-2b 3.27.4 / 3.28.4; ADR 0003 rules 1 and 8.
// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn lookup_and_reservation_commit_precede_response() -> Result<(), Error> {
    let (lookup, looked) = Committed::new();
    let (reserved, admitted) = Committed::new();
    let gates = LawGates {
        lookup: Some(lookup),
        reserved: Some(reserved),
        ..LawGates::default()
    };
    let (_temp, path, reference, running, mut peer) = start(
        &definition(),
        vec![order("order-1", "000Ab12", None, 2)?],
        gates,
    )
    .await?;
    let request = query(CONTAINER, "ordered");
    send(&mut peer, request.as_bytes()).await?;
    looked.reached().await?;
    let raw: Vec<u8> = database(&path)?.query_row("SELECT raw FROM receipts", [], |r| r.get(0))?;
    assert_eq!(
        raw,
        analyte_framing::hl7::encode(
            request.as_bytes(),
            analyte_framing::hl7::Limit::new(1048576)?
        )?
    );
    assert!(items(&path)?.is_empty());
    quiet(&mut peer).await?;
    looked.resume();
    admitted.reached().await?;
    assert_eq!(
        items(&path)?
            .iter()
            .map(|(_, state)| state.as_str())
            .collect::<Vec<_>>(),
        ["Sending", "Sending"]
    );
    assert_eq!(count(&path, "machine_orders")?, 1);
    quiet(&mut peer).await?;
    admitted.resume();
    assert_eq!(field(&packet(&mut peer).await?, b"MSA", 1)?, b"AA");
    let work = packet(&mut peer).await?;
    orl(&mut peer, &work, &["OK", "OK"], false).await?;
    vendor::state(&path, "Accepted").await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b 3.28.4 / ADR 0004: candidate eligibility is not cancel permission.
// IHE PaLM TF-2b §3.28.4
#[tokio::test]
async fn cancel_rechecks_trust_after_candidate_selection() -> Result<(), Error> {
    let (cancel, admission) = Committed::new();
    let (cancel_result, checked) = Committed::new();
    let gates = LawGates {
        cancel: Some(cancel),
        cancel_result: Some(cancel_result),
        ..LawGates::default()
    };
    let settings = definition().rewrite("send_ahead = false", "send_ahead = true");
    let (_temp, path, reference, running, mut peer) = start(
        &settings,
        vec![order("order-1", "000Ab12", None, 2)?],
        gates,
    )
    .await?;
    let sent = packet(&mut peer).await?;
    orl(&mut peer, &sent, &["OK", "OK"], false).await?;
    vendor::state(&path, "Accepted").await?;
    let before = items(&path)?;
    work::cancel_cache(&path, &reference).await?;
    admission.reached().await?;
    reference.close().await?;
    database(&path)?.execute_batch("UPDATE feed SET trusted=0,complete=0")?;
    admission.resume();
    checked.reached().await?;
    quiet(&mut peer).await?;
    assert_eq!(items(&path)?, before);
    assert_eq!(database(&path)?.query_row("SELECT count(*) FROM machine_order_history WHERE json_extract(evidence,'$.state')='CancelRequested'", [], |r| r.get::<_, i64>(0))?, 0);
    checked.resume();
    running.stop().await?;
    Ok(())
}
