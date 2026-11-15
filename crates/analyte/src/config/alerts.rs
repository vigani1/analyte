//! Bounded alert policies. Runtime evaluation is owned by the service.
use serde::Deserialize;

/// Specific policy failures; diagnostics never echo responder text.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AlertConfigError {
    /// Evaluation interval is outside the bounded range.
    #[error("alert interval must be between 1 and 3600 seconds")]
    Interval,
    /// Too many configured policies.
    #[error("at most 128 alert policies are supported")]
    TooMany,
    /// Machine is not configured.
    #[error("alert references an unknown machine")]
    UnknownMachine,
    /// Duplicate signal and selector.
    #[error("duplicate alert kind and machine selector")]
    Duplicate,
    /// Unsupported kind.
    #[error("unknown alert kind")]
    UnknownKind,
    /// Missing, oversized or multiline responder.
    #[error("responder must be nonempty, single-line and at most 256 bytes")]
    Responder,
    /// Impossible numeric threshold.
    #[error("invalid alert threshold")]
    Threshold,
    /// Missing or inappropriate machine selector.
    #[error("invalid alert machine selector")]
    Machine,
}

/// Supported nonclinical signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    /// Oldest pending delivery age in seconds.
    OldestUndelivered,
    /// Silence duration in seconds for an explicitly selected busy machine.
    MachineSilent,
    /// Margin in bytes above the configured disk reserve.
    DiskReserve,
    /// Order cache age in seconds.
    OrderCacheStale,
    /// Uncertain order count strictly above the threshold.
    UncertainOrders,
    /// Parked item count strictly above the threshold.
    ParkedItems,
    /// Restore reconciliation pause; threshold is one.
    RestorePaused,
}

/// Validated policy. Fields cannot be changed after boundary validation.
#[derive(Debug, Clone)]
pub struct Alert {
    kind: AlertKind,
    threshold: u64,
    responder: String,
    machine: Option<analyte_model::Identifier>,
}
impl Alert {
    /// Returns the supported signal.
    #[must_use]
    pub fn kind(&self) -> AlertKind {
        self.kind
    }
    /// Returns seconds, bytes, count or one, according to the kind.
    #[must_use]
    pub fn threshold(&self) -> u64 {
        self.threshold
    }
    /// Borrows the named role or person.
    #[must_use]
    pub fn responder(&self) -> &str {
        &self.responder
    }
    /// Borrows the selected busy machine, if any.
    #[must_use]
    pub fn machine(&self) -> Option<&analyte_model::Identifier> {
        self.machine.as_ref()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftAlert {
    kind: String,
    threshold: u64,
    responder: String,
    machine: Option<analyte_model::Identifier>,
}
impl DraftAlert {
    pub(super) fn validate(self, reserve: u64) -> Result<Alert, AlertConfigError> {
        let kind = match self.kind.as_str() {
            "oldest_undelivered" => AlertKind::OldestUndelivered,
            "machine_silent" => AlertKind::MachineSilent,
            "disk_reserve" => AlertKind::DiskReserve,
            "order_cache_stale" => AlertKind::OrderCacheStale,
            "uncertain_orders" => AlertKind::UncertainOrders,
            "parked_items" => AlertKind::ParkedItems,
            "restore_paused" => AlertKind::RestorePaused,
            _ => return Err(AlertConfigError::UnknownKind),
        };
        if self.responder.trim().is_empty()
            || self.responder.len() > 256
            || self.responder.chars().any(char::is_control)
        {
            return Err(AlertConfigError::Responder);
        }
        if (kind == AlertKind::MachineSilent) != self.machine.is_some() {
            return Err(AlertConfigError::Machine);
        }
        match kind {
            AlertKind::OldestUndelivered
            | AlertKind::MachineSilent
            | AlertKind::OrderCacheStale => {
                if !(1..=31_536_000).contains(&self.threshold) {
                    return Err(AlertConfigError::Threshold);
                }
            }
            AlertKind::DiskReserve => {
                reserve
                    .checked_add(self.threshold)
                    .ok_or(AlertConfigError::Threshold)?;
            }
            AlertKind::RestorePaused => {
                if self.threshold != 1 {
                    return Err(AlertConfigError::Threshold);
                }
            }
            AlertKind::UncertainOrders | AlertKind::ParkedItems => {}
        }
        Ok(Alert {
            kind,
            threshold: self.threshold,
            responder: self.responder,
            machine: self.machine,
        })
    }
}

pub(super) fn interval_default() -> u64 {
    30
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft(kind: &str, threshold: u64) -> DraftAlert {
        DraftAlert {
            kind: kind.into(),
            threshold,
            responder: "lab duty officer".into(),
            machine: None,
        }
    }
    #[test]
    fn plan11_kinds_and_thresholds_are_specific() {
        for kind in [
            "oldest_undelivered",
            "disk_reserve",
            "order_cache_stale",
            "uncertain_orders",
            "parked_items",
            "restore_paused",
        ] {
            assert!(draft(kind, 1).validate(100).is_ok());
        }
        let mut silent = draft("machine_silent", 1);
        assert_eq!(silent.validate(100).unwrap_err(), AlertConfigError::Machine);
        silent = draft("machine_silent", 1);
        silent.machine = Some(analyte_model::Identifier::new("busy").expect("synthetic identity"));
        assert!(silent.validate(100).is_ok());
        assert_eq!(
            draft("typo", 1).validate(100).unwrap_err(),
            AlertConfigError::UnknownKind
        );
        for kind in ["oldest_undelivered", "order_cache_stale"] {
            for value in [0, 31_536_001] {
                assert_eq!(
                    draft(kind, value).validate(100).unwrap_err(),
                    AlertConfigError::Threshold
                );
            }
        }
        assert_eq!(
            draft("disk_reserve", u64::MAX).validate(100).unwrap_err(),
            AlertConfigError::Threshold
        );
        assert_eq!(
            draft("restore_paused", 0).validate(100).unwrap_err(),
            AlertConfigError::Threshold
        );
        assert!(draft("uncertain_orders", 0).validate(100).is_ok());
    }
    #[test]
    fn plan11_responder_is_required_bounded_and_single_line() {
        for responder in [
            String::new(),
            " ".into(),
            "x".repeat(257),
            "role\nsecond".into(),
        ] {
            let mut policy = draft("restore_paused", 1);
            policy.responder = responder;
            assert_eq!(
                policy.validate(100).unwrap_err(),
                AlertConfigError::Responder
            );
        }
        assert!(toml::from_str::<DraftAlert>("kind='restore_paused'\nthreshold=1").is_err());
    }
}
