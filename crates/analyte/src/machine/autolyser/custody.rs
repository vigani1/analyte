//! Records Autolyser poll custody and blocks R while its outcome is uncertain.
use super::Error;
use crate::{deadline, installation::config::Machine};
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, Handle, PollIntentId, PollIntentState, PollOutcome};

fn audit(
    id: PollIntentId,
    machine: &Identifier,
    outcome: PollOutcome,
    reason: Reason,
) -> Result<Audit, analyte_model::TextError> {
    Ok(Audit {
        actor: Identifier::new("analyte")?,
        action: Identifier::new(match outcome {
            PollOutcome::Confirmed => "autolyser_poll_custody_complete",
            PollOutcome::Uncertain => "autolyser_poll_custody_uncertain",
        })?,
        targets: vec![machine.clone(), Identifier::new(&format!("poll:{id}"))?],
        time: jiff::Timestamp::now().as_second(),
        reason,
    })
}

pub(super) async fn ensure_reconciled(store: &Handle, machine: &Machine) -> Result<(), Error> {
    let machine_id = machine.id.clone();
    let blocked = store
        .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
            let Some(intent) = store.unconfirmed_poll_intent(&machine_id)? else {
                return Ok(false);
            };
            match intent.state {
                PollIntentState::Pending => {
                    let reason = Reason::new(
                        "Autolyser previous session left a poll without confirmed cursor custody",
                    )
                    .map_err(|cause| {
                        analyte_store::OrderError::Storage(analyte_store::StorageError::Text(cause))
                    })?;
                    let audit = audit(intent.id, &machine_id, PollOutcome::Uncertain, reason)
                        .map_err(|error| {
                            analyte_store::OrderError::DeliveryBuild(Box::new(error))
                        })?;
                    store.record_poll_outcome(intent.id, PollOutcome::Uncertain, &audit)?;
                }
                PollIntentState::Uncertain => {}
                PollIntentState::Confirmed => return Err(analyte_store::OrderError::Corruption),
            }
            Ok(true)
        })
        .await?;
    if blocked {
        return Err(Error::PollUnreconciled);
    }
    Ok(())
}

pub(super) async fn reserve(store: &Handle, machine: &Machine) -> Result<PollIntentId, Error> {
    let machine = machine.id.clone();
    let reason = Reason::new(
        "Autolyser R may advance its cursor; retain this guard until its response is durably in custody or an operator reconciles it",
    )?;
    Ok(store
        .request(deadline()?, move |store| {
            store.begin_poll_intent(&machine, &reason)
        })
        .await?)
}

pub(super) async fn uncertain(
    store: &Handle,
    guard: Option<PollIntentId>,
    machine: &Machine,
    reason: &str,
) -> Result<(), Error> {
    if let Some(id) = guard {
        let audit = audit(
            id,
            &machine.id,
            PollOutcome::Uncertain,
            Reason::new(reason)?,
        )?;
        store
            .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                store
                    .record_poll_outcome(id, PollOutcome::Uncertain, &audit)
                    .map(|_| ())
            })
            .await?;
    }
    Ok(())
}

pub(super) async fn complete(
    store: &Handle,
    guard: &mut Option<PollIntentId>,
    machine: &Machine,
) -> Result<(), Error> {
    if let Some(id) = *guard {
        let audit = audit(
            id,
            &machine.id,
            PollOutcome::Confirmed,
            Reason::new("Complete Autolyser response durably stored; R cursor custody reconciled")?,
        )?;
        let state = store
            .request::<_, analyte_store::OrderError>(deadline()?, move |store| {
                store
                    .record_poll_outcome(id, PollOutcome::Confirmed, &audit)
                    .map(|intent| intent.state)
            })
            .await?;
        match state {
            PollIntentState::Confirmed => *guard = None,
            PollIntentState::Pending | PollIntentState::Uncertain => {
                return Err(Error::PollConfirmationDidNotReconcileCustody);
            }
        }
    }
    Ok(())
}
