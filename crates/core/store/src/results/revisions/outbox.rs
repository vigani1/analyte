//! Writes eligible result deliveries within the caller transaction.
use super::{
    Delivery, Error, IdentityStatus, Interpretation, Reading, Reconciliation, RevisionOutcome,
    digest, key, params,
};
impl Reconciliation<'_, '_> {
    pub(super) fn write_deliveries(
        &self,
        receipt_key: &str,
        version: i64,
        evidence: &Interpretation,
        outcomes: &[RevisionOutcome],
        deliveries: Vec<Delivery>,
    ) -> Result<(), Error> {
        for delivery in deliveries {
            let eligible = match &evidence.outcome {
                Reading::Results(results) => results.iter().zip(outcomes).any(|(r,o)| r.may_deliver_to_production() && self.lifecycle.is_none_or(|policy|policy.may_deliver(r)) && matches!(o, RevisionOutcome::New(_) | RevisionOutcome::Correction(_)) && matches!(&r.identity, IdentityStatus::Resolved(id) if id.observation == delivery.ordering_key.observation)),
                Reading::Failed(_) => false,
            };
            if !eligible
                || delivery.ordering_key.machine != *self.machine
                || delivery.payload.len() > self.limit
            {
                return Err(Error::InvalidInterpretationInput);
            }
            self.tx.execute("INSERT INTO outbox(id,receipt,version,destination,ordering_key,payload,digest,contract,state,next_attempt,enqueued_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,unixepoch())", params![key(&delivery.id)?,receipt_key,version,delivery.destination.as_str(),key(&delivery.ordering_key)?,delivery.payload,digest(&delivery.payload).0.as_slice(),delivery.contract_version,crate::DeliveryState::Pending])?;
        }

        Ok(())
    }
}
