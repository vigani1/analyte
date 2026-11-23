//! Shares protected measurement review and decisions between the browser and CLI.
use super::Error;
use crate::{deadline, installation::config::Program};
use analyte_model::{Identifier, Reason};
use analyte_store::{
    Handle, MeasurementConfirmation, MeasurementDecision, MeasurementReview, ReconciliationError,
};

/// Returns an audited comparison, when the item has eligible retained evidence.
/// # Errors
/// Reports audit or evidence failures before disclosing any result data.
pub async fn review(
    store: &Handle,
    id: i64,
    actor: Identifier,
) -> Result<Option<MeasurementReview>, Error> {
    Ok(store
        .request(deadline()?, move |s| {
            s.measurement_review(id, actor, jiff::Timestamp::now().as_second())
        })
        .await?)
}

/// Confirms the same protected snapshot on both operator surfaces.
/// Cancellation can leave the action committed, including its delivery.
/// # Errors
/// Refuses invalid reasons, stale comparisons, unavailable routes and storage failures.
pub async fn decide(
    store: &Handle,
    program: Option<&Program>,
    id: i64,
    snapshot: String,
    decision: MeasurementDecision,
    actor: Identifier,
    typed: &str,
) -> Result<(), Error> {
    let reason = checked_reason(typed).map_err(Error::ReviewReason)?;
    let routes = program
        .into_iter()
        .flat_map(Program::machines)
        .filter_map(|machine| {
            let destination = machine.snapshot.effective().site().destination.as_ref()?;
            Some((
                machine.id.clone(),
                machine.snapshot.digest(),
                machine.model.clone(),
                destination.name.clone(),
            ))
        })
        .collect::<Vec<_>>();
    store
        .request(deadline()?, move |s| {
            s.decide_measurement(
                MeasurementConfirmation {
                    item: id,
                    snapshot: &snapshot,
                    decision,
                    actor,
                    time: jiff::Timestamp::now().as_second(),
                    reason,
                },
                |result, receipt| {
                    let (_, _, model, destination) = routes
                        .iter()
                        .find(|(machine, digest, _, _)| {
                            machine == &receipt.machine && *digest == receipt.config_digest.0
                        })
                        .ok_or(ReconciliationError::ReviewChanged)?;
                    analyte_sink::encode_delivery(
                        result,
                        receipt,
                        model,
                        destination.clone(),
                        jiff::Timestamp::now(),
                    )
                    .map_err(|error| ReconciliationError::DeliveryBuild(Box::new(error)))
                },
            )
        })
        .await?;
    Ok(())
}

/// Normalizes a typed audit reason to one nonblank line.
/// # Errors
/// Refuses blank reasons and reasons outside the shared dialog bounds.
pub fn checked_reason(typed: &str) -> Result<Reason, &'static str> {
    let line = typed.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
    if line.trim().is_empty() {
        return Err("Write a reason. It is recorded in the history.");
    }
    if line.chars().count() > 256 {
        return Err("Keep the reason to 256 characters.");
    }
    Reason::new(&line).map_err(|_| "The reason holds a character that cannot be recorded.")
}
