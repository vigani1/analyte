//! Applies the selected intake policy without exposing protocol custody to shared intake.
use crate::intake::InterpretationOutcome;
pub(crate) use crate::machine::law::policy::{BatchConflict, Mismatch as WorkMismatch};
use analyte_store::{ReconcileMode, RepeatPolicy, StatusLifecycle};

pub(crate) enum Policy {
    /// Ordinary identity recipes with the site's repeated-results choice.
    Standard(RepeatPolicy),
    /// Independent readings whose indistinguishable edits park with this reason.
    EditGuard(&'static str),
    Execution(crate::machine::law::policy::Policy),
}
impl Policy {
    /// Ordinary reconciliation driven by the site file's repeated-results
    /// choice (ADR 0035, 2026-11-26).
    pub(crate) fn standard(repeats: analyte_config::RepeatedResults) -> Self {
        Self::Standard(match repeats {
            analyte_config::RepeatedResults::Automatic => RepeatPolicy::Automatic,
            analyte_config::RepeatedResults::AlwaysAsk => RepeatPolicy::AlwaysAsk,
        })
    }
    pub(crate) fn atomic(&self) -> bool {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => false,
            Self::Execution(_) => true,
        }
    }
    pub(crate) fn lifecycle(&self) -> Option<StatusLifecycle<'static>> {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => None,
            Self::Execution(_) => Some(crate::machine::law::policy::lifecycle()),
        }
    }
    pub(crate) fn mode(&self) -> ReconcileMode<'_> {
        match self {
            Self::Standard(repeats) => ReconcileMode::Standard(*repeats),
            Self::EditGuard(reason) => ReconcileMode::EditGuard(reason),
            Self::Execution(policy) => policy.mode(),
        }
    }
    pub(crate) fn work_conflict(&self) -> bool {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => false,
            Self::Execution(policy) => policy.has_work_conflict(),
        }
    }
    pub(crate) fn conflict_reason(
        &self,
    ) -> Result<Option<crate::intake::ParkReason>, analyte_model::TextError> {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => Ok(None),
            Self::Execution(policy) => {
                if policy.has_work_conflict() {
                    Ok(Some(crate::machine::law::policy::conflict_reason()?))
                } else {
                    Ok(None)
                }
            }
        }
    }
    pub(crate) fn discard_custody(&mut self) {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => {}
            Self::Execution(policy) => policy.discard_custody(),
        }
    }
    pub(crate) fn rejected(&self) -> Self {
        match self {
            Self::Standard(repeats) => Self::Standard(*repeats),
            Self::EditGuard(_) => Self::Standard(RepeatPolicy::Automatic),
            Self::Execution(policy) => Self::Execution(policy.rejected()),
        }
    }
    pub(crate) fn mapping_refusal(&self) -> Option<&'static str> {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => None,
            Self::Execution(_) => Some(crate::machine::law::policy::MAPPING_REFUSAL),
        }
    }
    pub(crate) fn outcome(
        &self,
        commit: &analyte_store::InterpretationCommit,
        successful: InterpretationOutcome,
    ) -> InterpretationOutcome {
        match self {
            Self::Standard(_) | Self::EditGuard(_) => successful,
            Self::Execution(policy) => policy.outcome(commit, successful),
        }
    }
}
