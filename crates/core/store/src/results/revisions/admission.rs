//! Checks provenance, input bounds and byte budgets before writing revisions.
use super::{AssignedRevision, Delivery, Error, Interpretation, Reading, Store};
impl Store {
    pub(super) fn validate_reconciliation(
        &self,
        stored: &crate::Receipt,
        evidence: &Interpretation,
        corrections: &[bool],
        findings: &[crate::Parked],
        reprocessing: bool,
    ) -> Result<(usize, Vec<String>), Error> {
        if !reprocessing
            && (evidence.config != stored.config
                || evidence.driver != stored.metadata.driver
                || evidence.config_version != stored.metadata.config_version)
        {
            return Err(Error::InvalidInterpretationInput);
        }
        let count = match &evidence.outcome {
            Reading::Results(r) => r.len(),
            Reading::Failed(_) => 0,
        };
        if count != corrections.len() || count > self.limits.page_rows {
            return Err(Error::InvalidInterpretationInput);
        }
        self.encode(evidence)?;
        if findings.len() > self.limits.page_rows {
            return Err(Error::InvalidInterpretationInput);
        }
        let findings = findings
            .iter()
            .map(|finding| {
                self.encode(&crate::parked::ParkingEvidence {
                    details: finding.clone(),
                    context: None,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((count, findings))
    }
}

pub(super) fn required_capacity(
    encoded: &str,
    audit: Option<&str>,
    findings: &[String],
    deliveries: &[Delivery],
    revisions: &[AssignedRevision],
    limit: usize,
) -> Result<usize, Error> {
    let mut required = encoded
        .len()
        .checked_add(
            audit
                .map_or(0, str::len)
                .checked_mul(2)
                .ok_or(Error::Storage(crate::StorageError::Capacity))?,
        )
        .ok_or(Error::Storage(crate::StorageError::Capacity))?;
    if audit.is_some_and(|value| value.len() > limit) {
        return Err(Error::InvalidInterpretationInput);
    }
    for finding in findings {
        required = required
            .checked_add(finding.len())
            .ok_or(Error::Storage(crate::StorageError::Capacity))?;
    }
    for delivery in deliveries {
        required = required
            .checked_add(delivery.payload.len())
            .ok_or(Error::Storage(crate::StorageError::Capacity))?;
    }
    for (identity, _, _) in revisions {
        required = required
            .checked_add(identity.len())
            .and_then(|n| n.checked_add(128))
            .ok_or(Error::Storage(crate::StorageError::Capacity))?;
    }

    Ok(required)
}
