//! Defines lab-app response documents for deliveries and order polling.
use serde::{Deserialize, Serialize};
/// Successful durable delivery outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Newly committed.
    Accepted,
    /// Exact bytes already committed.
    Duplicate,
}
/// Success body including the contract version.
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusBody {
    /// Must be 1.
    pub contract_version: u8,
    /// Durable result.
    pub status: DeliveryStatus,
}
/// Sender action; retries retain the original identity and bytes.
#[derive(Debug, PartialEq, Eq)]
pub enum ResponseAction {
    /// Delivery completed.
    Done(DeliveryStatus),
    /// Same ID has different content.
    Conflict,
    /// Operator intervention required.
    Parked {
        /// App reason, or a protocol error.
        reason: String,
    },
    /// Credentials or destination setup require repair; pauses the whole destination.
    PauseForSetup {
        /// Operator-visible cause.
        reason: &'static str,
    },
    /// Retry after the supplied delay, or normal backoff.
    Retry {
        /// Retry-After seconds.
        after: Option<u64>,
    },
}
/// Classifies ADR 0008 responses; malformed success is never acknowledged locally.
#[must_use]
pub fn classify_response(status: u16, body: &[u8], retry_after: Option<&str>) -> ResponseAction {
    match status {
        200 => match crate::decode::<StatusBody>(body) {
            Ok(value) if value.contract_version == 1 => ResponseAction::Done(value.status),
            _ => ResponseAction::Parked {
                reason: "invalid success response".into(),
            },
        },
        409 => ResponseAction::Conflict,
        401 | 403 => ResponseAction::PauseForSetup {
            reason: "destination credentials require operator repair",
        },
        // Every later delivery would fail the same way, so parking items one by
        // one would hide a wrong URL. The sender never exceeds the contract's
        // document limit, so 413 means the app's limit is below it.
        300..=399 => ResponseAction::PauseForSetup {
            reason: "app redirected the request; correct the destination URL",
        },
        404 => ResponseAction::PauseForSetup {
            reason: "app endpoint not found; correct the destination URL",
        },
        413 => ResponseAction::PauseForSetup {
            reason: "app body limit is below the contract document limit",
        },
        408 | 429 | 500..=599 => ResponseAction::Retry {
            after: retry_after.and_then(|s| retry_delay(s, jiff::Timestamp::now().as_second())),
        },
        400 | 422 => ResponseAction::Parked {
            reason: crate::decode::<crate::ErrorBody>(body)
                .ok()
                .filter(|v| v.contract_version == 1)
                .map_or_else(
                    || "invalid error response".into(),
                    |v| v.detail.as_str().to_owned(),
                ),
        },
        _ => ResponseAction::Parked {
            reason: format!("unexpected HTTP status {status}"),
        },
    }
}

/// The one Retry-After rule, for Analyte's sender and the contract checker:
/// delay seconds or an IMF-fixdate, at most 64 characters (contract v1).
fn retry_delay(value: &str, now: i64) -> Option<u64> {
    if value.len() > 64 {
        return None;
    }
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value.parse().ok();
    }
    let date = value.strip_suffix(" GMT")?;
    let timestamp =
        jiff::Timestamp::strptime("%a, %d %b %Y %H:%M:%S %z", format!("{date} +0000")).ok()?;
    Some(u64::try_from(timestamp.as_second().saturating_sub(now)).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    // ADR 0008
    #[test]
    fn retry_after_honors_seconds_and_http_date() {
        assert_eq!(super::retry_delay("120", 0), Some(120));
        assert_eq!(
            super::retry_delay("Thu, 01 Jan 1970 00:02:00 GMT", 0),
            Some(120)
        );
        assert_eq!(
            super::retry_delay("Thu, 01 Jan 1970 00:02:00 GMT", 180),
            Some(0)
        );
        assert_eq!(super::retry_delay("invalid", 0), None);
    }

    // Contract v1 "Delivery responses and retries", published vector retry-after.json
    #[test]
    fn retry_after_vector_parses_or_falls_back() -> Result<(), Box<dyn std::error::Error>> {
        let vector: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../docs/contract/vectors/retry-after.json"
        ))?;
        let now = vector["now"].as_i64().ok_or("now")?;
        for case in vector["cases"].as_array().ok_or("cases")? {
            let value = case["value"].as_str().ok_or("value")?;
            assert_eq!(
                super::retry_delay(value, now),
                case["requested_seconds"].as_u64(),
                "{value:?}"
            );
        }
        Ok(())
    }
}
