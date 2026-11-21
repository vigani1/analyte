//! Serves `GET /status`: the Overview's overall state in one word, the kinds of thing behind it and the program version, for the desktop app's tray and notifications, without signing in.
//!
//! It is public like the sign-in page, which already says the engine runs, and
//! gets the same Host and Origin checks and `no-store`. It says nothing else:
//! the kinds are words from a closed list (`overview::Reason`), never names,
//! counts, identifiers or values (safety rule 10, ADR 0023). Both come from
//! the Overview's own rules (`overview::Summary`), so the tray, a notification
//! and the verdict never disagree.
use super::Context;
use crate::admin::overview::{self, Reason};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jiff::Timestamp;
use std::sync::Arc;

/// The whole answer: three fields and nothing more.
#[derive(serde::Serialize)]
struct Status {
    /// `good`, `attention` or `problem`, or `unknown` when the state could
    /// not be read in time.
    state: &'static str,
    /// The kind of each thing that needs someone, each once, most serious
    /// first; empty when all is well or the state is unknown.
    reasons: Vec<&'static str>,
    /// This program's version, such as `0.1.0`.
    version: &'static str,
}

/// The answer when the state could not be read: the engine runs but cannot
/// say how things are, which the app shows as such and never as "All good".
const UNKNOWN: &str = "unknown";

pub(in crate::admin) async fn show(State(state): State<Arc<Context>>) -> Response {
    let summary = overview::summary(&state, Timestamp::now()).await;
    let (status, answer) = match summary {
        Some(summary) => (
            StatusCode::OK,
            Status {
                state: summary.overall.word(),
                reasons: summary.reasons.into_iter().map(Reason::word).collect(),
                version: env!("CARGO_PKG_VERSION"),
            },
        ),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Status {
                state: UNKNOWN,
                reasons: Vec::new(),
                version: env!("CARGO_PKG_VERSION"),
            },
        ),
    };
    (status, Json(answer)).into_response()
}
