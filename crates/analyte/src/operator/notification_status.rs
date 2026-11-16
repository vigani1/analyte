//! Formats operational webhook health without payloads, endpoint URLs or credentials.
use analyte_store::AlertEndpointStatus;

pub(crate) fn describe(status: &AlertEndpointStatus, now: i64) -> String {
    let age = status
        .oldest
        .and_then(|time| now.checked_sub(time))
        .filter(|age| *age >= 0);
    format!(
        "alert_endpoint={} pending_count={} oldest_age_seconds={} last_success={} next_attempt={} exhausted={} queue_full={} failed={}",
        status.endpoint.as_str(),
        status.pending,
        known(age),
        known(status.last_success),
        known(status.next_attempt),
        status.exhausted,
        status.queue_full,
        status.failed
    )
}
fn known(value: Option<i64>) -> String {
    value.map_or_else(|| "unknown".into(), |n| n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0014_status_shows_failure_capacity_and_unknown_clock_without_secrets()
    -> Result<(), crate::Error> {
        let state = AlertEndpointStatus {
            endpoint: analyte_model::Identifier::new("relay")?,
            pending: 2,
            oldest: Some(100),
            last_success: Some(90),
            next_attempt: Some(130),
            exhausted: 1,
            queue_full: true,
            failed: 2,
        };
        let text = describe(&state, 120);
        for value in [
            "alert_endpoint=relay",
            "pending_count=2",
            "oldest_age_seconds=20",
            "last_success=90",
            "next_attempt=130",
            "exhausted=1",
            "queue_full=true",
            "failed=2",
        ] {
            assert!(text.contains(value));
        }
        assert!(describe(&state, 99).contains("oldest_age_seconds=unknown"));
        Ok(())
    }
}
