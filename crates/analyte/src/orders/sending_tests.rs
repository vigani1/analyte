//! Safety rule 8 and ADR 0004: restart distinguishes reservation from transfer.
use super::*;
use analyte_model::{Identifier, SampleId};
use std::{path::Path, time::Duration};

struct Space;

impl analyte_store::FreeSpace for Space {
    fn available(&self, _: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

async fn open(path: &Path) -> Result<Handle, Error> {
    let limits = analyte_store::Limits {
        reserve_bytes: 1024,
        database_bytes: 100_000_000,
        record_bytes: 65_536,
        page_rows: 100,
        busy_timeout: Duration::from_millis(20),
    };
    Ok(Handle::start(path.to_owned(), limits, Box::new(Space), 4, deadline()).await?)
}

#[tokio::test]
async fn rule8_sending_is_internal_and_only_interrupted_transfer_recovers_uncertain()
-> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("orders.db");
    let mut store = open(&path).await?;
    let completed_key = MachineOrderKey {
        destination: Identifier::new("demo")?,
        order: Identifier::new("order-1")?,
        machine: Identifier::new("completed")?,
    };
    let interrupted_key = MachineOrderKey {
        machine: Identifier::new("interrupted")?,
        ..completed_key.clone()
    };
    let page = analyte_store::OrderPage {
        destination: completed_key.destination.clone(),
        cursor: Identifier::new("cursor")?,
        complete: true,
        polled: 100,
        orders: vec![analyte_store::CachedOrder {
            id: completed_key.order.clone(),
            sample: SampleId::new("000Ab12")?,
            document: include_str!("../../../../docs/contract/vectors/order.json").to_owned(),
            tombstone: false,
            expiry: 200,
        }],
    };
    let completed = completed_key.clone();
    let interrupted = interrupted_key.clone();
    let (completed, interrupted) = store
        .request(deadline(), move |store| {
            store.order_page(&page)?;
            let completed = store
                .reserve_send(&completed, std::num::NonZeroU64::MIN, 101, 30)?
                .map_err(|_| analyte_store::Error::Conflict)?;
            let interrupted = store
                .reserve_send(&interrupted, std::num::NonZeroU64::MIN, 101, 30)?
                .map_err(|_| analyte_store::Error::Conflict)?;
            assert!(
                store.claim(101)?.is_none(),
                "reservation must not enqueue an app event"
            );
            Ok((completed, interrupted))
        })
        .await?;
    assert_eq!(completed.state, OrderState::Sending);
    assert_eq!(interrupted.state, OrderState::Sending);
    assert!(
        matches!(super::super::event(&interrupted),Err(analyte_store::Error::DeliveryBuild(error)) if matches!(error.downcast_ref::<analyte_contract::DocumentError>(),Some(analyte_contract::DocumentError::InternalOrderState)))
    );
    finished(&store, &completed, analyte_dialog::astm::End::Normal).await?;
    store.shutdown(deadline()).await?;
    let mut store = open(&path).await?;
    recover(&store).await?;
    recover(&store).await?;
    store
        .request(deadline(), move |store| {
            let rows = store.machine_orders(&analyte_store::MachineOrderFilter {
                after: 0,
                machine: None,
                state: None,
            })?;
            assert_eq!(rows.len(), 2);
            assert_eq!(
                rows.first().ok_or(analyte_store::Error::Corruption)?.state,
                OrderState::Sent
            );
            let interrupted = rows.last().ok_or(analyte_store::Error::Corruption)?;
            assert_eq!(interrupted.state, OrderState::Uncertain);
            assert_eq!(interrupted.source_code.as_str(), "host-restart");
            for key in [completed_key, interrupted_key] {
                assert!(matches!(
                    store.reserve_send(&key, std::num::NonZeroU64::MIN, 102, 30)?,
                    Err(analyte_store::SendRefusal::AlreadyReserved)
                ));
            }
            let mut events = Vec::new();
            while let Some(claim) = store.claim(jiff::Timestamp::now().as_second())? {
                events.push(claim.payload);
                store.attempted(
                    claim.id,
                    &analyte_store::Attempt::Done,
                    jiff::Timestamp::now().as_second(),
                )?;
            }
            assert_eq!(events.len(), 2, "exactly one app event per real outcome");
            Ok(())
        })
        .await?;
    store.shutdown(deadline()).await?;
    Ok(())
}
