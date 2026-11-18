//! Verifies coherent all-pending output while a competing feed update is queued.
use super::*;
use std::{future::Future, task::Poll};

// PaLM TF-2b 3.27.4 / ADR 0004: a changed snapshot is never partial positive work.
// IHE PaLM TF-2b §3.27.4
#[tokio::test]
async fn competing_feed_during_paging_is_refused() -> Result<(), Error> {
    let (pages, collected) = StoreGate::new();
    let probe = Arc::new(StoreProbe::default());
    let gates = LawGates {
        pages: Some(pages),
        store: Some(probe.clone()),
        ..LawGates::default()
    };
    let orders = (0..101)
        .map(|i| order(&format!("order-{i:03}"), &format!("old-{i:03}"), None, 1))
        .collect::<Result<Vec<_>, _>>()?;
    let (_temp, path, reference, running, mut peer) = start(&definition(), orders, gates).await?;
    let store = probe.get().await?;
    reference.close().await?;
    send(&mut peer, query(ALL, "paging").as_bytes()).await?;
    collected.reached().await?;
    assert_eq!(count(&path, "machine_order_items")?, 0);
    quiet(&mut peer).await?;
    {
        let update = store.request::<_, analyte_store::OrderError>(std::time::Instant::now() + HANG_GUARD, |s| {
        let destination = analyte_model::Identifier::new("demo").map_err(analyte_store::StorageError::Text)?;
        let now = jiff::Timestamp::now().as_second();
        s.reset_orders(&destination)?;
        for chunk in (0..101).collect::<Vec<_>>().chunks(100) {
            let mut orders = Vec::new();
            for i in chunk {
                let document = serde_json::json!({"id":format!("order-{i:03}"),"version":2,"state":"requested","sample_id":format!("new-{i:03}"),"tests":[{"namespace":"synthetic","value":"TEST"}],"priority":"routine","target_analyzers":null,"expires_at":null,"tombstone":false}).to_string();
                orders.push(analyte_store::CachedOrder { id: analyte_model::Identifier::new(&format!("order-{i:03}")).map_err(analyte_store::StorageError::Text)?, sample: analyte_model::SampleId::new(&format!("new-{i:03}")).map_err(analyte_store::StorageError::Text)?, document, tombstone:false, expiry:now+3600 });
            }
            s.order_page(&analyte_store::OrderPage {destination:destination.clone(), cursor:analyte_model::Identifier::new("updated").map_err(analyte_store::StorageError::Text)?, complete:true, polled:now, orders})?;
        }
        Ok(())
    });
        tokio::pin!(update);
        // Poll the competing request while the store owner is held in page collection.
        std::future::poll_fn(|cx| match update.as_mut().poll(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(r) => panic!("feed update crossed held store request: {r:?}"),
        })
        .await;
        collected.resume();
        update.await?;
    }
    let rsp = packet(&mut peer).await?;
    assert_eq!(field(&rsp, b"QAK", 2)?, b"AE");
    quiet(&mut peer).await?;
    assert_eq!(count(&path, "machine_order_items")?, 0);
    assert_eq!(count(&path, "machine_orders")?, 0);
    let samples = database(&path)?
        .prepare("SELECT sample FROM orders ORDER BY id")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        samples,
        (0..101).map(|i| format!("new-{i:03}")).collect::<Vec<_>>()
    );
    drop(store);
    running.stop().await?;
    Ok(())
}
