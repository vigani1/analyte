//! Tests sending behavior with synthetic fixtures.
#[derive(Debug, thiserror::Error)]
enum FixtureError {
    #[error(transparent)]
    Order(#[from] analyte_store::OrderError),
    #[error(transparent)]
    Delivery(#[from] analyte_store::DeliveryError),
    #[error(transparent)]
    Worker(#[from] analyte_store::WorkerError),
}

use super::*;
use crate::Error;
use analyte_model::{Identifier, SampleId};
use analyte_store::OrderReservation;
use std::{path::Path, time::Duration};

use analyte_fixtures::UnlimitedSpace as Space;

async fn open(path: &Path) -> Result<Handle, Error> {
    let limits = {
        let mut limits = analyte_fixtures::store_limits();
        limits.busy_timeout = Duration::from_millis(20);
        limits
    };
    Ok(Handle::start(
        path.to_owned(),
        limits,
        Box::new(Space),
        4,
        crate::test_startup_deadline()?,
    )
    .await?)
}

// Safety rule 8 (ADR 0003)
#[tokio::test]
async fn only_interrupted_transfer_recovers_as_uncertain() -> Result<(), Error> {
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
            document: include_str!("../../../../../docs/contract/vectors/order.json").to_owned(),
            tombstone: false,
            expiry: 200,
        }],
    };
    let completed = completed_key.clone();
    let interrupted = interrupted_key.clone();
    let model = Identifier::new("test/synthetic")?;
    let (completed, interrupted) = store
        .request::<_, FixtureError>(deadline()?, move |store| {
            store.order_page(&page)?;
            let completed = store
                .reserve_send(
                    &OrderReservation {
                        identity: &completed,
                        model: &model,
                        version: std::num::NonZeroU64::MIN,
                        now: 101,
                        freshness: 30,
                    },
                    analyte_store::ReserveOptions::default(),
                )?
                .map_err(|_| analyte_store::OrderError::Conflict)?;
            let interrupted = store
                .reserve_send(
                    &OrderReservation {
                        identity: &interrupted,
                        model: &model,
                        version: std::num::NonZeroU64::MIN,
                        now: 101,
                        freshness: 30,
                    },
                    analyte_store::ReserveOptions::default(),
                )?
                .map_err(|_| analyte_store::OrderError::Conflict)?;
            assert!(
                store
                    .claim_destination(101, &completed.key.destination)?
                    .is_none(),
                "reservation must not enqueue an app event"
            );
            Ok::<_, FixtureError>((completed, interrupted))
        })
        .await?;
    assert_eq!(completed.state, OrderState::Sending);
    assert_eq!(interrupted.state, OrderState::Sending);
    assert!(
        matches!(super::super::event(&interrupted),Err(analyte_store::OrderError::DeliveryBuild(error)) if matches!(error.downcast_ref::<analyte_contract::DocumentError>(),Some(analyte_contract::DocumentError::InternalOrderState)))
    );
    finished(&store, &completed, analyte_dialog::astm::End::Normal).await?;
    store.shutdown(deadline()?).await?;
    let mut store = open(&path).await?;
    recover(&store).await?;
    recover(&store).await?;
    assert_recovered_outcomes(&store, completed_key, interrupted_key).await?;
    store.shutdown(deadline()?).await?;
    Ok(())
}

async fn assert_recovered_outcomes(
    store: &Handle,
    completed_key: MachineOrderKey,
    interrupted_key: MachineOrderKey,
) -> Result<(), Error> {
    let model = Identifier::new("test/synthetic")?;
    store
        .request::<_, FixtureError>(deadline()?, move |store| {
            let rows = store.machine_orders(&analyte_store::MachineOrderFilter {
                after: 0,
                machine: None,
                state: None,
            })?;
            assert_eq!(rows.len(), 2);
            assert_eq!(
                rows.first()
                    .ok_or(analyte_store::OrderError::Corruption)?
                    .state,
                OrderState::Sent
            );
            let interrupted = rows.last().ok_or(analyte_store::OrderError::Corruption)?;
            assert_eq!(interrupted.state, OrderState::Uncertain);
            assert_eq!(interrupted.source_code.as_str(), "host-restart");
            let destination = completed_key.destination.clone();
            for key in [completed_key, interrupted_key] {
                assert!(matches!(
                    store.reserve_send(
                        &OrderReservation {
                            identity: &key,
                            model: &model,
                            version: std::num::NonZeroU64::MIN,
                            now: 102,
                            freshness: 30
                        },
                        analyte_store::ReserveOptions::default()
                    )?,
                    Err(analyte_store::SendRefusal::AlreadyReserved)
                ));
            }
            let mut events = Vec::new();
            while let Some(claim) =
                store.claim_destination(jiff::Timestamp::now().as_second(), &destination)?
            {
                events.push(claim.payload);
                store.attempted(
                    claim.id,
                    &analyte_store::Attempt::Done,
                    jiff::Timestamp::now().as_second(),
                )?;
            }
            assert_eq!(events.len(), 2, "exactly one app event per real outcome");
            Ok::<_, FixtureError>(())
        })
        .await?;
    Ok(())
}

// ADR 0003
#[tokio::test]
async fn cleanup_attempts_every_key_and_keeps_failures() -> Result<(), Error> {
    let temp = tempfile::tempdir()?;
    let mut store = open(&temp.path().join("closed.db")).await?;
    store.shutdown(deadline()?).await?;
    let key = MachineOrderKey {
        destination: Identifier::new("demo")?,
        order: Identifier::new("order")?,
        machine: Identifier::new("machine")?,
    };
    let owner = Identifier::new("owner")?;
    let error = settle_owned(
        &store,
        vec![key.clone(), key.clone(), key],
        &owner,
        "disconnect",
        Settlement::Order,
    )
    .await
    .expect_err("closed store must reject each cleanup");
    // Each link proves another independent request was attempted after failure.
    assert_eq!(
        error.to_string().matches("dialog effect failed:").count(),
        2
    );
    Ok(())
}
