//! Formats operational webhook health without payloads, endpoint URLs or credentials.
use crate::operator::Error;
use analyte_store::AlertEndpointStatus;

pub(crate) fn describe(status: &AlertEndpointStatus, now: i64) -> String {
    let age = status
        .oldest
        .and_then(|time| now.checked_sub(time))
        .filter(|age| *age >= 0);
    format!(
        "alert_endpoint={} pending_count={} oldest_age_seconds={} last_success={} next_attempt={} exhausted={} queue_full={} failed={} last_failure={}",
        status.endpoint.as_str(),
        status.pending,
        known(age),
        known(status.last_success),
        known(status.next_attempt),
        status.exhausted,
        status.queue_full,
        status.failed,
        status.last_failure.as_deref().unwrap_or("none")
    )
}
pub(crate) async fn snapshot(
    store: &analyte_store::Handle,
    configured: &[analyte_model::Identifier],
    now: i64,
) -> Result<Vec<String>, Error> {
    let statuses = store
        .request(crate::deadline()?, |s| s.alert_endpoint_status())
        .await?;
    let mut lines: Vec<_> = statuses
        .iter()
        .map(|status| describe(status, now))
        .collect();
    for endpoint in configured {
        if !statuses.iter().any(|status| &status.endpoint == endpoint) {
            lines.push(format!(
                "alert_endpoint={} state=unavailable pending_count=unknown",
                endpoint.as_str()
            ));
        }
    }
    Ok(lines)
}
fn known(value: Option<i64>) -> String {
    value.map_or_else(|| "unknown".into(), |n| n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    // ADR 0014
    #[test]
    fn status_shows_failure_and_capacity_without_secrets() -> Result<(), crate::Error> {
        let state = AlertEndpointStatus {
            endpoint: analyte_model::Identifier::new("relay")?,
            pending: 2,
            oldest: Some(100),
            last_success: Some(90),
            next_attempt: Some(130),
            exhausted: 1,
            queue_full: true,
            failed: 2,
            last_failure: Some("setup".into()),
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
            "last_failure=setup",
        ] {
            assert!(text.contains(value));
        }
        assert!(describe(&state, 99).contains("oldest_age_seconds=unknown"));
        Ok(())
    }
}
