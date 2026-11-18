//! Supplies LAW custody, lifecycle and returned-work conflict policy for intake.
use super::custody;
pub(crate) use crate::machine::law::orders::validation::Mismatch;
use crate::{
    installation::config::Machine,
    intake::{InterpretationOutcome, ParkReason},
};
use analyte_store::{Handle, ReconcileMode, StatusLifecycle};

#[derive(Clone)]
pub(crate) struct Policy {
    custody: Option<analyte_store::law::Custody>,
    work_conflict: Option<Mismatch>,
}
impl Policy {
    pub(crate) async fn prepare(
        store: &Handle,
        machine: &Machine,
        results: Option<&analyte_hl7::law::Results<'_>>,
    ) -> Result<Self, custody::Error> {
        let work_conflict = custody::work_conflict(store, machine, results).await?;
        let custody = custody::plan(store, machine, results, work_conflict).await?;
        Ok(Self {
            custody,
            work_conflict,
        })
    }
    pub(crate) fn has_work_conflict(&self) -> bool {
        self.work_conflict.is_some()
    }
    pub(crate) fn discard_custody(&mut self) {
        // A work refusal must retain its rejection evidence even when mapping failed.
        if self.work_conflict.is_none() {
            self.custody = None;
        }
    }
    pub(crate) fn mode(&self) -> ReconcileMode<'_> {
        match &self.custody {
            Some(custody) => ReconcileMode::Law {
                lifecycle: lifecycle(),
                custody,
            },
            None => ReconcileMode::Lifecycle(lifecycle()),
        }
    }
    pub(crate) fn rejected(&self) -> Self {
        let mut policy = self.clone();
        if let Some(custody) = &mut policy.custody {
            custody.rejection = Some(analyte_store::law::Rejection {
                occurrence: 1,
                reason: analyte_store::law::Conflict::Progression,
            });
        }
        policy
    }
    pub(crate) fn outcome(
        &self,
        commit: &analyte_store::InterpretationCommit,
        successful: InterpretationOutcome,
    ) -> InterpretationOutcome {
        match commit.law {
            Some(analyte_store::law::Disposition::Accepted) => InterpretationOutcome::Accepted,
            Some(analyte_store::law::Disposition::Rejected(rejection)) => {
                InterpretationOutcome::WorkConflict(self.work_conflict.unwrap_or(Mismatch {
                    segment: b"OBR",
                    occurrence: rejection.occurrence,
                    field: 2,
                }))
            }
            None => self
                .work_conflict
                .map_or(successful, InterpretationOutcome::WorkConflict),
        }
    }
}
pub(crate) fn conflict_reason() -> Result<ParkReason, analyte_model::TextError> {
    Ok(ParkReason::WorkConflict(analyte_model::Reason::new(
        "LAW returned work differs from retained downloaded command; operator review required",
    )?))
}
pub(crate) const MAPPING_REFUSAL: &str =
    "LAW mapping requires operator review; inspect protected findings and original receipt";
#[derive(Debug, thiserror::Error)]
#[error(
    "LAW result batch conflicts with retained observation history; no result revisions committed"
)]
pub(crate) struct BatchConflict;
pub(crate) fn lifecycle() -> StatusLifecycle<'static> {
    StatusLifecycle {
        transitions: analyte_hl7::law::STATUS_TRANSITIONS,
        withheld: analyte_hl7::law::WITHHELD_STATUSES,
        single_run: analyte_hl7::law::SINGLE_RUN_STATUSES,
        run: |identity| {
            analyte_hl7::law::observation_run(identity.sub_id.as_str()).map(str::to_owned)
        },
    }
}

#[cfg(test)]
mod tests {
    //! Checks that pure mapping cannot bypass the LAW host custody boundary.
    use crate::intake::{Mapping, map_reading};
    use analyte_store::Reading;

    #[test]
    fn work_conflict_reason_is_preserved() -> Result<(), analyte_model::TextError> {
        assert_eq!(
            super::conflict_reason()?.into_reason()?.as_str(),
            "LAW returned work differs from retained downloaded command; operator review required"
        );
        Ok(())
    }
    #[test]
    fn adr0003_0010_law_status_maps_for_transactional_custody()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut mapped = analyte_mapping::Output::default();
        mapped.status_notices = vec![analyte_mapping::law::StatusNotice {
            position: analyte_mapping::Position {
                record: 4,
                field: 2,
            },
            processing: analyte_model::ProcessingMode::Production,
            execution: analyte_model::AwosExecution::InProgress,
            order: analyte_model::SourceText::new("OBR||awos-1||test^Test^99LAB")?,
            specimen: analyte_model::SourceText::new("SPM|1")?,
            container: analyte_model::SourceText::new("SAC|||tube")?,
            control: analyte_model::SourceText::new("ORC|SC||||IP")?,
            estimate: analyte_mapping::law::AvailabilityEstimate::Absent,
        }];

        let reading = map_reading(Mapping::Mapped(mapped))?;
        assert!(reading.accepted);
        assert!(matches!(reading.reading, Reading::Results(ref results) if results.is_empty()));
        assert!(reading.corrections.is_empty());
        Ok(())
    }
}
