//! Serves `GET /status`: the Overview's overall state in one word and the program version, for the desktop app's tray, without signing in.
//!
//! It is public like the sign-in page, which already says the engine runs, and
//! gets the same Host and Origin checks and `no-store`. It says nothing else:
//! no names, counts, identifiers or values (safety rule 10). The word comes
//! from the Overview's own rules (`overview::Overall`), so the tray and the
//! verdict never disagree.
use super::Context;
use crate::admin::overview::{self, Overall};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use jiff::Timestamp;
use std::sync::Arc;

/// The whole answer: two fields and nothing more.
#[derive(serde::Serialize)]
struct Status {
    /// `good`, `attention` or `problem`, or `unknown` when the state could
    /// not be read in time.
    state: &'static str,
    /// This program's version, such as `0.1.0`.
    version: &'static str,
}

/// The answer when the state could not be read: the engine runs but cannot
/// say how things are, which the app shows as such and never as "All good".
const UNKNOWN: &str = "unknown";

pub(in crate::admin) async fn show(State(state): State<Arc<Context>>) -> Response {
    let overall = overview::overall(&state, Timestamp::now()).await;
    let status = if overall.is_some() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let answer = Status {
        state: overall.map_or(UNKNOWN, Overall::word),
        version: env!("CARGO_PKG_VERSION"),
    };
    (status, Json(answer)).into_response()
}
