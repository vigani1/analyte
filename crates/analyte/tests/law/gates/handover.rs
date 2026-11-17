//! Verifies independent LAW ownership, issuance and settlement across handover.
use super::*;

// PaLM TF-2b B.6 / ADR 0003: queue ownership and RSP issuance precede dial.
#[tokio::test]
async fn law_tf2b_b6_rsp_issuance_precedes_outbound_dial() -> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let (handover, owned) = Committed::new();
    let (rsp, issued) = Committed::new();
    let gates = LawGates {
        handover: Some(handover),
        rsp: Some(rsp),
        ..LawGates::default()
    };
    let (_temp, path, reference, running, mut peer) = start(
        &independent(listener.local_addr()?),
        vec![order("order-1", "000Ab12", None, 2)?],
        gates,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "handover").as_bytes()).await?;
    owned.reached().await?;
    assert_eq!(items(&path)?.len(), 2);
    quiet(&mut peer).await?;
    no_dial(&listener).await?;
    owned.resume();
    issued.reached().await?;
    assert_eq!(field(&packet(&mut peer).await?, b"MSA", 1)?, b"AA");
    no_dial(&listener).await?;
    issued.resume();
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let work = packet(&mut outbound).await?;
    orl(&mut outbound, &work, &["OK", "OK"], false).await?;
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, outbound.read(&mut [0])).await??,
        0
    );
    vendor::state(&path, "Accepted").await?;
    running.stop().await?;
    reference.close().await?;
    Ok(())
}

// PaLM TF-2b B.6 / ADR 0003 rule 8: every transferred AWOS keeps an owner.
#[tokio::test]
async fn law_tf2b_b6_shutdown_during_handover_settles_all_without_reissue() -> Result<(), Error> {
    transfer_failure(false).await
}

// PaLM TF-2b B.6 / 3.27.4: expiry of a queued followup cannot authorize a retry.
#[tokio::test]
async fn law_tf2b_b6_queued_followup_expiry_settles_all_without_reissue() -> Result<(), Error> {
    transfer_failure(true).await
}
async fn transfer_failure(expiry: bool) -> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let (handover, owned) = Committed::new();
    let (queued, dequeued) = Committed::new();
    let gates = LawGates {
        handover: Some(handover),
        queued: expiry.then_some(queued),
        expire_queued: expiry,
        ..LawGates::default()
    };
    let settings = independent(listener.local_addr()?);
    let (_temp, path, reference, running, mut peer) = start(
        &settings,
        vec![order("order-1", "000Ab12", None, 2)?],
        gates,
    )
    .await?;
    send(&mut peer, query(CONTAINER, "interrupted").as_bytes()).await?;
    owned.reached().await?;
    let before = items(&path)?;
    assert_eq!(before.len(), 2);
    assert!(before.iter().all(|(_, state)| state == "Sending"));
    no_dial(&listener).await?;
    if expiry {
        dequeued.reached().await?;
        dequeued.resume();
        uncertain(&path, &before).await?;
        let reason: String =
            database(&path)?
                .query_row("SELECT source_code FROM machine_orders", [], |r| r.get(0))?;
        assert_eq!(
            reason,
            "item-outcomes: 1=Uncertain:LAW-independent-queue-deadline; 2=Uncertain:LAW-independent-queue-deadline"
        );
        no_dial(&listener).await?;
        owned.resume();
        running.stop().await?;
    } else {
        running.stop().await?;
        uncertain(&path, &before).await?;
    }
    assert_no_reissue(&path, &reference, &listener, &before).await?;
    reference.close().await?;
    Ok(())
}

async fn assert_no_reissue(
    path: &Fixture,
    reference: &Reference,
    listener: &TcpListener,
    before: &[(String, String)],
) -> Result<(), Error> {
    let restarted = Running::start(path, &reference.url, "synthetic-api").await?;
    no_dial(listener).await?;
    let mut peer = connect(path, "law").await?;
    send(&mut peer, query(CONTAINER, "after-restart").as_bytes()).await?;
    assert_eq!(field(&packet(&mut peer).await?, b"QAK", 2)?, b"OK");
    let (mut outbound, _) = tokio::time::timeout(HANG_GUARD, listener.accept()).await??;
    let negative = packet(&mut outbound).await?;
    assert!(!negative.segments().any(|s| s.id() == b"OBR"));
    orl(&mut outbound, &negative, &[], false).await?;
    wait_audit(path).await?;
    uncertain(path, before).await?;
    assert_eq!(count(path, "machine_orders")?, 1);
    restarted.stop().await?;
    Ok(())
}
