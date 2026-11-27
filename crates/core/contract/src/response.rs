//! Defines lab-app response documents for deliveries and order polling.
use serde::{Deserialize, Serialize};
/// Successful durable delivery outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Newly committed.
    Accepted,
    /// Exact bytes already committed.
    Duplicate,
}
impl DeliveryStatus {
    /// The status as a status body spells it: `accepted` or `duplicate`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Duplicate => "duplicate",
        }
    }
}
/// Success body including the contract version.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StatusBody {
    /// Must be 1.
    #[schemars(with = "crate::schema::VersionOne")]
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
    /// The app does not take Needs review notices: stop sending them to it,
    /// without pausing it or parking anything (ADR 0037). Only
    /// [`classify_notice_response`] gives this.
    Declined {
        /// Operator-visible cause.
        reason: &'static str,
    },
}

/// The reason kept when a lab app answers 404 for /review-notices.
pub const NOTICES_NOT_FOUND: &str =
    "the lab app has no /review-notices endpoint (404); Analyte stopped sending it Needs review notices";
/// The reason kept when a lab app answers 501 for /review-notices.
pub const NOTICES_NOT_IMPLEMENTED: &str =
    "the lab app does not implement /review-notices (501); Analyte stopped sending it Needs review notices";

/// Classifies an answer to `POST /review-notices`. Notices are optional for a
/// lab app, so 404 and 501 mean it takes none: [`ResponseAction::Declined`].
/// Every other answer means what it means for any delivery
/// ([`classify_response`]), so a wrong key still pauses the lab app.
#[must_use]
pub fn classify_notice_response(
    status: u16,
    body: &[u8],
    retry_after: Option<&str>,
) -> ResponseAction {
    match status {
        404 => ResponseAction::Declined {
            reason: NOTICES_NOT_FOUND,
        },
        501 => ResponseAction::Declined {
            reason: NOTICES_NOT_IMPLEMENTED,
        },
        _ => classify_response(status, body, retry_after),
    }
}
/// Why the response rules park a delivery when the reason is not the app's
/// own `detail`. Its `Display` is the stored reason, so spellings never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyProblem {
    /// HTTP 200 whose body is not a version 1 status body.
    InvalidSuccess,
    /// HTTP 400 or 422 whose body is not a version 1 error body.
    InvalidError,
    /// A status the contract gives no meaning for a delivery.
    UnexpectedStatus(u16),
}

const UNEXPECTED_OPEN: &str = "unexpected HTTP status ";

impl std::fmt::Display for ReplyProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSuccess => f.write_str("invalid success response"),
            Self::InvalidError => f.write_str("invalid error response"),
            Self::UnexpectedStatus(status) => write!(f, "{UNEXPECTED_OPEN}{status}"),
        }
    }
}

impl ReplyProblem {
    /// Reads back a reason written by `Display`, the exact reverse of it; any
    /// other text, such as an app's own detail, gives `None`.
    #[must_use]
    pub fn from_stored(text: &str) -> Option<Self> {
        let read = match text.strip_prefix(UNEXPECTED_OPEN) {
            Some(status) => Self::UnexpectedStatus(status.parse().ok()?),
            None => [Self::InvalidSuccess, Self::InvalidError]
                .into_iter()
                .find(|problem| problem.to_string() == text)?,
        };
        // Only the exact spelling reads back: no sign and no leading zero.
        (read.to_string() == text).then_some(read)
    }
}

/// Classifies ADR 0008 responses; malformed success is never acknowledged locally.
#[must_use]
pub fn classify_response(status: u16, body: &[u8], retry_after: Option<&str>) -> ResponseAction {
    match status {
        200 => match crate::decode::<StatusBody>(body) {
            Ok(value) if value.contract_version == 1 => ResponseAction::Done(value.status),
            _ => ResponseAction::Parked {
                reason: ReplyProblem::InvalidSuccess.to_string(),
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
                    || ReplyProblem::InvalidError.to_string(),
                    |v| v.detail.as_str().to_owned(),
                ),
        },
        _ => ResponseAction::Parked {
            reason: ReplyProblem::UnexpectedStatus(status).to_string(),
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

    // ADR 0008: the parked reasons the response rules write read back exactly;
    // the app's own detail, and any other spelling, does not.
    #[test]
    fn reply_problems_read_back_only_their_exact_spelling() {
        for (problem, stored) in [
            (
                super::ReplyProblem::InvalidSuccess,
                "invalid success response",
            ),
            (super::ReplyProblem::InvalidError, "invalid error response"),
            (
                super::ReplyProblem::UnexpectedStatus(418),
                "unexpected HTTP status 418",
            ),
            (
                super::ReplyProblem::UnexpectedStatus(0),
                "unexpected HTTP status 0",
            ),
        ] {
            assert_eq!(problem.to_string(), stored);
            assert_eq!(super::ReplyProblem::from_stored(stored), Some(problem));
        }
        for text in [
            "",
            "sample 12 is unknown",
            "unexpected HTTP status 0418",
            "unexpected HTTP status +418",
            "unexpected HTTP status 70000",
            "unexpected HTTP status 418 ",
            "invalid success response.",
        ] {
            assert_eq!(super::ReplyProblem::from_stored(text), None, "{text}");
        }
    }
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
