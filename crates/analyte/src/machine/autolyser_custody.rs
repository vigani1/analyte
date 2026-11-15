//! A durable poll intent prevents advancing R after an uncertain disconnect.
use crate::{Error, config::Machine, deadline, invalid};
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, Handle, Parked};

pub(super) async fn ensure_reconciled(store: &Handle, machine: &Machine) -> Result<(), Error> {
    let machine_id = machine.id.clone();
    let blocked = store
        .request(deadline(), move |store| {
            let mut after = 0;
            // Startup inspection is bounded too. A full history requires operator cleanup,
            // never permission to guess that there are no outstanding cursor movements.
            for _ in 0..128 {
                let page = store.parked(after)?;
                if page.is_empty() {
                    return Ok(false);
                }
                for item in page {
                    after = item.id;
                    if item.resolution.is_some() {
                        continue;
                    }
                    let evidence: serde_json::Value = serde_json::from_str(&item.evidence)?;
                    if evidence.get("kind").and_then(serde_json::Value::as_str)
                        == Some("autolyser_poll_custody")
                        && evidence
                            .get("targets")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|targets| {
                                targets
                                    .iter()
                                    .any(|t| t.as_str() == Some(machine_id.as_str()))
                            })
                    {
                        return Ok(true);
                    }
                }
            }
            Err(analyte_store::Error::InvalidInput)
        })
        .await?;
    if blocked {
        return Err(invalid(
            "Autolyser previous poll requires operator reconciliation before R can advance",
        ));
    }
    Ok(())
}

pub(super) async fn reserve(store: &Handle, machine: &Machine) -> Result<i64, Error> {
    let item = Parked {
        kind: Identifier::new("autolyser_poll_custody")?,
        reason: Reason::new(
            "Autolyser R may advance its cursor; retain this guard until its response is durably in custody or an operator reconciles it",
        )?,
        targets: vec![machine.id.clone()],
    };
    Ok(store
        .request(deadline(), move |store| store.park(&item))
        .await?)
}

pub(super) async fn complete(
    store: &Handle,
    guard: &mut Option<i64>,
    machine: &Machine,
) -> Result<(), Error> {
    if let Some(id) = *guard {
        let audit = Audit {
            actor: Identifier::new("analyte")?,
            action: Identifier::new("autolyser_poll_custody_complete")?,
            targets: vec![machine.id.clone()],
            time: jiff::Timestamp::now().as_second(),
            reason: Reason::new(
                "Complete Autolyser response durably stored; R cursor custody reconciled",
            )?,
        };
        store
            .request(deadline(), move |store| store.resolve(id, &audit))
            .await?;
        *guard = None;
    }
    Ok(())
}
