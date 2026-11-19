//! Holds bounded alert policies for service evaluation.
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
    /// Restore reconciliation pause; has no threshold.
    RestorePaused,
    /// Items waiting, strictly above the threshold, for one lab app destination
    /// this run cannot deliver to (not configured, or its keys are missing).
    UndeliverableDestination,
}

/// Validated policy. Each signal owns exactly the fields it needs.
#[derive(Debug, Clone)]
pub struct Alert {
    condition: Condition,
    responder: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Condition {
    OldestUndelivered(u64),
    MachineSilent(analyte_model::Identifier, u64),
    DiskReserve { margin: u64, below: u64 },
    OrderCacheStale(u64),
    UncertainOrders(u64),
    ParkedItems(u64),
    RestorePaused,
    UndeliverableDestination(u64),
}

impl Alert {
    #[cfg(all(test, feature = "test-hooks"))]
    pub(crate) fn for_test(text: &str) -> Result<Self, crate::Error> {
        Ok(toml::from_str::<DraftAlert>(text)?.validate(1024)?)
    }

    pub(crate) fn condition(&self) -> &Condition {
        &self.condition
    }

    /// Returns the supported signal.
    #[must_use]
    pub fn kind(&self) -> AlertKind {
        match self.condition {
            Condition::OldestUndelivered(_) => AlertKind::OldestUndelivered,
            Condition::MachineSilent(_, _) => AlertKind::MachineSilent,
            Condition::DiskReserve { .. } => AlertKind::DiskReserve,
            Condition::OrderCacheStale(_) => AlertKind::OrderCacheStale,
            Condition::UncertainOrders(_) => AlertKind::UncertainOrders,
            Condition::ParkedItems(_) => AlertKind::ParkedItems,
            Condition::RestorePaused => AlertKind::RestorePaused,
            Condition::UndeliverableDestination(_) => AlertKind::UndeliverableDestination,
        }
    }
    /// Returns seconds, bytes or count; restore pause has no threshold.
    #[must_use]
    pub fn threshold(&self) -> Option<u64> {
        match self.condition {
            Condition::OldestUndelivered(value)
            | Condition::MachineSilent(_, value)
            | Condition::OrderCacheStale(value)
            | Condition::UncertainOrders(value)
            | Condition::ParkedItems(value)
            | Condition::UndeliverableDestination(value) => Some(value),
            Condition::DiskReserve { margin, .. } => Some(margin),
            Condition::RestorePaused => None,
        }
    }
    /// Borrows the named role or person.
    #[must_use]
    pub fn responder(&self) -> &str {
        &self.responder
    }
    /// Borrows the selected busy machine, if any.
    #[must_use]
    pub fn machine(&self) -> Option<&analyte_model::Identifier> {
        match &self.condition {
            Condition::MachineSilent(machine, _) => Some(machine),
            Condition::OldestUndelivered(_)
            | Condition::DiskReserve { .. }
            | Condition::OrderCacheStale(_)
            | Condition::UncertainOrders(_)
            | Condition::ParkedItems(_)
            | Condition::RestorePaused
            | Condition::UndeliverableDestination(_) => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftAlert {
    kind: String,
    threshold: Option<u64>,
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
            "undeliverable_destination" => AlertKind::UndeliverableDestination,
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
        let threshold = match kind {
            AlertKind::RestorePaused => {
                if self.threshold.is_some() {
                    return Err(AlertConfigError::Threshold);
                }
                None
            }
            AlertKind::OldestUndelivered
            | AlertKind::MachineSilent
            | AlertKind::DiskReserve
            | AlertKind::OrderCacheStale
            | AlertKind::UncertainOrders
            | AlertKind::ParkedItems
            | AlertKind::UndeliverableDestination => {
                Some(self.threshold.ok_or(AlertConfigError::Threshold)?)
            }
        };
        let condition = match (kind, threshold) {
            (AlertKind::RestorePaused, None) => Condition::RestorePaused,
            (
                AlertKind::OldestUndelivered
                | AlertKind::MachineSilent
                | AlertKind::OrderCacheStale,
                Some(age),
            ) if !(1..=31_536_000).contains(&age) => return Err(AlertConfigError::Threshold),
            (AlertKind::OldestUndelivered, Some(age)) => Condition::OldestUndelivered(age),
            (AlertKind::MachineSilent, Some(age)) => {
                Condition::MachineSilent(self.machine.ok_or(AlertConfigError::Machine)?, age)
            }
            (AlertKind::OrderCacheStale, Some(age)) => Condition::OrderCacheStale(age),
            (AlertKind::DiskReserve, Some(margin)) => Condition::DiskReserve {
                margin,
                below: reserve
                    .checked_add(margin)
                    .ok_or(AlertConfigError::Threshold)?,
            },
            (
                AlertKind::UncertainOrders
                | AlertKind::ParkedItems
                | AlertKind::UndeliverableDestination,
                Some(count),
            ) if count > i64::MAX as u64 => {
                return Err(AlertConfigError::Threshold);
            }
            (AlertKind::UncertainOrders, Some(count)) => Condition::UncertainOrders(count),
            (AlertKind::ParkedItems, Some(count)) => Condition::ParkedItems(count),
            (AlertKind::UndeliverableDestination, Some(count)) => {
                Condition::UndeliverableDestination(count)
            }
            (AlertKind::RestorePaused, Some(_))
            | (
                AlertKind::OldestUndelivered
                | AlertKind::MachineSilent
                | AlertKind::DiskReserve
                | AlertKind::OrderCacheStale
                | AlertKind::UncertainOrders
                | AlertKind::ParkedItems
                | AlertKind::UndeliverableDestination,
                None,
            ) => return Err(AlertConfigError::Threshold),
        };
        Ok(Alert {
            condition,
            responder: self.responder,
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
            threshold: Some(threshold),
            responder: "lab duty officer".into(),
            machine: None,
        }
    }
    // Plan §11
    #[test]
    fn alert_kinds_and_thresholds_are_specific() {
        for kind in [
            "oldest_undelivered",
            "disk_reserve",
            "order_cache_stale",
            "uncertain_orders",
            "parked_items",
            "undeliverable_destination",
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
        let mut pause = draft("restore_paused", 1);
        pause.threshold = None;
        assert!(pause.validate(100).is_ok());
        for kind in [
            "oldest_undelivered",
            "disk_reserve",
            "order_cache_stale",
            "uncertain_orders",
            "parked_items",
            "undeliverable_destination",
        ] {
            let mut policy = draft(kind, 1);
            policy.threshold = None;
            assert_eq!(
                policy.validate(100).unwrap_err(),
                AlertConfigError::Threshold
            );
        }
        assert!(draft("uncertain_orders", 0).validate(100).is_ok());
        assert!(draft("undeliverable_destination", 0).validate(100).is_ok());
        assert_eq!(
            draft("undeliverable_destination", u64::MAX)
                .validate(100)
                .unwrap_err(),
            AlertConfigError::Threshold
        );
    }
    // Plan §11
    #[test]
    fn responder_is_required_bounded_and_single_line() {
        for responder in [
            String::new(),
            " ".into(),
            "x".repeat(257),
            "role\nsecond".into(),
        ] {
            let mut policy = draft("restore_paused", 1);
            policy.threshold = None;
            policy.responder = responder;
            assert_eq!(
                policy.validate(100).unwrap_err(),
                AlertConfigError::Responder
            );
        }
        assert!(toml::from_str::<DraftAlert>("kind='restore_paused'\nthreshold=1").is_err());
    }
}
