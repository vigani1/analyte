//! Applies the selected intake policy without exposing protocol custody to shared intake.
use crate::intake::InterpretationOutcome;
pub(crate) use crate::machine::law::policy::{BatchConflict, Mismatch as WorkMismatch};
use analyte_store::{ReconcileMode, StatusLifecycle};

pub(crate) enum Policy {
    Standard { edit_guard: Option<&'static str> },
    Execution(crate::machine::law::policy::Policy),
}
impl Policy {
    pub(crate) fn standard() -> Self {
        Self::Standard { edit_guard: None }
    }
    pub(crate) fn independent(reason: &'static str) -> Self {
        Self::Standard {
            edit_guard: Some(reason),
        }
    }
    pub(crate) fn atomic(&self) -> bool {
        match self {
            Self::Standard { .. } => false,
            Self::Execution(_) => true,
        }
    }
    pub(crate) fn lifecycle(&self) -> Option<StatusLifecycle<'static>> {
        match self {
            Self::Standard { .. } => None,
            Self::Execution(_) => Some(crate::machine::law::policy::lifecycle()),
        }
    }
    pub(crate) fn mode(&self) -> ReconcileMode<'_> {
        match self {
            Self::Standard { edit_guard: None } => ReconcileMode::Standard,
            Self::Standard {
                edit_guard: Some(reason),
            } => ReconcileMode::EditGuard(reason),
            Self::Execution(policy) => policy.mode(),
        }
    }
    pub(crate) fn work_conflict(&self) -> bool {
        match self {
            Self::Standard { .. } => false,
            Self::Execution(policy) => policy.work_conflict.is_some(),
        }
    }
    pub(crate) fn conflict_reason(
        &self,
    ) -> Result<Option<crate::intake::ParkReason>, analyte_model::TextError> {
        match self {
            Self::Standard { .. } => Ok(None),
            Self::Execution(policy) => {
                if policy.work_conflict.is_some() {
                    Ok(Some(crate::machine::law::policy::conflict_reason()?))
                } else {
                    Ok(None)
                }
            }
        }
    }
    pub(crate) fn discard_custody(&mut self) {
        match self {
            Self::Standard { .. } => {}
            Self::Execution(policy) => policy.custody = None,
        }
    }
    pub(crate) fn rejected(&self) -> Self {
        match self {
            Self::Standard { .. } => Self::standard(),
            Self::Execution(policy) => Self::Execution(policy.rejected()),
        }
    }
    pub(crate) fn mapping_refusal(&self) -> Option<&'static str> {
        match self {
            Self::Standard { .. } => None,
            Self::Execution(_) => Some(crate::machine::law::policy::MAPPING_REFUSAL),
        }
    }
    pub(crate) fn outcome(
        &self,
        commit: &analyte_store::InterpretationCommit,
        successful: InterpretationOutcome,
    ) -> InterpretationOutcome {
        match self {
            Self::Standard { .. } => successful,
            Self::Execution(policy) => policy.outcome(commit, successful),
        }
    }
}
